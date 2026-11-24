//! Captura / restauración del layout de ventanas en Mac + snapshot para "Deshacer".
//! Mismo formato que Windows (WindowPlacementRec): proceso + ordinal + rect.
//! "Maximizado" no existe como estado en Mac (el botón verde es pantalla completa o
//! zoom), así que se guarda como normal con su rect.

use super::{apps, ax, placement};
use crate::profiles::{LayoutSnapshot, Rect, ShowState, WindowPlacementRec};
use std::collections::HashMap;

/// Ignorados al capturar: el Finder (escritorio) y la propia app.
const SKIP: &[&str] = &["Finder", "ContextSwitch"];

fn skip(name: &str) -> bool { SKIP.iter().any(|s| s.eq_ignore_ascii_case(name)) }

pub fn capture(now_iso: String) -> LayoutSnapshot {
    let mut windows = Vec::new();
    for app in apps::running().into_iter().filter(|a| !skip(&a.name)) {
        for (ordinal, (_, w)) in ax::windows_of_pid(app.pid).into_iter().enumerate() {
            let (Some(p), Some(s)) = (w.position(), w.size()) else { continue };
            windows.push(WindowPlacementRec {
                process_name: app.name.clone(),
                title_contains: None,
                ordinal: ordinal as u32,
                show: if w.is_minimized() { ShowState::Minimized } else { ShowState::Normal },
                rect: Rect { left: p.x as i32, top: p.y as i32, right: (p.x + s.w) as i32, bottom: (p.y + s.h) as i32 },
            });
        }
    }
    LayoutSnapshot { captured_at: now_iso, windows }
}

/// Aplica un layout. Devuelve el snapshot previo (para "Deshacer último cambio").
pub fn apply(target: &LayoutSnapshot) -> LayoutSnapshot {
    let before = capture("undo".into());
    let mut by_app: HashMap<String, Vec<ax::Element>> = HashMap::new();
    for app in apps::running() {
        by_app.entry(app.name.to_lowercase()).or_default()
            .extend(ax::windows_of_pid(app.pid).into_iter().map(|(_, w)| w));
    }
    for rec in &target.windows {
        // Layouts capturados en Windows traen "code.exe": se compara sin la extensión.
        let key = rec.process_name.trim_end_matches(".exe").to_lowercase();
        let Some(w) = by_app.get(&key).and_then(|l| l.get(rec.ordinal as usize)) else { continue };
        match rec.show {
            ShowState::Minimized => { w.set_minimized(true); }
            ShowState::Normal | ShowState::Maximized => {
                let r = &rec.rect;
                placement::place(w, placement::Rect {
                    left: r.left as f64, top: r.top as f64, right: r.right as f64, bottom: r.bottom as f64,
                });
            }
        }
    }
    before
}
