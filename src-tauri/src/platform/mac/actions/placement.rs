//! Posiciones directas en Mac: cada ventana del modo va a una zona de un monitor.
//! Igual que en Windows, la zona está en fracciones del área útil (acá: sin la barra de
//! menú ni el Dock) y los monitores van de izquierda a derecha.
//!
//! Coordenadas: las de Accesibilidad (puntos, origen arriba a la izquierda del monitor
//! principal, y hacia abajo). NSScreen usa origen ABAJO a la izquierda: se convierte.

use super::ax::{Element, Point, Size};
use crate::profiles::Placement;
use objc2_app_kit::NSScreen;
use objc2_foundation::MainThreadMarker;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorInfo {
    pub index: u32,
    pub primary: bool,
    pub left: i32, pub top: i32, pub width: i32, pub height: i32,
    pub work_left: i32, pub work_top: i32, pub work_width: i32, pub work_height: i32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect { pub left: f64, pub top: f64, pub right: f64, pub bottom: f64 }

/// Rect de Cocoa (origen abajo) → arriba-izquierda, dada la altura del monitor principal.
fn flip(x: f64, y: f64, w: f64, h: f64, primary_h: f64) -> (f64, f64, f64, f64) {
    (x, primary_h - (y + h), w, h)
}

pub fn monitors() -> Vec<MonitorInfo> { objc2::rc::autoreleasepool(|_| monitors_inner()) }

fn monitors_inner() -> Vec<MonitorInfo> {
    // NSScreen se declara "solo hilo principal", pero leer la lista de pantallas es
    // seguro desde cualquier hilo (no dibuja ni toca ventanas propias). Los comandos
    // de Tauri y el hilo worker la llaman fuera del principal.
    let mtm = unsafe { MainThreadMarker::new_unchecked() };
    let screens = NSScreen::screens(mtm);
    let Some(primary_h) = screens.iter().next().map(|s| s.frame().size.height) else { return vec![] };
    let mut v: Vec<MonitorInfo> = screens.iter().enumerate().map(|(i, s)| {
        let (f, w) = (s.frame(), s.visibleFrame());
        let (l, t, wd, ht) = flip(f.origin.x, f.origin.y, f.size.width, f.size.height, primary_h);
        let (wl, wt, ww, wh) = flip(w.origin.x, w.origin.y, w.size.width, w.size.height, primary_h);
        MonitorInfo {
            index: 0, primary: i == 0,
            left: l as i32, top: t as i32, width: wd as i32, height: ht as i32,
            work_left: wl as i32, work_top: wt as i32, work_width: ww as i32, work_height: wh as i32,
        }
    }).collect();
    v.sort_by_key(|m| (m.left, m.top));
    for (i, m) in v.iter_mut().enumerate() { m.index = i as u32; }
    v
}

/// Rect destino. Si ese monitor ya no está conectado, usa el principal.
pub fn target_rect(p: &Placement, mons: &[MonitorInfo]) -> Option<Rect> {
    let m = mons.get(p.monitor as usize).or_else(|| mons.iter().find(|m| m.primary))?;
    let clamp = |v: f64| v.clamp(0.0, 1.0);
    let (x, y) = (clamp(p.x), clamp(p.y));
    let (w, h) = (clamp(p.w).min(1.0 - x), clamp(p.h).min(1.0 - y));
    if w <= 0.0 || h <= 0.0 { return None; }
    let (wl, wt, ww, wh) = (m.work_left as f64, m.work_top as f64, m.work_width as f64, m.work_height as f64);
    Some(Rect {
        left: (wl + x * ww).round(), top: (wt + y * wh).round(),
        right: (wl + (x + w) * ww).round(), bottom: (wt + (y + h) * wh).round(),
    })
}

/// Mueve y redimensiona la ventana a `r`. Sin bordes invisibles que compensar (a
/// diferencia de Windows), pero sí apps con tamaño mínimo: si queda más grande que la
/// zona se corre hacia adentro para no salirse del monitor.
pub fn place(win: &Element, r: Rect) {
    if win.is_fullscreen() { return; } // en pantalla completa vive en su propio Space
    if win.is_minimized() { win.set_minimized(false); }
    let size = Size { w: r.right - r.left, h: r.bottom - r.top };
    // Posición → tamaño → posición: al pasar a otro monitor la ventana puede quedar
    // limitada por el tamaño del monitor anterior; la segunda posición corrige.
    win.set_position(Point { x: r.left, y: r.top });
    win.set_size(size);
    win.set_position(Point { x: r.left, y: r.top });
    if let Some(got) = win.size() {
        let dx = (got.w - size.w).max(0.0);
        let dy = (got.h - size.h).max(0.0);
        if dx > 0.5 || dy > 0.5 { win.set_position(Point { x: r.left - dx, y: r.top - dy }); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mon(index: u32, left: i32) -> MonitorInfo {
        MonitorInfo { index, primary: index == 0, left, top: 0, width: 1512, height: 982,
                      work_left: left, work_top: 33, work_width: 1512, work_height: 900 }
    }

    #[test]
    fn mitad_derecha_del_segundo_monitor_bajo_la_barra_de_menu() {
        let mons = [mon(0, 0), mon(1, 1512)];
        let r = target_rect(&Placement { monitor: 1, x: 0.5, y: 0.0, w: 0.5, h: 1.0 }, &mons).unwrap();
        assert_eq!(r, Rect { left: 2268.0, top: 33.0, right: 3024.0, bottom: 933.0 });
    }

    #[test]
    fn monitor_desconectado_usa_el_principal() {
        let r = target_rect(&Placement { monitor: 4, x: 0.0, y: 0.0, w: 0.25, h: 1.0 }, &[mon(0, 0)]).unwrap();
        assert_eq!((r.left, r.right), (0.0, 378.0));
    }

    #[test]
    fn de_cocoa_a_arriba_izquierda() {
        // Monitor principal de 982 de alto; ventana pegada abajo de 100 de alto → top = 882.
        assert_eq!(flip(0.0, 0.0, 300.0, 100.0, 982.0), (0.0, 882.0, 300.0, 100.0));
        // Monitor arriba del principal (y Cocoa positiva grande) → top negativo.
        assert_eq!(flip(0.0, 982.0, 1920.0, 1080.0, 982.0).1, -1080.0);
    }
}
