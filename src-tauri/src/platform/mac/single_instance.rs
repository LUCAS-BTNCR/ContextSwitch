//! Una sola instancia en Mac (misma interfaz que single_instance_win.rs).
//!
//! Un socket Unix en la carpeta de datos del usuario hace de canal con la instancia que
//! corre: "show" = mostrá los Ajustes, "quit" = cerrate (la nueva toma su lugar).
//! Abrir la .app desde el Finder mientras corre no llega acá: macOS reusa el proceso y
//! avisa con RunEvent::Reopen (ver main.rs). Esto cubre `--activate` y `--autostart`.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

fn socket_path() -> PathBuf { crate::paths::data_dir().join("instance.sock") }

struct Channels { quit: Mutex<Receiver<()>>, show: Mutex<Receiver<()>> }
static CHANNELS: OnceLock<Channels> = OnceLock::new();

fn send(cmd: &str) -> bool {
    UnixStream::connect(socket_path())
        .and_then(|mut s| s.write_all(format!("{cmd}\n").as_bytes()))
        .is_ok()
}

/// Si ya corre una instancia, le pide que muestre los Ajustes y devuelve true (salir).
pub fn ask_running_to_show() -> bool { send("show") }

/// Toma el lugar de instancia única: si hay otra, le pide que se cierre y espera.
pub fn take_over() -> Option<isize> {
    let path = socket_path();
    if send("quit") {
        let start = Instant::now();
        while UnixStream::connect(&path).is_ok() && start.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    let _ = std::fs::remove_file(&path); // socket viejo (la otra ya salió o quedó colgada)
    if let Some(d) = path.parent() { let _ = std::fs::create_dir_all(d); }
    let listener = UnixListener::bind(&path).ok()?;
    let (quit_tx, quit_rx) = mpsc::channel();
    let (show_tx, show_rx) = mpsc::channel();
    CHANNELS.set(Channels { quit: Mutex::new(quit_rx), show: Mutex::new(show_rx) }).ok()?;
    std::thread::Builder::new().name("cs-instance".into()).spawn(move || {
        for conn in listener.incoming().map_while(Result::ok) {
            let mut line = String::new();
            let _ = BufReader::new(conn).read_line(&mut line);
            match line.trim() {
                "quit" => { let _ = quit_tx.send(()); }
                "show" => { let _ = show_tx.send(()); }
                _ => {}
            }
        }
    }).ok()?;
    Some(0)
}

pub fn show_event() -> Option<isize> { CHANNELS.get().map(|_| 1) }

pub fn wait_quit(_: isize) {
    // Sin canal (o si se cortara) se queda esperando para siempre: nunca cerrar por error.
    match CHANNELS.get() {
        Some(c) if c.quit.lock().unwrap().recv().is_ok() => {}
        _ => loop { std::thread::park() },
    }
}

pub fn wait_show(_: isize) {
    match CHANNELS.get() {
        Some(c) if c.show.lock().unwrap().recv().is_ok() => {}
        _ => loop { std::thread::park() },
    }
}
