//! cs-bridge.exe — Native Messaging host para la extensión de Zen.
//!
//! - Lo lanza Zen (no ContextSwitch). Vive mientras la extensión mantenga el puerto.
//! - Servidor de named pipe local → reenvía cada línea JSON a stdout con el framing
//!   de Native Messaging (u32 LE + JSON).
//! - No abre red, no lee nada del navegador, no persiste nada.

#[cfg(windows)]
fn main() -> std::io::Result<()> {
    use std::fs::File;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::os::windows::io::FromRawHandle;
    use windows::core::HSTRING;
    use windows::Win32::Foundation::{CloseHandle, ERROR_PIPE_CONNECTED};
    use windows::Win32::Storage::FileSystem::PIPE_ACCESS_INBOUND;
    use windows::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS,
        PIPE_TYPE_BYTE, PIPE_WAIT,
    };

    const PIPE_NAME: &str = r"\\.\pipe\contextswitch-zen";
    const MAX_MSG: usize = 64 * 1024;

    // stdin: mensajes de la extensión (framing Native Messaging). Solo diagnóstico: se
    // anexan a %APPDATA%\ContextSwitch\bridge.log. Si el navegador cierra stdin → salir.
    std::thread::spawn(|| {
        let mut stdin = std::io::stdin();
        let log_path = std::env::var_os("APPDATA")
            .map(|a| std::path::PathBuf::from(a).join("ContextSwitch").join("bridge.log"));
        loop {
            let mut len = [0u8; 4];
            if stdin.read_exact(&mut len).is_err() { break; }
            let n = u32::from_le_bytes(len) as usize;
            if n > MAX_MSG { break; }
            let mut body = vec![0u8; n];
            if stdin.read_exact(&mut body).is_err() { break; }
            if let Some(p) = &log_path {
                // Tope de 256 KB: si se pasa, se empieza de nuevo.
                if std::fs::metadata(p).map(|m| m.len() > 256 * 1024).unwrap_or(false) {
                    let _ = std::fs::remove_file(p);
                }
                if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(p) {
                    let _ = f.write_all(&body);
                    let _ = f.write_all(b"\n");
                }
            }
        }
        std::process::exit(0);
    });

    let mut stdout = std::io::stdout().lock();
    loop {
        let pipe = unsafe {
            CreateNamedPipeW(
                &HSTRING::from(PIPE_NAME),
                PIPE_ACCESS_INBOUND,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                0,
                MAX_MSG as u32,
                0,
                None,
            )
        };
        if pipe.is_invalid() {
            return Err(std::io::Error::last_os_error());
        }
        unsafe {
            if let Err(e) = ConnectNamedPipe(pipe, None) {
                if e.code() != ERROR_PIPE_CONNECTED.to_hresult() {
                    // Cerrar la instancia: con nMaxInstances = 1 la próxima CreateNamedPipeW fallaría.
                    let _ = CloseHandle(pipe);
                    continue;
                }
            }
        }
        let file = unsafe { File::from_raw_handle(pipe.0 as _) }; // se cierra al hacer drop
        for line in BufReader::new(file).lines().map_while(Result::ok) {
            if line.len() > MAX_MSG || serde_json::from_str::<serde_json::Value>(&line).is_err() {
                continue; // solo JSON válido y acotado
            }
            stdout.write_all(&(line.len() as u32).to_le_bytes())?;
            stdout.write_all(line.as_bytes())?;
            stdout.flush()?;
        }
    }
}

#[cfg(not(windows))]
fn main() {}
