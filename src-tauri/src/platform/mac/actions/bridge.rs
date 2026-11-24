//! Lado app del puente local con Zen, en Mac.
//!
//!   ContextSwitch ──socket Unix──▶ cs-bridge ──stdio (Native Messaging)──▶ extensión Zen
//!
//! El socket vive en la carpeta de datos del usuario (permisos 0600 del directorio del
//! usuario): ningún otro usuario ni la red llegan a él.

pub use crate::actions::protocol::ZenCommand;
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::{Duration, Instant};

pub fn socket_path() -> PathBuf { crate::paths::data_dir().join("zen-bridge.sock") }

/// Espera hasta `ms` a que cs-bridge esté escuchando (la extensión conectada).
/// Conectar y cerrar sin escribir no le manda nada a la extensión.
pub fn wait_ready(ms: u32) -> bool {
    let start = Instant::now();
    loop {
        if UnixStream::connect(socket_path()).is_ok() { return true; }
        if start.elapsed() >= Duration::from_millis(ms as u64) { return false; }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Err si la extensión no está corriendo (Zen cerrado).
pub fn send(cmd: &ZenCommand) -> anyhow::Result<()> {
    let mut s = UnixStream::connect(socket_path())?;
    let mut line = serde_json::to_string(cmd)?;
    line.push('\n');
    s.write_all(line.as_bytes())?;
    Ok(())
}
