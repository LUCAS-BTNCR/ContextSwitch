//! Posiciones directas: cada ventana del modo va a una zona de un monitor.
//! La zona se guarda en fracciones del área de trabajo (sin barra de tareas), así
//! sobrevive a cambios de resolución. Monitores ordenados de izquierda a derecha.

use crate::profiles::Placement;
use serde::Serialize;
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, RECT};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO};
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowRect, IsIconic, IsZoomed, SetWindowPos, ShowWindow, SWP_NOACTIVATE, SWP_NOOWNERZORDER,
    SWP_NOSIZE, SWP_NOZORDER, SW_RESTORE,
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorInfo {
    pub index: u32,
    pub primary: bool,
    /// Monitor completo (para dibujar la vista previa).
    pub left: i32, pub top: i32, pub width: i32, pub height: i32,
    /// Área de trabajo: el monitor sin la barra de tareas.
    pub work_left: i32, pub work_top: i32, pub work_width: i32, pub work_height: i32,
}

pub fn monitors() -> Vec<MonitorInfo> {
    unsafe extern "system" fn cb(h: HMONITOR, _: HDC, _: *mut RECT, lp: LPARAM) -> BOOL {
        let out = &mut *(lp.0 as *mut Vec<MonitorInfo>);
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        if GetMonitorInfoW(h, &mut mi).as_bool() {
            let (m, w) = (mi.rcMonitor, mi.rcWork);
            out.push(MonitorInfo {
                index: 0,
                primary: mi.dwFlags & 1 != 0, // MONITORINFOF_PRIMARY
                left: m.left, top: m.top, width: m.right - m.left, height: m.bottom - m.top,
                work_left: w.left, work_top: w.top, work_width: w.right - w.left, work_height: w.bottom - w.top,
            });
        }
        true.into()
    }
    let mut v: Vec<MonitorInfo> = Vec::new();
    unsafe { let _ = EnumDisplayMonitors(HDC::default(), None, Some(cb), LPARAM(&mut v as *mut _ as isize)); }
    v.sort_by_key(|m| (m.left, m.top));
    for (i, m) in v.iter_mut().enumerate() { m.index = i as u32; }
    v
}

/// Rect destino en píxeles. Si ese monitor ya no está conectado, usa el principal.
pub fn target_rect(p: &Placement, mons: &[MonitorInfo]) -> Option<RECT> {
    let m = mons.get(p.monitor as usize).or_else(|| mons.iter().find(|m| m.primary))?;
    let clamp = |v: f64| v.clamp(0.0, 1.0);
    let (x, y) = (clamp(p.x), clamp(p.y));
    let (w, h) = (clamp(p.w).min(1.0 - x), clamp(p.h).min(1.0 - y));
    if w <= 0.0 || h <= 0.0 { return None; }
    let left = m.work_left + (x * m.work_width as f64).round() as i32;
    let top = m.work_top + (y * m.work_height as f64).round() as i32;
    let right = m.work_left + ((x + w) * m.work_width as f64).round() as i32;
    let bottom = m.work_top + ((y + h) * m.work_height as f64).round() as i32;
    Some(RECT { left, top, right, bottom })
}

/// Borde invisible de Windows 10/11 (≈7 px a los lados y abajo): sin compensarlo
/// quedan huecos entre ventanas vecinas.
unsafe fn invisible_border(hwnd: HWND) -> (i32, i32, i32, i32) {
    let mut wr = RECT::default();
    let mut fr = RECT::default();
    if GetWindowRect(hwnd, &mut wr).is_err()
        || DwmGetWindowAttribute(hwnd, DWMWA_EXTENDED_FRAME_BOUNDS, &mut fr as *mut _ as *mut _,
                                 std::mem::size_of::<RECT>() as u32).is_err()
    {
        return (0, 0, 0, 0);
    }
    (fr.left - wr.left, fr.top - wr.top, wr.right - fr.right, wr.bottom - fr.bottom)
}

