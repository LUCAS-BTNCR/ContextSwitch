//! Integración con Windows: autoarranque (HKCU\\Run), host de Native Messaging de
//! Zen/Firefox y ubicación de profiles.json.

use std::path::PathBuf;

/// Autoarranque vía HKCU\\...\\Run (sin admin, sin tareas programadas).
pub fn set_autostart(on: bool) {
    use windows::core::{w, HSTRING};
    use windows::Win32::System::Registry::{RegDeleteKeyValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ};
    let key = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
    unsafe {
        if on {
            let Ok(exe) = std::env::current_exe() else { return };
            let val = HSTRING::from(autostart_command(&exe));
            let bytes = (val.len() + 1) * 2;
            let _ = RegSetKeyValueW(HKEY_CURRENT_USER, key, w!("ContextSwitch"), REG_SZ.0,
                                    Some(val.as_ptr() as *const _), bytes as u32);
        } else {
            let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, key, w!("ContextSwitch"));
        }
    }
}

/// Comando del autoarranque: con `--autostart` la app sabe que la abrió Windows
/// (muestra la ventanita de modos) y no el usuario (muestra los Ajustes).
fn autostart_command(exe: &std::path::Path) -> String { format!("\"{}\" --autostart", exe.display()) }

/// Con "Iniciar con Windows" activado: crea la entrada si falta y migra la vieja
/// (sin `--autostart`) si apunta a ESTE exe. No toca una que apunte a otro exe.
pub fn refresh_autostart() {
    use windows::core::w;
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ};
    let Ok(exe) = std::env::current_exe() else { return };
    let mut buf = [0u16; 1024];
    let mut size = (buf.len() * 2) as u32;
    let r = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run"), w!("ContextSwitch"),
                     RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr() as *mut _), Some(&mut size))
    };
    if r.is_err() { set_autostart(true); return; } // falta (ej. la borró un desinstalador): restaurarla
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    let cur = String::from_utf16_lossy(&buf[..len]);
    let points_here = cur.to_lowercase().contains(&exe.display().to_string().to_lowercase());
    if points_here && cur != autostart_command(&exe) { set_autostart(true); }
}

/// Si hay un cs-bridge.exe junto al exe (instalación o carpeta portable), registra el host
/// de Native Messaging de Zen/Firefox apuntando a él. Idempotente; solo HKCU, sin admin.
/// En dev (target\debug) no hay bridge al lado y no se toca el registro existente.
pub fn register_zen_host() {
    use windows::core::{w, HSTRING};
    use windows::Win32::System::Registry::{RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ};
    let Some(bridge) = std::env::current_exe().ok()
        .and_then(|e| e.parent().map(|d| d.join("cs-bridge.exe")))
        .filter(|b| b.is_file()) else { return };
    let manifest_path = config_path().with_file_name("contextswitch_bridge.json");
    let manifest = serde_json::json!({
        "name": "contextswitch_bridge",
        "description": "ContextSwitch local bridge",
        "path": bridge.to_string_lossy(),
        "type": "stdio",
        "allowed_extensions": ["companion@contextswitch.app"],
    });
    if let Some(dir) = manifest_path.parent() { let _ = std::fs::create_dir_all(dir); }
    if std::fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest).unwrap_or_default()).is_err() { return; }
    let val = HSTRING::from(manifest_path.to_string_lossy().as_ref());
    unsafe {
        let _ = RegSetKeyValueW(HKEY_CURRENT_USER, w!("Software\\Mozilla\\NativeMessagingHosts\\contextswitch_bridge"),
                                None, REG_SZ.0, Some(val.as_ptr() as *const _), ((val.len() + 1) * 2) as u32);
    }
}

pub fn config_path() -> PathBuf { crate::paths::data_dir().join("profiles.json") }

/// En Windows no hace falta ningún permiso especial.
pub fn accessibility_ok() -> bool { true }
pub fn open_accessibility_settings() {}
