//! Apartado Windows. Instalador: `installer/` (NSIS, ver tauri.windows.conf.json).
//! Build: `npm run build:windows`. Prueba de punta a punta: `e2e.ps1`.

pub mod actions;
pub mod hook;
pub mod single_instance;
mod system;

pub use system::*;