/// Mueve la ventana para que su borde VISIBLE coincida con `r`.
pub fn place(hwnd: HWND, r: RECT) {
    unsafe {
        if IsZoomed(hwnd).as_bool() || IsIconic(hwnd).as_bool() { let _ = ShowWindow(hwnd, SW_RESTORE); }
        // Dos pasadas: al cruzar a un monitor con otro DPI la ventana se reescala
        // (WM_DPICHANGED) y el borde invisible cambia; la segunda corrige.
        for _ in 0..2 {
            let (l, t, rr, b) = invisible_border(hwnd);
            let _ = SetWindowPos(
                hwnd, HWND::default(),
                r.left - l, r.top - t,
                (r.right - r.left) + l + rr, (r.bottom - r.top) + t + b,
                SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
            );
        }
        // Si la app tiene un ancho/alto mínimo mayor que la zona, queda más grande y se
        // saldría por la derecha/abajo (p. ej. fuera del monitor): se corre hacia adentro
        // para que su borde derecho/inferior coincida con el de la zona.
        let mut fr = RECT::default();
        if DwmGetWindowAttribute(hwnd, DWMWA_EXTENDED_FRAME_BOUNDS, &mut fr as *mut _ as *mut _,
                                 std::mem::size_of::<RECT>() as u32).is_ok() {
            let dx = (fr.right - r.right).max(0);
            let dy = (fr.bottom - r.bottom).max(0);
            if dx > 0 || dy > 0 {
                let mut wr = RECT::default();
                if GetWindowRect(hwnd, &mut wr).is_ok() {
                    let _ = SetWindowPos(hwnd, HWND::default(), wr.left - dx, wr.top - dy, 0, 0,
                                         SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_NOSIZE);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mon(index: u32, left: i32) -> MonitorInfo {
        MonitorInfo { index, primary: index == 0, left, top: 0, width: 1920, height: 1080,
                      work_left: left, work_top: 0, work_width: 1920, work_height: 1040 }
    }

    #[test]
    fn tres_columnas_en_el_segundo_monitor() {
        let mons = [mon(0, 0), mon(1, 1920)];
        let p = |x, w| Placement { monitor: 1, x, y: 0.0, w, h: 1.0 };
        let l = target_rect(&p(0.0, 0.25), &mons).unwrap();
        let c = target_rect(&p(0.25, 0.5), &mons).unwrap();
        let r = target_rect(&p(0.75, 0.25), &mons).unwrap();
        assert_eq!((l.left, l.right, l.bottom), (1920, 2400, 1040));
        assert_eq!((c.left, c.right), (2400, 3360));
        assert_eq!((r.left, r.right), (3360, 3840));
    }

    #[test]
    fn monitor_desconectado_usa_el_principal() {
        let mons = [mon(0, 0)];
        let r = target_rect(&Placement { monitor: 3, x: 0.5, y: 0.0, w: 0.5, h: 1.0 }, &mons).unwrap();
        assert_eq!((r.left, r.right), (960, 1920));
    }
}

/// Prueba real (crea una ventana propia y la mueve). `cargo test -- --ignored placement_real`
#[cfg(test)]
mod real {
    use super::*;
    use windows::core::w;
    use windows::Win32::UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
    use windows::Win32::UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow, WINDOW_EX_STYLE, WS_OVERLAPPEDWINDOW, WS_VISIBLE};

    #[test]
    #[ignore]
    fn placement_real() {
        unsafe {
            // Igual que la app (tao la declara per-monitor v2): coordenadas físicas.
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            let hwnd = CreateWindowExW(WINDOW_EX_STYLE(0), w!("STATIC"), w!("cs-placement-test"),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE, 100, 100, 400, 300, None, None, None, None).unwrap();
            let mons = monitors();
            println!("monitores: {}", mons.iter().map(|m| format!("#{} {}x{} work@{},{} {}x{}{}", m.index, m.width, m.height,
                m.work_left, m.work_top, m.work_width, m.work_height, if m.primary {" (principal)"} else {""})).collect::<Vec<_>>().join(" | "));
            for m in &mons {
                for (name, x, w) in [("izq 25%", 0.0, 0.25), ("centro 50%", 0.25, 0.5)] {
                    let want = target_rect(&Placement { monitor: m.index, x, y: 0.0, w, h: 1.0 }, &mons).unwrap();
                    place(hwnd, want);
                    let mut got = RECT::default();
                    DwmGetWindowAttribute(hwnd, DWMWA_EXTENDED_FRAME_BOUNDS, &mut got as *mut _ as *mut _, 16).unwrap();
                    println!("monitor {} {name}: pedido {:?} obtenido {:?}", m.index,
                             (want.left, want.top, want.right, want.bottom), (got.left, got.top, got.right, got.bottom));
                    for (a, b) in [(want.left, got.left), (want.top, got.top), (want.right, got.right), (want.bottom, got.bottom)] {
                        assert!((a - b).abs() <= 2, "desvío {a} vs {b}");
                    }
                }
            }
            let _ = DestroyWindow(hwnd);
        }
    }
}
