//! Integración real de cs-bridge en Mac: proceso + socket Unix + framing de Native Messaging.
//! Cada test usa un socket PROPIO (`CS_BRIDGE_SOCKET` en una carpeta temporal), así nunca
//! se conecta al cs-bridge real ni le manda nada a Zen.

#![cfg(unix)]

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn frame(stdout: &mut impl Read) -> String {
    let mut len = [0u8; 4];
    stdout.read_exact(&mut len).unwrap();
    let mut body = vec![0u8; u32::from_le_bytes(len) as usize];
    stdout.read_exact(&mut body).unwrap();
    String::from_utf8(body).unwrap()
}

#[test]
fn reenvia_lineas_json_con_framing_y_descarta_basura() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("b.sock");
    let mut child = Command::new(env!("CARGO_BIN_EXE_cs-bridge"))
        .env("CS_BRIDGE_SOCKET", &sock)
        .env("HOME", dir.path())
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null())
        .spawn().unwrap();
    let start = Instant::now();
    let mut s = loop {
        if let Ok(s) = UnixStream::connect(&sock) { break s; }
        assert!(start.elapsed() < Duration::from_secs(5), "cs-bridge no abrió el socket");
        std::thread::sleep(Duration::from_millis(50));
    };
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(std::fs::metadata(&sock).unwrap().permissions().mode() & 0o777, 0o600, "solo el usuario");

    s.write_all(b"no soy json\n{\"type\":\"unloadInactive\"}\n").unwrap();
    drop(s);
    let mut out = child.stdout.take().unwrap();
    assert_eq!(frame(&mut out), r#"{"type":"unloadInactive"}"#);

    // Otra conexión (la app conecta una vez por mensaje).
    let mut s = UnixStream::connect(&sock).unwrap();
    s.write_all(b"{\"type\":\"activate\",\"urls\":[]}\n").unwrap();
    drop(s);
    assert_eq!(frame(&mut out), r#"{"type":"activate","urls":[]}"#);

    // El navegador cierra stdin → cs-bridge sale y borra su socket.
    drop(child.stdin.take());
    let start = Instant::now();
    while child.try_wait().unwrap().is_none() {
        assert!(start.elapsed() < Duration::from_secs(5), "cs-bridge no salió al cerrarse stdin");
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(!sock.exists());
}
