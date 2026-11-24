//! "Recordar cómo lo dejé" en Mac: foto al SALIR de un modo y restauración al VOLVER.
//! Se guarda en ~/Library/Application Support/ContextSwitch/sessions/<id-del-modo>.json
//! con el mismo formato que Windows.
//!
//! No se reabren: el Finder, la propia app ni los navegadores con ventanas por modo
//! (sus ventanas ya se guardan y reaparecen solas).

use super::{apps, ax, layout};
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

const NEVER_RELAUNCH: &[&str] = &["finder", "contextswitch", "zen", "system settings", "ajustes del sistema"];

pub fn relaunchable(process_name: &str, app_path: &str) -> bool {
    let p = process_name.to_lowercase();
    !NEVER_RELAUNCH.contains(&p.as_str())
        && app_path.ends_with(".app")
        && !app_path.starts_with("/System/") // apps del sistema: se abren solas cuando hacen falta
}

fn path_for(profile_id: &str) -> PathBuf {
    crate::paths::data_dir().join("sessions").join(file_name_for(profile_id))
}

/// El id del modo no puede salir de la carpeta (`..`, `/`).
fn file_name_for(profile_id: &str) -> String {
    let safe: String = profile_id.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect();
    format!("{safe}.json")
}

pub fn save(profile_id: &str) {
    let mut seen = HashSet::new();
    let list: Vec<SessionApp> = apps::running().into_iter()
        .filter(|a| !a.hidden && relaunchable(&a.name, &a.path))
        .filter(|a| !ax::windows_of_pid(a.pid).is_empty()) // solo las que tenían ventana
        .filter(|a| seen.insert(a.name.to_lowercase()))
        .map(|a| SessionApp { process_name: a.name, exe_path: a.path })
        .collect();
    let s = Session { captured_at: chrono::Local::now().to_rfc3339(), apps: list, layout: layout::capture("session".into()) };
    let _ = save_to(&path_for(profile_id), &s);
}

pub fn load(profile_id: &str) -> Option<Session> { load_from(&path_for(profile_id)) }

fn save_to(path: &std::path::Path, s: &Session) -> std::io::Result<()> {
    if let Some(d) = path.parent() { std::fs::create_dir_all(d)?; }
    std::fs::write(path, serde_json::to_string_pretty(s).map_err(std::io::Error::other)?)
}

fn load_from(path: &std::path::Path) -> Option<Session> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

pub fn processes(s: &Session) -> HashSet<String> {
    s.apps.iter().map(|a| a.process_name.to_lowercase()).collect()
}

/// Reabre las apps de la sesión que no están (o no tienen ventana) y restaura el layout.
pub fn restore(s: &Session) -> Vec<String> {
    let mut reopened = Vec::new();
    for a in &s.apps {
        if !relaunchable(&a.process_name, &a.exe_path) { continue; }
        let has_window = apps::find(&a.process_name).iter().any(|r| !r.hidden && !ax::windows_of_pid(r.pid).is_empty());
        if has_window { continue; }
        if apps::open(&a.exe_path, &[], &[], false) { reopened.push(a.process_name.clone()); }
    }
    if !reopened.is_empty() {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(12)
            && !reopened.iter().all(|p| apps::find(p).iter().any(|r| !ax::windows_of_pid(r.pid).is_empty()))
        {
            std::thread::sleep(Duration::from_millis(200));
        }
        std::thread::sleep(Duration::from_millis(1000));
    }
    layout::apply(&s.layout);
    reopened
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn que_se_reabre_y_que_no() {
        assert!(relaunchable("Visual Studio Code", "/Applications/Visual Studio Code.app"));
        assert!(!relaunchable("Finder", "/System/Library/CoreServices/Finder.app"));
        assert!(!relaunchable("Calculator", "/System/Applications/Calculator.app"));
        assert!(!relaunchable("Zen", "/Applications/Zen.app"));
        assert!(!relaunchable("algo", ""));
    }

    #[test]
    fn sesion_ida_y_vuelta_y_nombre_seguro() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("s").join(file_name_for("../../x"));
        assert!(!p.to_string_lossy().contains(".."));
        assert!(load_from(&p).is_none());
        let s = Session { captured_at: "t".into(), apps: vec![SessionApp { process_name: "Discord".into(), exe_path: "/Applications/Discord.app".into() }],
                          layout: LayoutSnapshot { captured_at: "t".into(), windows: vec![] } };
        save_to(&p, &s).unwrap();
        assert_eq!(load_from(&p).unwrap().apps[0].process_name, "Discord");
        assert!(processes(&s).contains("discord"));
    }
}
