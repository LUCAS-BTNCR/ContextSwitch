//! Apartado Mac. Instalador: `installer/build.sh` (.app + .dmg, ver tauri.macos.conf.json).
//! Build: `npm run build:mac`.

pub mod actions;
pub mod hook;
pub mod keys;
pub mod single_instance;
mod system;

pub use system::*;
