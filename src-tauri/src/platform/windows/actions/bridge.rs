//! Lado app del puente local con Zen.
//!
//! Topología (100% local, sin sockets de red):
//!   ContextSwitch ──named pipe──▶ cs-bridge.exe ──stdio (Native Messaging)──▶ extensión Zen
//!
//! cs-bridge.exe lo lanza el propio navegador cuando la extensión llama connectNative.
//! El pipe se crea con PIPE_REJECT_REMOTE_CLIENTS y DACL por defecto (mismo usuario).

use std::{fs::OpenOptions, io::Write};

pub const PIPE_NAME: &str = r"\\.\pipe\contextswitch-zen";

pub use crate::actions::protocol::ZenCommand;

/// Espera hasta `ms` a que cs-bridge esté escuchando (la extensión conectada), sin
/// consumir la instancia del pipe.
pub fn wait_ready(ms: u32) -> bool {
    use windows::core::HSTRING;
    use windows::Win32::System::Pipes::WaitNamedPipeW;
    unsafe { WaitNamedPipeW(&HSTRING::from(PIPE_NAME), ms).as_bool() }
}

/// Err si la extensión no está corriendo (Zen cerrado) → el caller hace fallback.
pub fn send(cmd: &ZenCommand) -> anyhow::Result<()> {
    let mut pipe = OpenOptions::new().write(true).open(PIPE_NAME)?;
    let mut line = serde_json::to_string(cmd)?;
    line.push('\n');
    pipe.write_all(line.as_bytes())?;
    Ok(())
}
