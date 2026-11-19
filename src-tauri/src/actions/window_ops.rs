//! Cierre amable, minimizado, foco y lanzamiento. Solo contratos públicos Win32.

use super::windows_enum::{windows_of, TopWindow};
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, GetForegroundWindow, GetWindowThreadProcessId, IsIconic, PostMessageW,
    SetForegroundWindow, ShowWindowAsync, SW_MINIMIZE, SW_RESTORE, SW_SHOWNORMAL, WM_CLOSE,
};

/// WM_CLOSE = lo mismo que clic en la X. La app decide: si hay cambios sin guardar,
/// muestra SU diálogo nativo. Nunca TerminateProcess, nunca timeouts que maten.
pub fn close_gracefully(process_name: &str) -> usize {
    if never_wm_close(process_name) { minimize(process_name); return 0; }
    let wins = windows_of(process_name);
    for w in &wins {
        unsafe { let _ = PostMessageW(w.hwnd, WM_CLOSE, WPARAM(0), LPARAM(0)); }
    }
    wins.len()
}

/// Minimiza (async: no se bloquea si la ventana está colgada).
/// Windows recorta el working set de apps minimizadas por su cuenta.
pub fn minimize(process_name: &str) {
    for w in windows_of(process_name) {
        unsafe { let _ = ShowWindowAsync(w.hwnd, SW_MINIMIZE); }
    }
}

/// Foco robusto: SetForegroundWindow tiene restricciones (foreground lock);
/// adjuntar temporalmente la cola de input al hilo en foreground es el método documentado.
pub fn focus(w: &TopWindow) {
    unsafe {
        if IsIconic(w.hwnd).as_bool() { let _ = ShowWindowAsync(w.hwnd, SW_RESTORE); }
        let fg = GetForegroundWindow();
        let fg_thread = GetWindowThreadProcessId(fg, None);
        let me = GetCurrentThreadId();
        let attached = fg_thread != 0 && fg_thread != me && AttachThreadInput(me, fg_thread, true).as_bool();
        let _ = BringWindowToTop(w.hwnd);
        let _ = SetForegroundWindow(w.hwnd);
        if attached { let _ = AttachThreadInput(me, fg_thread, false); }
    }
}

/// `focus` para una ventana identificada solo por su HWND.
pub fn focus_hwnd(hwnd: windows::Win32::Foundation::HWND) {
    focus(&TopWindow { hwnd, exe_path: String::new(), process_name: String::new(), title: String::new(), cloaked: false, pid: 0, class_name: String::new() });
}

/// Apps a las que NUNCA se les manda WM_CLOSE; se minimizan.
/// Zen: cerrar una ventana borra los Essentials que contiene (los sincroniza entre ventanas
/// y los cierra uno por uno con ella). Verificado en el SessionStore el 2026-11-19.
const NEVER_WM_CLOSE: &[&str] = &["zen.exe"];

pub fn never_wm_close(process_name: &str) -> bool {
    NEVER_WM_CLOSE.iter().any(|p| p.eq_ignore_ascii_case(process_name))
}

/// Cierra UNA ventana (WM_CLOSE), o la minimiza si es de una app protegida.
pub fn close_window_of(w: &TopWindow) {
    unsafe {
        if never_wm_close(&w.process_name) { let _ = ShowWindowAsync(w.hwnd, SW_MINIMIZE); }
        else { let _ = PostMessageW(w.hwnd, WM_CLOSE, WPARAM(0), LPARAM(0)); }
    }
}

pub fn minimize_hwnd(hwnd: windows::Win32::Foundation::HWND) {
    unsafe { let _ = ShowWindowAsync(hwnd, SW_MINIMIZE); }
}

/// Oculta la ventana sin cerrarla: desaparece de la pantalla y de la barra de tareas.
pub fn hide_hwnd(hwnd: windows::Win32::Foundation::HWND) {
    use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};
    // Sincrónico: al volver, la ventana ya está oculta (nadie más la puede enfocar en el medio).
    unsafe { let _ = ShowWindow(hwnd, SW_HIDE); }
}

/// Vuelve a mostrar una ventana oculta. `minimized`: aparece en la barra sin saltar al frente.
pub fn unhide_hwnd(hwnd: windows::Win32::Foundation::HWND, minimized: bool) {
    use windows::Win32::UI::WindowsAndMessaging::{IsWindowVisible, SW_SHOWMINNOACTIVE, SW_SHOWNA};
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() {
            let _ = windows::Win32::UI::WindowsAndMessaging::ShowWindow(hwnd, if minimized { SW_SHOWMINNOACTIVE } else { SW_SHOWNA });
        }
    }
}

pub fn shell_open(target: &str, args: &str) {
    // Si es un .exe, arranca en SU carpeta (como un acceso directo). Sin esto hereda la
    // carpeta de ContextSwitch y apps como Steam escriben sus logs/archivos ahí.
    let dir = std::path::Path::new(target);
    let workdir = (target.to_ascii_lowercase().ends_with(".exe") && dir.is_file())
        .then(|| dir.parent().map(|p| HSTRING::from(p.as_os_str())))
        .flatten();
    unsafe {
        let params = HSTRING::from(args);
        ShellExecuteW(
            None,
            &HSTRING::from("open"),
            &HSTRING::from(target),
            if args.is_empty() { PCWSTR::null() } else { PCWSTR(params.as_ptr()) },
            workdir.as_ref().map(|d| PCWSTR(d.as_ptr())).unwrap_or(PCWSTR::null()),
            SW_SHOWNORMAL,
        );
    }
}

/// Devuelve true si la app ya tenía ventana (y se enfocó).
pub fn launch_or_focus(process_name: &str, exe_path: &str, args: &[String], force_launch: bool) -> bool {
    if !force_launch {
        if let Some(w) = windows_of(process_name).first() {
            focus(w);
            return true;
        }
    }
    shell_open(exe_path, &quote_args(args));
    false
}

pub fn quote_args(args: &[String]) -> String {
    args.iter()
        .map(|a| {
            if a.is_empty() || a.contains([' ', '\t']) {
                // `"C:\dir\"` escaparía la comilla de cierre (reglas de CommandLineToArgvW):
                // se duplican las barras finales.
                let trailing = a.len() - a.trim_end_matches('\\').len();
                format!("\"{a}{}\"", "\\".repeat(trailing))
            } else {
                a.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::quote_args;
    fn q(v: &[&str]) -> String { quote_args(&v.iter().map(|s| s.to_string()).collect::<Vec<_>>()) }

    #[test]
    fn quoting() {
        assert_eq!(q(&["--new-window", "C:\\a"]), r"--new-window C:\a");
        assert_eq!(q(&[r"C:\Proyectos Personales\x"]), r#""C:\Proyectos Personales\x""#);
        assert_eq!(q(&[r"C:\Proyectos Personales\"]), r#""C:\Proyectos Personales\\""#);
        assert_eq!(q(&[""]), r#""""#);
    }
}
