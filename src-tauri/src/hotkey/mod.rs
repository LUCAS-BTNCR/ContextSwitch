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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profiles::Config;

    fn cfg() -> Config { Config::default_config() } // trabajo = Ctrl+Shift+T, gaming = Ctrl+Shift+G

    #[test]
    fn un_binding_por_modo_habilitado_con_su_indice() {
        let (b, _) = bindings_from(&cfg());
        assert_eq!(b.len(), 2);
        assert_eq!((b[0].profile, b[0].chord.vk), (0, 0x54));
        assert_eq!((b[1].profile, b[1].chord.vk), (1, 0x47));
        assert_eq!(b[0].chord.mods, MOD_CTRL | MOD_SHIFT);
    }

    #[test]
    fn deshabilitados_y_sin_modificadores_no_generan_binding_pero_no_corren_indices() {
        let mut c = cfg();
        c.profiles[0].enabled = false;
        let (b, _) = bindings_from(&c);
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].profile, 1, "el índice es la posición real en profiles");

        let mut c = cfg();
        c.profiles[1].trigger.modifiers.clear();
        assert_eq!(bindings_from(&c).0.len(), 1, "sin modificadores sería escribir normal: se ignora");
    }

    #[test]
    fn todos_los_modificadores_se_traducen() {
        let mut c = cfg();
        c.profiles[0].trigger.modifiers = vec![Modifier::Ctrl, Modifier::Shift, Modifier::Alt, Modifier::Win];
        assert_eq!(bindings_from(&c).0[0].chord.mods, MOD_CTRL | MOD_SHIFT | MOD_ALT | MOD_WIN);
    }

    #[test]
    fn la_ventana_de_doble_toque_se_acota() {
        let mut c = cfg();
        c.settings.double_tap_window_ms = 10;
        assert_eq!(bindings_from(&c).1.window_ms, 150);
        c.settings.double_tap_window_ms = 99_999;
        assert_eq!(bindings_from(&c).1.window_ms, 1000);
        c.settings.double_tap_window_ms = 450;
        assert_eq!(bindings_from(&c).1.window_ms, 450);
    }

    #[test]
    fn reservar_combinacion_se_pasa_al_motor() {
        let mut c = cfg();
        c.settings.swallow_trigger = false;
        assert!(!bindings_from(&c).1.swallow);
    }
}
