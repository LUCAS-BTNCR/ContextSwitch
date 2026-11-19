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
    focus(&TopWindow { hwnd, exe_path: String::new(), process_name: String::new(), title: String::new(), cloaked: false });
}

pub fn shell_open(target: &str, args: &str) {
    unsafe {
        let params = HSTRING::from(args);
        ShellExecuteW(
            None,
            &HSTRING::from("open"),
            &HSTRING::from(target),
            if args.is_empty() { PCWSTR::null() } else { PCWSTR(params.as_ptr()) },
            PCWSTR::null(),
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
