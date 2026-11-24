//! Integración con macOS: inicio al iniciar sesión (LaunchAgent del usuario), host de
//! Native Messaging de Zen/Firefox, ubicación de profiles.json y permiso de Accesibilidad.
//! Todo por usuario, sin admin.

use std::path::{Path, PathBuf};

pub fn config_path() -> PathBuf { crate::paths::data_dir().join("profiles.json") }

const AGENT_LABEL: &str = "app.contextswitch";

fn agent_path() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/LaunchAgents").join(format!("{AGENT_LABEL}.plist")))
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// LaunchAgent que abre la app al iniciar sesión, con `--autostart` (así sabe que la
/// abrió el sistema y muestra la ventanita de modos en vez de los Ajustes).
fn agent_plist(exe: &Path) -> String {
    format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{AGENT_LABEL}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
        <string>--autostart</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>ProcessType</key>
    <string>Interactive</string>
</dict>
</plist>
"#, xml_escape(&exe.to_string_lossy()))
}

pub fn set_autostart(on: bool) {
    let Some(path) = agent_path() else { return };
    if on {
        let Ok(exe) = std::env::current_exe() else { return };
        if let Some(d) = path.parent() { let _ = std::fs::create_dir_all(d); }
        let _ = std::fs::write(&path, agent_plist(&exe));
    } else {
        let _ = std::fs::remove_file(&path);
    }
}

/// Con "Iniciar al iniciar sesión" activado: recrea el LaunchAgent si falta o si apunta
/// a otra copia de la app (por ejemplo, se movió la .app a /Applications).
pub fn refresh_autostart() {
    let (Some(path), Ok(exe)) = (agent_path(), std::env::current_exe()) else { return };
    if std::fs::read_to_string(&path).ok().as_deref() != Some(agent_plist(&exe).as_str()) { set_autostart(true); }
}

/// cs-bridge junto al ejecutable (dev / portable) o en Resources (dentro de la .app).
fn bridge_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    [dir.join("cs-bridge"), dir.join("../Resources/cs-bridge")].into_iter()
        .find(|p| p.is_file())
        .map(|p| p.canonicalize().unwrap_or(p))
}

/// Registra el host de Native Messaging de Zen/Firefox (un JSON por carpeta, sin admin).
/// Zen lo busca en la carpeta de Mozilla o en la suya según la versión: se escribe en las dos.
pub fn register_zen_host() {
    let (Some(bridge), Some(home)) = (bridge_path(), std::env::var_os("HOME").map(PathBuf::from)) else { return };
    let manifest = serde_json::json!({
        "name": "contextswitch_bridge",
        "description": "ContextSwitch local bridge",
        "path": bridge.to_string_lossy(),
        "type": "stdio",
        "allowed_extensions": ["companion@contextswitch.app"],
    });
    let body = serde_json::to_vec_pretty(&manifest).unwrap_or_default();
    for vendor in ["Mozilla", "zen"] {
        let dir = home.join("Library/Application Support").join(vendor).join("NativeMessagingHosts");
        if std::fs::create_dir_all(&dir).is_ok() {
            let _ = std::fs::write(dir.join("contextswitch_bridge.json"), &body);
        }
    }
}

pub fn accessibility_ok() -> bool { crate::actions::ax::trusted(false) }

/// Muestra el aviso de macOS (la primera vez) y abre Privacidad → Accesibilidad.
pub fn open_accessibility_settings() {
    crate::actions::ax::trusted(true);
    let _ = std::process::Command::new("/usr/bin/open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
        .status();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_launch_agent_abre_esta_app_con_autostart() {
        let p = agent_plist(Path::new("/Applications/ContextSwitch.app/Contents/MacOS/contextswitch"));
        assert!(p.contains("<string>/Applications/ContextSwitch.app/Contents/MacOS/contextswitch</string>"));
        assert!(p.contains("<string>--autostart</string>"));
        assert!(p.contains("<key>RunAtLoad</key>"));
    }

    #[test]
    fn rutas_con_caracteres_raros_no_rompen_el_plist() {
        let p = agent_plist(Path::new("/Users/a&b/<x>.app"));
        assert!(p.contains("/Users/a&amp;b/&lt;x&gt;.app"));
    }
}
