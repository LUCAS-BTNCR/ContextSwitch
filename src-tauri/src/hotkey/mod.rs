pub mod chord;
#[cfg(windows)]
pub mod hook;

use crate::profiles::{Config, Modifier};
use chord::{Binding, Chord, EngineConfig, MOD_ALT, MOD_CTRL, MOD_SHIFT, MOD_WIN};

/// Traduce `profiles.json` a la tabla compacta del motor.
/// El índice de perfil es la posición en `config.profiles`.
pub fn bindings_from(config: &Config) -> (Vec<Binding>, EngineConfig) {
    let bindings = config
        .profiles
        .iter()
        .enumerate()
        .filter(|(_, p)| p.enabled && !p.trigger.modifiers.is_empty())
        .map(|(i, p)| {
            let mods = p.trigger.modifiers.iter().fold(0u8, |acc, m| {
                acc | match m {
                    Modifier::Ctrl => MOD_CTRL,
                    Modifier::Shift => MOD_SHIFT,
                    Modifier::Alt => MOD_ALT,
                    Modifier::Win => MOD_WIN,
                }
            });
            Binding { chord: Chord { mods, vk: p.trigger.vk }, profile: i as u16 }
        })
        .collect();
    let cfg = EngineConfig {
        window_ms: config.settings.double_tap_window_ms.clamp(150, 1000),
        swallow: config.settings.swallow_trigger,
    };
    (bindings, cfg)
}
