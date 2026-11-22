//! Integración real de cs-bridge.exe: proceso + named pipe + framing de Native Messaging.
//!
//! Cada test usa un pipe PROPIO (`CS_BRIDGE_PIPE=\\.\pipe\contextswitch-test-…`) y un
//! %APPDATA% temporal, así nunca se conecta al cs-bridge real ni le manda nada a Zen.

#![cfg(windows)]

use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

static N: AtomicU32 = AtomicU32::new(0);

struct Bridge {
    child: Child,
    stdin: Option<ChildStdin>,
    frames: Receiver<String>,
    pipe: String,
    appdata: tempfile::TempDir,
}

impl Bridge {
    fn start() -> Self {
        let pipe = format!(r"\\.\pipe\contextswitch-test-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst));
        let appdata = tempfile::tempdir().unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_cs-bridge"))
            .env("CS_BRIDGE_PIPE", &pipe)
            .env("APPDATA", appdata.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("no arrancó cs-bridge");
        let stdin = child.stdin.take();
        let mut stdout = child.stdout.take().unwrap();
        let (tx, frames) = mpsc::channel();
        // Lector de frames u32 LE + JSON (lo que recibiría Zen).
        std::thread::spawn(move || loop {
            let mut len = [0u8; 4];
            if stdout.read_exact(&mut len).is_err() { break; }
            let mut body = vec![0u8; u32::from_le_bytes(len) as usize];
            if stdout.read_exact(&mut body).is_err() { break; }
            if tx.send(String::from_utf8(body).unwrap()).is_err() { break; }
        });
        Bridge { child, stdin, frames, pipe, appdata }
    }

    /// Conecta como ContextSwitch (reintenta hasta que el pipe exista) y escribe `data`.
    fn send(&self, data: &[u8]) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match OpenOptions::new().write(true).open(&self.pipe) {
                Ok(mut p) => { p.write_all(data).unwrap(); return; }
                Err(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
                Err(e) => panic!("el pipe {} no apareció: {e}", self.pipe),
            }
        }
    }

    fn next(&self) -> Option<String> { self.frames.recv_timeout(Duration::from_secs(3)).ok() }
    fn none_within(&self, ms: u64) -> bool { self.frames.recv_timeout(Duration::from_millis(ms)).is_err() }
}

impl Drop for Bridge {
    fn drop(&mut self) { let _ = self.child.kill(); let _ = self.child.wait(); }
}

#[test]
fn reenvia_cada_linea_json_con_framing_de_native_messaging() {
    let b = Bridge::start();
    b.send(b"{\"type\":\"unloadInactive\"}\n{\"type\":\"openWindows\",\"urls\":[\"https://a.com/\"]}\n");
    assert_eq!(b.next().as_deref(), Some(r#"{"type":"unloadInactive"}"#));
    assert_eq!(b.next().as_deref(), Some(r#"{"type":"openWindows","urls":["https://a.com/"]}"#));
}

#[test]
fn descarta_lineas_que_no_son_json_sin_cortar_las_siguientes() {
    let b = Bridge::start();
    b.send(b"no soy json\n{\"ok\":1}\n{roto\n{\"ok\":2}\n");
    assert_eq!(b.next().as_deref(), Some(r#"{"ok":1}"#));
    assert_eq!(b.next().as_deref(), Some(r#"{"ok":2}"#));
    assert!(b.none_within(300), "las líneas inválidas no llegan a Zen");
}

#[test]
fn descarta_mensajes_de_mas_de_64_kb() {
    let b = Bridge::start();
    let grande = format!("{{\"x\":\"{}\"}}\n", "a".repeat(70 * 1024));
    let mut data = grande.into_bytes();
    data.extend_from_slice(b"{\"chico\":true}\n");
    b.send(&data);
    assert_eq!(b.next().as_deref(), Some(r#"{"chico":true}"#), "solo pasa el mensaje acotado");
}

#[test]
fn acepta_una_conexion_nueva_despues_de_cerrar_la_anterior() {
    let b = Bridge::start();
    b.send(b"{\"n\":1}\n");
    assert_eq!(b.next().as_deref(), Some(r#"{"n":1}"#));
    b.send(b"{\"n\":2}\n"); // ContextSwitch abre y cierra el pipe en cada mensaje
    assert_eq!(b.next().as_deref(), Some(r#"{"n":2}"#));
    b.send(b"{\"n\":3}\n");
    assert_eq!(b.next().as_deref(), Some(r#"{"n":3}"#));
}

#[test]
fn los_mensajes_de_la_extension_van_al_log_local() {
    let mut b = Bridge::start();
    let msg = br#"{"type":"log","event":"connected","version":"test"}"#;
    let stdin = b.stdin.as_mut().unwrap();
    stdin.write_all(&(msg.len() as u32).to_le_bytes()).unwrap();
    stdin.write_all(msg).unwrap();
    stdin.flush().unwrap();
    let log = b.appdata.path().join("ContextSwitch").join("bridge.log");
    let deadline = Instant::now() + Duration::from_secs(3);
    while !log.exists() && Instant::now() < deadline { std::thread::sleep(Duration::from_millis(20)); }
    let content = std::fs::read_to_string(&log).expect("no se escribió bridge.log");
    assert!(content.contains(r#""event":"connected""#));
}

#[test]
fn termina_cuando_el_navegador_cierra_stdin() {
    let mut b = Bridge::start();
    drop(b.stdin.take()); // Zen cerró / extensión descargada
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(status) = b.child.try_wait().unwrap() { assert!(status.success()); return; }
        assert!(Instant::now() < deadline, "cs-bridge siguió vivo con stdin cerrado");
        std::thread::sleep(Duration::from_millis(20));
    }
}
