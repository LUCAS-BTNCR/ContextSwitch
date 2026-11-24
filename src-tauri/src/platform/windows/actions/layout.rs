//! Captura / restauración de layout multi-monitor + snapshot para "Deshacer".
//! Get/SetWindowPlacement guardan estado normal/maximizado/minimizado y el rect
//! restaurado, en coordenadas de workspace: funciona bien entre monitores.

use super::windows_enum::{top_windows, TopWindow};
use crate::profiles::{LayoutSnapshot, Rect, ShowState, WindowPlacementRec};
use std::collections::HashMap;
use windows::Win32::Foundation::RECT;
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowPlacement, SetWindowPlacement, SHOW_WINDOW_CMD, SW_SHOWMAXIMIZED, SW_SHOWMINNOACTIVE,
    SW_SHOWNOACTIVATE, WINDOWPLACEMENT,
};

/// Ignorados al capturar (shell, la propia app).
const SKIP: &[&str] = &["explorer.exe", "contextswitch.exe", "textinputhost.exe", "applicationframehost.exe"];

fn placement(w: &TopWindow) -> Option<WINDOWPLACEMENT> {
    let mut wp = WINDOWPLACEMENT { length: std::mem::size_of::<WINDOWPLACEMENT>() as u32, ..Default::default() };
    unsafe { GetWindowPlacement(w.hwnd, &mut wp).ok()?; }
    Some(wp)
}

fn to_rec(w: &TopWindow, ordinal: u32, wp: &WINDOWPLACEMENT) -> WindowPlacementRec {
    let r = wp.rcNormalPosition;
    WindowPlacementRec {
        process_name: w.process_name.clone(),
        title_contains: None,
        ordinal,
        show: match SHOW_WINDOW_CMD(wp.showCmd as i32) {
            SW_SHOWMAXIMIZED => ShowState::Maximized,
            c if c.0 == 2 || c.0 == 6 || c.0 == 7 => ShowState::Minimized, // SW_SHOWMINIMIZED/SW_MINIMIZE/SW_SHOWMINNOACTIVE
            _ => ShowState::Normal,
        },
        rect: Rect { left: r.left, top: r.top, right: r.right, bottom: r.bottom },
    }
}

/// Snapshot de todas las ventanas actuales (ordinal por proceso).
pub fn capture(now_iso: String) -> LayoutSnapshot {
    let mut counters: HashMap<String, u32> = HashMap::new();
    let windows = top_windows()
        .into_iter()
        .filter(|w| !SKIP.contains(&w.process_name.as_str()))
        .filter_map(|w| {
            let wp = placement(&w)?;
            let n = counters.entry(w.process_name.clone()).or_insert(0);
            let rec = to_rec(&w, *n, &wp);
            *n += 1;
            Some(rec)
        })
        .collect();
    LayoutSnapshot { captured_at: now_iso, windows }
}

/// Aplica un layout. Devuelve el snapshot previo (para "Deshacer último cambio").
pub fn apply(target: &LayoutSnapshot) -> LayoutSnapshot {
    let before = capture(String::from("undo"));
    let current = top_windows();

    // Agrupar ventanas actuales por proceso, en orden de Z (orden de EnumWindows).
    let mut by_proc: HashMap<&str, Vec<&TopWindow>> = HashMap::new();
    for w in &current { by_proc.entry(w.process_name.as_str()).or_default().push(w); }

    for rec in &target.windows {
        let Some(list) = by_proc.get(rec.process_name.as_str()) else { continue };
        let candidate = match &rec.title_contains {
            Some(t) => list.iter().find(|w| w.title.contains(t.as_str())),
            None => list.get(rec.ordinal as usize),
        };
        let Some(w) = candidate else { continue };

        let mut wp = WINDOWPLACEMENT { length: std::mem::size_of::<WINDOWPLACEMENT>() as u32, ..Default::default() };
        wp.rcNormalPosition = RECT { left: rec.rect.left, top: rec.rect.top, right: rec.rect.right, bottom: rec.rect.bottom };
        wp.showCmd = match rec.show {
            ShowState::Maximized => SW_SHOWMAXIMIZED.0 as u32,
            ShowState::Minimized => SW_SHOWMINNOACTIVE.0 as u32,
            ShowState::Normal => SW_SHOWNOACTIVATE.0 as u32,
        };
        // Para maximizar en OTRO monitor: primero se fija el rect normal en ese monitor;
        // SetWindowPlacement maximiza en el monitor que contiene rcNormalPosition.
        unsafe { let _ = SetWindowPlacement(w.hwnd, &wp); }
    }
    before
}
