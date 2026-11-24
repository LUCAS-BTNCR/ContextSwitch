//! Todo lo que es propio de cada sistema vive acá, en dos apartados:
//!
//! - `windows/`: Win32 (hook WH_KEYBOARD_LL, ventanas, Core Audio + IPolicyConfig,
//!   registro, named pipe) e `installer/` (hooks del instalador NSIS .exe).
//! - `mac/`: macOS (CGEventTap, Accesibilidad, NSWorkspace, Core Audio, LaunchAgent,
//!   socket Unix) e `installer/` (script del .app / .dmg).
//!
//! Los dos exponen la misma interfaz: `actions`, `hook`, `single_instance` y las
//! funciones de sistema (`config_path`, `set_autostart`, `register_zen_host`…).
//! El resto del código (main.rs, interfaz, motor del atajo, perfiles) es compartido.

#[cfg(windows)]
pub mod windows;
#[cfg(windows)]
pub use self::windows::*;

#[cfg(target_os = "macos")]
pub mod mac;
#[cfg(target_os = "macos")]
pub use self::mac::*;
