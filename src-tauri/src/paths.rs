//! Carpeta de datos de la app: %APPDATA%\ContextSwitch (Windows) o
//! ~/Library/Application Support/ContextSwitch (Mac).

use std::path::PathBuf;

pub fn data_dir() -> PathBuf {
    #[cfg(windows)]
    let base = std::env::var_os("APPDATA").map(PathBuf::from);
    #[cfg(not(windows))]
    let base = std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library").join("Application Support"));
    base.unwrap_or_else(|| ".".into()).join("ContextSwitch")
}
