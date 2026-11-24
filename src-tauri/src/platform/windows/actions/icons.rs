//! Ícono de un .exe como `data:image/png;base64,…` para mostrarlo en la UI.
//! Solo lee el recurso de ícono del archivo (SHGetFileInfoW); no ejecuta nada.

use windows::core::HSTRING;
use windows::Win32::Graphics::Gdi::{
    DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO, BITMAPINFOHEADER,
    BI_RGB, DIB_RGB_COLORS, HBITMAP,
};
use windows::Win32::Foundation::HWND;
use windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES;
use windows::Win32::UI::Shell::{SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON};
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, ICONINFO};

pub fn exe_icon_data_url(exe_path: &str) -> Option<String> {
    let (w, h, rgba) = unsafe { icon_rgba(exe_path)? };
    let mut png_bytes = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut png_bytes, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut wr = enc.write_header().ok()?;
        wr.write_image_data(&rgba).ok()?;
    }
    Some(format!("data:image/png;base64,{}", base64(&png_bytes)))
}

unsafe fn icon_rgba(exe_path: &str) -> Option<(u32, u32, Vec<u8>)> {
    let mut info = SHFILEINFOW::default();
    let ok = SHGetFileInfoW(
        &HSTRING::from(exe_path),
        FILE_FLAGS_AND_ATTRIBUTES(0),
        Some(&mut info),
        std::mem::size_of::<SHFILEINFOW>() as u32,
        SHGFI_ICON | SHGFI_LARGEICON,
    );
    if ok == 0 || info.hIcon.is_invalid() { return None; }
    let hicon = info.hIcon;

    let mut ii = ICONINFO::default();
    let res = GetIconInfo(hicon, &mut ii).ok().and_then(|_| {
        let color = read_bgra(ii.hbmColor)?;
        let mask = if ii.hbmMask.is_invalid() { None } else { read_bgra(ii.hbmMask) };
        Some((color, mask))
    });
    if !ii.hbmColor.is_invalid() { let _ = DeleteObject(ii.hbmColor); }
    if !ii.hbmMask.is_invalid() { let _ = DeleteObject(ii.hbmMask); }
    let _ = DestroyIcon(hicon);

    let ((w, h, mut px), mask) = res?;
    // Íconos viejos sin canal alfa: la transparencia sale de la máscara (blanco = transparente).
    let has_alpha = px.as_chunks::<4>().0.iter().any(|p| p[3] != 0);
    if !has_alpha {
        for (i, p) in px.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let transparent = mask.as_ref()
                .filter(|(mw, mh, _)| *mw == w && *mh >= h)
                .map(|(_, _, m)| m[i * 4] != 0)
                .unwrap_or(false);
            p[3] = if transparent { 0 } else { 255 };
        }
    }
    for p in px.as_chunks_mut::<4>().0 { p.swap(0, 2); } // BGRA → RGBA
    Some((w, h, px))
}

/// Lee un HBITMAP como BGRA 32 bpp, de arriba hacia abajo.
unsafe fn read_bgra(bmp: HBITMAP) -> Option<(u32, u32, Vec<u8>)> {
    if bmp.is_invalid() { return None; }
    let mut bm = BITMAP::default();
    if GetObjectW(bmp, std::mem::size_of::<BITMAP>() as i32, Some(&mut bm as *mut _ as *mut _)) == 0 {
        return None;
    }
    let (w, h) = (bm.bmWidth, bm.bmHeight);
    if w <= 0 || h <= 0 || w > 512 || h > 512 { return None; }
    let mut bi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h, // top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut buf = vec![0u8; (w * h * 4) as usize];
    let hdc = GetDC(HWND::default());
    let lines = GetDIBits(hdc, bmp, 0, h as u32, Some(buf.as_mut_ptr() as *mut _), &mut bi, DIB_RGB_COLORS);
    ReleaseDC(HWND::default(), hdc);
    (lines == h).then_some((w as u32, h as u32, buf))
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn base64_rfc4648() {
        assert_eq!(super::base64(b""), "");
        assert_eq!(super::base64(b"f"), "Zg==");
        assert_eq!(super::base64(b"fo"), "Zm8=");
        assert_eq!(super::base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn notepad_icon() {
        let url = super::exe_icon_data_url(r"C:\Windows\System32\notepad.exe").expect("ícono");
        assert!(url.starts_with("data:image/png;base64,iVBOR"));
    }
}
