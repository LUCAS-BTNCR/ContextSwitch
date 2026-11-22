//! Una sola instancia: la que se abre ÚLTIMA gana.
//!
//! - Mutex con nombre = "hay una instancia corriendo".
//! - Evento con nombre = canal "cerrate" hacia la instancia vieja.
//!
//! Al arrancar: si el mutex ya existe, se señala el evento y se espera a que la vieja
//! salga (libera el mutex). Si no responde en 5 s (colgada), se la termina.
//! Excepción: si la que corre es el MISMO exe (el usuario reabrió la app desde el menú
//! Inicio / acceso directo), no se reemplaza: se le pide que muestre los Ajustes
//! (evento "mostrate") y la nueva sale. Así "abrir de nuevo" trae la interfaz.
//! Todo es local a la sesión del usuario (prefijo `Local\`).

use windows::core::{w, PCWSTR};
use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE, WAIT_ABANDONED, WAIT_OBJECT_0};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{
    CreateEventW, CreateMutexW, GetCurrentProcessId, OpenEventW, OpenProcess, QueryFullProcessImageNameW,
    ResetEvent, SetEvent, TerminateProcess, WaitForSingleObject, EVENT_MODIFY_STATE, INFINITE,
    PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
};

const MUTEX_NAME: PCWSTR = w!("Local\\ContextSwitch.SingleInstance");
const QUIT_NAME: PCWSTR = w!("Local\\ContextSwitch.Quit");
const SHOW_NAME: PCWSTR = w!("Local\\ContextSwitch.Show");

/// Si ya corre una instancia de ESTE mismo exe y escucha el evento "mostrate", se lo
/// señala y devuelve true (el llamador debe salir). Si no hay otra, es otro exe
/// (portable vs. instalada) o es una versión vieja sin el evento → false.
pub fn ask_running_to_show() -> bool {
    unsafe {
        if !same_exe_running() { return false; }
        let Ok(ev) = OpenEventW(EVENT_MODIFY_STATE, false, SHOW_NAME) else { return false };
        let ok = SetEvent(ev).is_ok();
        let _ = CloseHandle(ev);
        ok
    }
}

/// Crea el evento "mostrate" de esta instancia (auto-reset) para esperarlo con [`wait_show`].
pub fn show_event() -> Option<isize> {
    unsafe { CreateEventW(None, false, false, SHOW_NAME).ok().map(|h| h.0 as isize) }
}

/// Bloquea hasta que otra copia del mismo exe pida mostrar la interfaz.
pub fn wait_show(ev: isize) {
    unsafe { WaitForSingleObject(HANDLE(ev as _), INFINITE); }
}

/// Toma el lugar de instancia única. Devuelve el evento "cerrate" de ESTA instancia
/// (como isize, para mandarlo a otro hilo) para esperarlo con [`wait_quit`].
pub fn take_over() -> Option<isize> {
    unsafe {
        let mutex = CreateMutexW(None, true, MUTEX_NAME).ok()?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            // Hay otra: pedirle que se cierre y esperar a quedarnos con el mutex.
            if let Ok(ev) = OpenEventW(EVENT_MODIFY_STATE, false, QUIT_NAME) {
                let _ = SetEvent(ev);
                let _ = CloseHandle(ev);
            }
            if !acquired(mutex, 5000) {
                kill_other_instances();
                acquired(mutex, 3000);
            }
        }
        // El mutex queda abierto (y nuestro) hasta que el proceso termina.
        let quit = CreateEventW(None, false, false, QUIT_NAME).ok()?;
        let _ = ResetEvent(quit);
        Some(quit.0 as isize)
    }
}

/// Bloquea hasta que otra instancia pida que esta se cierre.
pub fn wait_quit(quit: isize) {
    unsafe { WaitForSingleObject(HANDLE(quit as _), INFINITE); }
}

unsafe fn acquired(mutex: HANDLE, ms: u32) -> bool {
    let r = WaitForSingleObject(mutex, ms);
    r == WAIT_OBJECT_0 || r == WAIT_ABANDONED // abandonado = la vieja terminó sin liberarlo: es nuestro
}

/// ¿Hay otro proceso (otro PID) corriendo exactamente nuestro exe (misma ruta)?
unsafe fn same_exe_running() -> bool {
    let Some(me) = std::env::current_exe().ok() else { return false };
    let me_full = me.to_string_lossy().to_lowercase();
    let Some(me_name) = me.file_name().map(|n| n.to_string_lossy().to_lowercase()) else { return false };
    let my_pid = GetCurrentProcessId();
    let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return false };
    let mut pe = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
    let mut ok = Process32FirstW(snap, &mut pe).is_ok();
    let mut found = false;
    while ok && !found {
        let len = pe.szExeFile.iter().position(|&c| c == 0).unwrap_or(pe.szExeFile.len());
        let name = String::from_utf16_lossy(&pe.szExeFile[..len]).to_lowercase();
        if name == me_name && pe.th32ProcessID != my_pid {
            if let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pe.th32ProcessID) {
                let mut buf = [0u16; 1024];
                let mut n = buf.len() as u32;
                if QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut n).is_ok() {
                    found = String::from_utf16_lossy(&buf[..n as usize]).to_lowercase() == me_full;
                }
                let _ = CloseHandle(h);
            }
        }
        ok = Process32NextW(snap, &mut pe).is_ok();
    }
    let _ = CloseHandle(snap);
    found
}

/// Último recurso si la otra instancia está colgada: mismo nombre de exe, otro PID.
unsafe fn kill_other_instances() {
    let Some(me) = std::env::current_exe().ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase())) else { return };
    let my_pid = GetCurrentProcessId();
    let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return };
    let mut pe = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
    let mut ok = Process32FirstW(snap, &mut pe).is_ok();
    while ok {
        let len = pe.szExeFile.iter().position(|&c| c == 0).unwrap_or(pe.szExeFile.len());
        let name = String::from_utf16_lossy(&pe.szExeFile[..len]).to_lowercase();
        if name == me && pe.th32ProcessID != my_pid {
            if let Ok(h) = OpenProcess(PROCESS_TERMINATE, false, pe.th32ProcessID) {
                let _ = TerminateProcess(h, 1);
                let _ = CloseHandle(h);
            }
        }
        ok = Process32NextW(snap, &mut pe).is_ok();
    }
    let _ = CloseHandle(snap);
}
