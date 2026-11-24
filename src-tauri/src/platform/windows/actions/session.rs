//! "Recordar cómo lo dejé": foto del escritorio al SALIR de un modo y restauración al VOLVER.
//!
//! Se guarda en %APPDATA%\ContextSwitch\sessions\<id-del-modo>.json:
//! - qué apps tenían ventana (proceso + ruta del .exe, para reabrirlas si se cerraron),
//! - el layout de todas las ventanas (Get/SetWindowPlacement, ver layout.rs).
//!
//! No se reabren: el shell de Windows, la propia app, Zen (sus ventanas del modo ya se
//! ocultan/reaparecen con sus pestañas) ni programas de Windows (pueden pedir admin).

use super::{layout, window_ops, windows_enum};
use crate::profiles::LayoutSnapshot;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionApp { pub process_name: String, pub exe_path: String }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session { pub captured_at: String, pub apps: Vec<SessionApp>, pub layout: LayoutSnapshot }

const NEVER_RELAUNCH: &[&str] = &[
    "explorer.exe", "contextswitch.exe", "cs-bridge.exe", "zen.exe", "applicationframehost.exe",
    "shellexperiencehost.exe", "startmenuexperiencehost.exe", "searchhost.exe", "textinputhost.exe",
    "lockapp.exe", "systemsettings.exe",
];

/// ¿Se reabre esta app al volver al modo?
pub fn relaunchable(process_name: &str, exe_path: &str) -> bool {
    let p = process_name.to_lowercase();
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into()).to_lowercase();
    !NEVER_RELAUNCH.contains(&p.as_str())
        && !exe_path.is_empty()
        && !exe_path.to_lowercase().starts_with(&(windir + "\\"))
}

fn path_for(profile_id: &str) -> PathBuf {
    std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| ".".into())
        .join("ContextSwitch").join("sessions").join(file_name_for(profile_id))
}

/// Nombre de archivo seguro: el id del modo no puede salir de la carpeta (`..`, `\`, `:`).
fn file_name_for(profile_id: &str) -> String {
    let safe: String = profile_id.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect();
    format!("{safe}.json")
}

/// Foto del escritorio actual para `profile_id` (se llama al SALIR de ese modo).
pub fn save(profile_id: &str) {
    let mut seen = HashSet::new();
    let apps: Vec<SessionApp> = windows_enum::top_windows().into_iter()
        .filter(|w| !w.cloaked && relaunchable(&w.process_name, &w.exe_path))
        .filter(|w| seen.insert(w.process_name.clone()))
        .map(|w| SessionApp { process_name: w.process_name, exe_path: w.exe_path })
        .collect();
    let s = Session { captured_at: chrono::Local::now().to_rfc3339(), apps, layout: layout::capture("session".into()) };
    let _ = save_to(&path_for(profile_id), &s);
}

pub fn load(profile_id: &str) -> Option<Session> { load_from(&path_for(profile_id)) }

fn save_to(path: &std::path::Path, s: &Session) -> std::io::Result<()> {
    if let Some(d) = path.parent() { std::fs::create_dir_all(d)?; }
    std::fs::write(path, serde_json::to_string_pretty(s).map_err(std::io::Error::other)?)
}

/// Inexistente o dañada = sin sesión (al volver al modo no se restaura nada).
fn load_from(path: &std::path::Path) -> Option<Session> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/// Procesos de la sesión guardada: "Cerrar todo lo demás" no los cierra al volver.
pub fn processes(s: &Session) -> HashSet<String> {
    s.apps.iter().map(|a| a.process_name.to_lowercase()).collect()
}

/// Reabre las apps de la sesión que no tienen ventana y restaura el layout.
/// Devuelve los nombres de las apps reabiertas.
pub fn restore(s: &Session) -> Vec<String> {
    let mut reopened = Vec::new();
    for a in &s.apps {
        if !relaunchable(&a.process_name, &a.exe_path) { continue; }
        if windows_enum::windows_of(&a.process_name).iter().any(|w| !w.cloaked) { continue; }
        // Sin ventana visible (cerrada o en la bandeja): abrirla la muestra de nuevo.
        window_ops::shell_open(&a.exe_path, "");
        reopened.push(a.process_name.clone());
    }
    if !reopened.is_empty() {
        // Esperar a que aparezcan sus ventanas (máx. 12 s) y un momento más (splash screens).
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(12)
            && !reopened.iter().all(|p| !windows_enum::windows_of(p).is_empty())
        {
            std::thread::sleep(Duration::from_millis(200));
        }
        std::thread::sleep(Duration::from_millis(1500));
    }
    layout::apply(&s.layout);
    reopened
}

#[cfg(test)]
mod tests {
    use super::{file_name_for, load_from, processes, relaunchable, save_to, Session, SessionApp};
    use crate::profiles::LayoutSnapshot;

    fn sesion() -> Session {
        Session {
            captured_at: "2026-11-19T16:00:00-03:00".into(),
            apps: vec![SessionApp { process_name: "Code.exe".into(), exe_path: r"C:\x\Code.exe".into() }],
            layout: LayoutSnapshot { captured_at: "s".into(), windows: vec![] },
        }
    }

    #[test]
    fn sesion_ida_y_vuelta_en_disco() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("sessions").join("trabajo.json");
        assert!(load_from(&p).is_none(), "sin sesión guardada no se restaura nada");
        save_to(&p, &sesion()).unwrap();
        let s = load_from(&p).unwrap();
        assert_eq!(s.apps.len(), 1);
        assert_eq!(s.apps[0].exe_path, r"C:\x\Code.exe");
    }

    #[test]
    fn sesion_danada_es_como_no_tener_sesion() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("x.json");
        std::fs::write(&p, "{\"apps\": [").unwrap();
        assert!(load_from(&p).is_none());
    }

    #[test]
    fn el_id_del_modo_no_puede_salir_de_la_carpeta() {
        assert_eq!(file_name_for("trabajo"), "trabajo.json");
        assert_eq!(file_name_for("modo-1a2b"), "modo-1a2b.json");
        for malo in [r"..\..\Windows\x", "../x", "C:x", "a/b", "con|cat"] {
            let f = file_name_for(malo);
            assert!(!f.contains(['\\', '/', ':', '|']) && !f.contains(".."), "{malo} → {f}");
        }
    }

    #[test]
    fn procesos_de_la_sesion_en_minusculas() {
        assert!(processes(&sesion()).contains("code.exe"), "close_decision compara en minúsculas");
    }

    #[test]
    fn que_se_reabre_y_que_no() {
        assert!(relaunchable("code.exe", r"C:\Users\u\AppData\Local\Programs\Microsoft VS Code\Code.exe"));
        assert!(relaunchable("discord.exe", r"C:\Users\u\AppData\Local\Discord\app-1.0\Discord.exe"));
        assert!(!relaunchable("explorer.exe", r"C:\Windows\explorer.exe"));
        assert!(!relaunchable("zen.exe", r"C:\Program Files\Zen Browser\zen.exe"));
        assert!(!relaunchable("taskmgr.exe", r"C:\Windows\System32\Taskmgr.exe")); // de Windows: puede pedir admin
        assert!(!relaunchable("algo.exe", ""));
    }
}
