//! Ícono de una .app como `data:image/png;base64,…` para la UI.
//! Solo lee el ícono (NSWorkspace iconForFile); no abre ni ejecuta la app.

use objc2::rc::autoreleasepool;
use objc2::AllocAnyThread;
use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSWorkspace};
use objc2_foundation::{NSDictionary, NSPoint, NSRect, NSSize, NSString};

/// Tamaño en puntos: la UI lo muestra a 24–28 px; el sistema lo dibuja a 2× en Retina.
const SIZE: f64 = 32.0;

pub fn exe_icon_data_url(app_path: &str) -> Option<String> {
    if app_path.is_empty() || !std::path::Path::new(app_path).exists() { return None; }
    // Los objetos de AppKit se liberan al salir del pool: sin él, en los hilos de Tauri
    // (que no tienen uno) cada ícono quedaba en memoria para siempre.
    let png = autoreleasepool(|_| {
        let image = NSWorkspace::sharedWorkspace().iconForFile(&NSString::from_str(app_path));
        // Se dibuja SOLO el tamaño necesario (antes se generaba un TIFF con todos los
        // tamaños del ícono, hasta 1024 px: varios MB por app).
        let mut rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(SIZE, SIZE));
        let cg = unsafe { image.CGImageForProposedRect_context_hints(&mut rect, None, None) }?;
        let rep = NSBitmapImageRep::initWithCGImage(NSBitmapImageRep::alloc(), &cg);
        let data = unsafe { rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new()) }?;
        Some(data.to_vec())
    })?;
    Some(format!("data:image/png;base64,{}", base64(&png)))
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        s.push(T[(n >> 18) as usize & 63] as char);
        s.push(T[(n >> 12) as usize & 63] as char);
        s.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        s.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    s
}

#[cfg(test)]
mod tests {
    #[test]
    fn base64_estandar() {
        assert_eq!(super::base64(b"Man"), "TWFu");
        assert_eq!(super::base64(b"Ma"), "TWE=");
        assert_eq!(super::base64(b"M"), "TQ==");
    }

    #[test]
    fn icono_de_una_app_del_sistema() {
        let url = super::exe_icon_data_url("/System/Applications/Calculator.app").expect("ícono");
        assert!(url.starts_with("data:image/png;base64,iVBOR"));
        assert!(url.len() < 40_000, "tamaño razonable: {}", url.len());
        assert!(super::exe_icon_data_url("/no/existe.app").is_none());
    }

    fn rss_kb() -> u64 {
        let out = std::process::Command::new("/bin/ps").args(["-o", "rss=", "-p", &std::process::id().to_string()]).output().unwrap();
        String::from_utf8_lossy(&out.stdout).trim().parse().unwrap()
    }

    #[test]
    fn pedir_iconos_muchas_veces_no_acumula_memoria() {
        // Igual que los comandos de Tauri: un hilo sin autorelease pool propio.
        std::thread::spawn(|| {
            let _ = super::exe_icon_data_url("/System/Applications/Calculator.app"); // calentar cachés
            let before = rss_kb();
            for _ in 0..40 { let _ = super::exe_icon_data_url("/System/Applications/Calculator.app"); }
            let grew_mb = rss_kb().saturating_sub(before) / 1024;
            assert!(grew_mb < 30, "creció {grew_mb} MB en 40 íconos (fuga)");
        }).join().unwrap();
    }
}
