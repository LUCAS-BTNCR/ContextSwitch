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
        });
        true.into()
    }
    let mut v: Vec<TopWindow> = Vec::new();
    unsafe { let _ = EnumWindows(Some(cb), LPARAM(&mut v as *mut _ as isize)); }
    v
}

pub fn windows_of(process_name: &str) -> Vec<TopWindow> {
    let p = process_name.to_lowercase();
    top_windows().into_iter().filter(|w| w.process_name == p).collect()
}
