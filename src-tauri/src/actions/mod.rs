//! Acciones de un modo. La implementación es de cada sistema (platform/windows/actions,
//! platform/mac/actions) con la misma interfaz (`activate`, `audio`, `layout`,
//! `placement`, `icons`, `bridge`…); main.rs no sabe cuál usa.

pub mod protocol;

pub use crate::platform::actions::*;
