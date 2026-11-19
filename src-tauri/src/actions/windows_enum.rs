//! Enumeración de ventanas top-level "de usuario" con su proceso.
//! Solo lee: HWND, nombre de ejecutable y título (este último solo para el layout).

use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, BOOL, HWND, LPARAM};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindow, GetWindowLongW, GetWindowTextW, GetWindowThreadProcessId,
    IsWindowVisible, GWL_EXSTYLE, GW_OWNER, WS_EX_TOOLWINDOW,
};

#[derive(Debug, Clone)]
pub struct TopWindow {
    pub hwnd: HWND,
    pub exe_path: String,
    pub process_name: String, // en minúsculas
    pub title: String,
    /// DWM la oculta aunque sea "visible": otro escritorio virtual, apps UWP suspendidas, etc.
    pub cloaked: bool,
    pub pid: u32,
    /// Clase de ventana (ej. "CabinetWClass" = carpeta del Explorador).
    pub class_name: String,
}

unsafe fn class_of(hwnd: HWND) -> String {
    use windows::Win32::UI::WindowsAndMessaging::GetClassNameW;
    let mut buf = [0u16; 128];
    let n = GetClassNameW(hwnd, &mut buf);
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

/// true si el proceso corre como administrador (token elevado). Windows (UIPI) no deja que
/// una app normal le mande mensajes como WM_CLOSE a una elevada.
pub fn is_elevated(pid: u32) -> bool {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
    use windows::Win32::System::Threading::OpenProcessToken;
    unsafe {
        let Ok(p) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return false };
        let mut tok = HANDLE::default();
        let mut elev = TOKEN_ELEVATION::default();
        let mut len = 0u32;
        let ok = OpenProcessToken(p, TOKEN_QUERY, &mut tok).is_ok()
            && GetTokenInformation(tok, TokenElevation, Some(&mut elev as *mut _ as *mut _),
                                   std::mem::size_of::<TOKEN_ELEVATION>() as u32, &mut len).is_ok();
        if !tok.is_invalid() { let _ = CloseHandle(tok); }
        let _ = CloseHandle(p);
        ok && elev.TokenIsElevated != 0
    }
}

unsafe fn is_cloaked(hwnd: HWND) -> bool {
    use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
    let mut v: u32 = 0;
    DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, &mut v as *mut _ as *mut _, 4).is_ok() && v != 0
}

pub fn process_path(pid: u32) -> Option<String> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
        let _ = CloseHandle(h);
        ok.then(|| String::from_utf16_lossy(&buf[..len as usize]))
    }
}

pub fn top_windows() -> Vec<TopWindow> {
    unsafe extern "system" fn cb(hwnd: HWND, lp: LPARAM) -> BOOL {
        let out = &mut *(lp.0 as *mut Vec<TopWindow>);
        if !IsWindowVisible(hwnd).as_bool() { return true.into(); }
        if GetWindow(hwnd, GW_OWNER).map(|o| !o.0.is_null()).unwrap_or(false) { return true.into(); }
        if (GetWindowLongW(hwnd, GWL_EXSTYLE) as u32) & WS_EX_TOOLWINDOW.0 != 0 { return true.into(); }

        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let Some(exe_path) = process_path(pid) else { return true.into() };
        let process_name = exe_path.rsplit('\\').next().unwrap_or("").to_lowercase();

        let mut t = [0u16; 512];
        let n = GetWindowTextW(hwnd, &mut t);
        if n == 0 { return true.into(); } // ventanas sin título suelen ser auxiliares
        out.push(TopWindow {
            hwnd, exe_path, process_name,
            title: String::from_utf16_lossy(&t[..n as usize]),
            cloaked: is_cloaked(hwnd),
            pid,
            class_name: class_of(hwnd),
        });
        true.into()
    }
    let mut v: Vec<TopWindow> = Vec::new();
    unsafe { let _ = EnumWindows(Some(cb), LPARAM(&mut v as *mut _ as isize)); }
    v
}

/// true si `hwnd` sigue siendo una ventana de `process_name`, aunque esté OCULTA
/// (top_windows solo lista las visibles).
pub fn is_window_of(hwnd: HWND, process_name: &str) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::IsWindow;
    unsafe {
        if !IsWindow(hwnd).as_bool() { return false; }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        process_path(pid)
            .map(|p| p.rsplit('\\').next().unwrap_or("").eq_ignore_ascii_case(process_name))
            .unwrap_or(false)
    }
}

/// ¿Hay algún proceso con ese nombre corriendo (tenga o no ventanas visibles)?
pub fn process_running(process_name: &str) -> bool {
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
    };
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return false };
        let mut pe = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut ok = Process32FirstW(snap, &mut pe).is_ok();
        let mut found = false;
        while ok && !found {
            let len = pe.szExeFile.iter().position(|&c| c == 0).unwrap_or(pe.szExeFile.len());
            found = String::from_utf16_lossy(&pe.szExeFile[..len]).eq_ignore_ascii_case(process_name);
            ok = Process32NextW(snap, &mut pe).is_ok();
        }
        let _ = CloseHandle(snap);
        found
    }
}

pub fn windows_of(process_name: &str) -> Vec<TopWindow> {
    let p = process_name.to_lowercase();
    top_windows().into_iter().filter(|w| w.process_name == p).collect()
}
