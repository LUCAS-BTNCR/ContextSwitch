//! Máquina de estados del Chorded Double-Tap.
//!
//! Módulo PURO: sin dependencias de Win32, testeable en Linux/CI.
//! El hook (`hook.rs`) le pasa (vk, down/up, timestamp) y obedece la `Decision`.
//!
//! Garantías de privacidad (Zero-Keylogging):
//! - No hay buffers ni historial. El estado completo es: modificadores sostenidos
//!   (8 bits), la tecla disparadora sostenida (si es una registrada) y la fase actual.
//! - Toda tecla que no sea modificador ni disparadora registrada se descarta en O(n)
//!   sobre la tabla de chords (n = cantidad de perfiles, típicamente < 10).

pub type Vk = u32;
pub type ProfileIdx = u16;

// ── Modificadores lógicos ────────────────────────────────────────────────────
pub const MOD_CTRL: u8 = 1 << 0;
pub const MOD_SHIFT: u8 = 1 << 1;
pub const MOD_ALT: u8 = 1 << 2;
pub const MOD_WIN: u8 = 1 << 3;

// Virtual-key codes de modificadores (WH_KEYBOARD_LL reporta variantes L/R).
const VK_SHIFT: Vk = 0x10;
const VK_CONTROL: Vk = 0x11;
const VK_MENU: Vk = 0x12;
const VK_LWIN: Vk = 0x5B;
const VK_RWIN: Vk = 0x5C;
const VK_LSHIFT: Vk = 0xA0;
const VK_RSHIFT: Vk = 0xA1;
const VK_LCONTROL: Vk = 0xA2;
const VK_RCONTROL: Vk = 0xA3;
const VK_LMENU: Vk = 0xA4;
const VK_RMENU: Vk = 0xA5;

/// Bit físico (L/R por separado) para un VK de modificador.
fn physical_mod_bit(vk: Vk) -> Option<u8> {
    Some(match vk {
        VK_LCONTROL | VK_CONTROL => 1 << 0,
        VK_RCONTROL => 1 << 1,
        VK_LSHIFT | VK_SHIFT => 1 << 2,
        VK_RSHIFT => 1 << 3,
        VK_LMENU | VK_MENU => 1 << 4,
        VK_RMENU => 1 << 5,
        VK_LWIN => 1 << 6,
        VK_RWIN => 1 << 7,
        _ => return None,
    })
}

fn logical_mods(physical: u8) -> u8 {
    let mut m = 0;
    if physical & 0b0000_0011 != 0 { m |= MOD_CTRL; }
    if physical & 0b0000_1100 != 0 { m |= MOD_SHIFT; }
    if physical & 0b0011_0000 != 0 { m |= MOD_ALT; }
    if physical & 0b1100_0000 != 0 { m |= MOD_WIN; }
    m
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chord {
    pub mods: u8, // combinación EXACTA requerida (no superconjunto)
    pub vk: Vk,
}

#[derive(Clone, Copy, Debug)]
pub struct Binding {
    pub chord: Chord,
    pub profile: ProfileIdx,
}

#[derive(Clone, Copy, Debug)]
pub struct EngineConfig {
    /// Ventana máxima entre el 1er y el 2do KEYDOWN.
    pub window_ms: u32,
    /// Si es true, el chord queda reservado: la tecla disparadora no llega a la app
    /// en foco mientras se sostienen exactamente esos modificadores.
    pub swallow: bool,
}

impl Default for EngineConfig {
    fn default() -> Self { Self { window_ms: 500, swallow: true } }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fire {
    pub profile: ProfileIdx,
    pub delta_ms: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Decision {
    /// Devolver 1 desde el hook (la app en foco no ve el evento).
    pub swallow: bool,
    /// Inyectar una tecla "máscara" (vkE8) para que soltar Win/Alt no abra
    /// el menú Inicio / la barra de menú tras un disparador tragado.
    pub inject_mask: bool,
    pub fire: Option<Fire>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Idle,
    /// 1er KEYDOWN visto; esperamos su KEYUP.
    Down1 { b: usize, t0: u32 },
    /// 1er KEYUP visto; esperamos el 2do KEYDOWN dentro de la ventana.
    Up1 { b: usize, t0: u32 },
    /// Ya disparó; esperamos KEYUP para volver a Idle (los repeats se ignoran).
    Down2 { b: usize },
}

pub struct Engine {
    bindings: Vec<Binding>,
    cfg: EngineConfig,
    phys_mods: u8,
    /// Disparador registrado actualmente sostenido (para detectar auto-repeat).
    held_trigger: Option<Vk>,
    /// Si tragamos el KEYDOWN de esta tecla, tragamos sus repeats y su KEYUP
    /// (nunca dejamos un KEYUP huérfano en la app en foco).
    swallowed: Option<Vk>,
    phase: Phase,
}

impl Engine {
    pub fn new(bindings: Vec<Binding>, cfg: EngineConfig) -> Self {
        Self { bindings, cfg, phys_mods: 0, held_trigger: None, swallowed: None, phase: Phase::Idle }
    }

    /// Reemplaza la tabla en caliente (se llama desde el hilo del hook).
    pub fn reconfigure(&mut self, bindings: Vec<Binding>, cfg: EngineConfig) {
        self.bindings = bindings;
        self.cfg = cfg;
        self.phase = Phase::Idle;
    }

    fn mods(&self) -> u8 { logical_mods(self.phys_mods) }

    fn is_trigger_vk(&self, vk: Vk) -> bool {
        self.bindings.iter().any(|b| b.chord.vk == vk)
    }

    fn find(&self, mods: u8, vk: Vk) -> Option<usize> {
        self.bindings.iter().position(|b| b.chord.vk == vk && b.chord.mods == mods)
    }

    fn active_binding(&self) -> Option<usize> {
        match self.phase {
            Phase::Idle => None,
            Phase::Down1 { b, .. } | Phase::Up1 { b, .. } | Phase::Down2 { b } => Some(b),
        }
    }

    /// Punto de entrada único. `time_ms` = `KBDLLHOOKSTRUCT.time` (tick en ms, hace wrap).
    pub fn on_key(&mut self, vk: Vk, down: bool, time_ms: u32) -> Decision {
        if let Some(bit) = physical_mod_bit(vk) {
            return self.on_modifier(bit, down);
        }
        if down { self.on_trigger_down(vk, time_ms) } else { self.on_trigger_up(vk) }
    }

    fn on_modifier(&mut self, bit: u8, down: bool) -> Decision {
        if down { self.phys_mods |= bit } else { self.phys_mods &= !bit }
        // Si la combinación deja de ser exactamente la del chord en curso → reset.
        if let Some(b) = self.active_binding() {
            if self.mods() != self.bindings[b].chord.mods {
                self.phase = Phase::Idle;
            }
        }
        Decision::default() // los modificadores nunca se tragan
    }

    fn on_trigger_down(&mut self, vk: Vk, t: u32) -> Decision {
        // Auto-repeat: KEYDOWN de una tecla que ya estaba abajo.
        if self.held_trigger == Some(vk) {
            return Decision { swallow: self.swallowed == Some(vk), ..Default::default() };
        }

        // Descarte inmediato: no es disparador registrado → reset y pasa de largo.
        if !self.is_trigger_vk(vk) {
            self.phase = Phase::Idle;
            return Decision::default();
        }
        self.held_trigger = Some(vk);

        let mods = self.mods();
        let Some(b) = self.find(mods, vk) else {
            self.phase = Phase::Idle; // misma tecla, otros modificadores
            return Decision::default();
        };

        let mut fire = None;
        self.phase = match self.phase {
            Phase::Up1 { b: prev, t0 } if prev == b && t.wrapping_sub(t0) <= self.cfg.window_ms => {
                fire = Some(Fire { profile: self.bindings[b].profile, delta_ms: t.wrapping_sub(t0) });
                Phase::Down2 { b }
            }
            _ => Phase::Down1 { b, t0: t },
        };

        let swallow = self.cfg.swallow;
        if swallow { self.swallowed = Some(vk); }
        Decision {
            swallow,
            inject_mask: swallow && mods & (MOD_WIN | MOD_ALT) != 0,
            fire,
        }
    }

    fn on_trigger_up(&mut self, vk: Vk) -> Decision {
        if self.held_trigger == Some(vk) { self.held_trigger = None; }
        let swallow = self.swallowed == Some(vk);
        if swallow { self.swallowed = None; }

        self.phase = match self.phase {
            Phase::Down1 { b, t0 } if self.bindings[b].chord.vk == vk => Phase::Up1 { b, t0 },
            Phase::Down2 { b } if self.bindings[b].chord.vk == vk => Phase::Idle,
            p => p,
        };
        Decision { swallow, ..Default::default() }
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    const X: Vk = 0x58;
    const Y: Vk = 0x59;

    fn engine() -> Engine {
        Engine::new(
            vec![
                Binding { chord: Chord { mods: MOD_CTRL | MOD_SHIFT, vk: X }, profile: 7 },
                Binding { chord: Chord { mods: MOD_WIN, vk: Y }, profile: 3 },
            ],
            EngineConfig { window_ms: 500, swallow: true },
        )
    }

    fn hold_ctrl_shift(e: &mut Engine) {
        e.on_key(VK_LCONTROL, true, 0);
        e.on_key(VK_LSHIFT, true, 0);
    }

    #[test]
    fn double_tap_fires_on_second_down() {
        let mut e = engine();
        hold_ctrl_shift(&mut e);
        assert_eq!(e.on_key(X, true, 100).fire, None);
        e.on_key(X, false, 150);
        let d = e.on_key(X, true, 300);
        assert_eq!(d.fire, Some(Fire { profile: 7, delta_ms: 200 }));
        assert!(d.swallow);
        assert!(e.on_key(X, false, 350).swallow);
    }

    #[test]
    fn auto_repeat_never_fires() {
        let mut e = engine();
        hold_ctrl_shift(&mut e);
        e.on_key(X, true, 100);
        for t in (130..2000).step_by(33) {
            let d = e.on_key(X, true, t); // repeats
            assert_eq!(d.fire, None);
            assert!(d.swallow, "los repeats de una tecla tragada también se tragan");
        }
    }

    #[test]
    fn window_expired_restarts_sequence() {
        let mut e = engine();
        hold_ctrl_shift(&mut e);
        e.on_key(X, true, 100);
        e.on_key(X, false, 150);
        assert_eq!(e.on_key(X, true, 700).fire, None); // 600ms > 500
        e.on_key(X, false, 750);
        assert!(e.on_key(X, true, 900).fire.is_some()); // nueva 1ra tap = 700
    }

    #[test]
    fn releasing_modifier_resets() {
        let mut e = engine();
        hold_ctrl_shift(&mut e);
        e.on_key(X, true, 100);
        e.on_key(X, false, 150);
        e.on_key(VK_LSHIFT, false, 160);
        e.on_key(VK_LSHIFT, true, 170);
        assert_eq!(e.on_key(X, true, 200).fire, None);
    }

    #[test]
    fn exact_modifiers_required() {
        let mut e = engine();
        hold_ctrl_shift(&mut e);
        e.on_key(VK_LMENU, true, 0); // Ctrl+Shift+Alt ≠ Ctrl+Shift
        let d = e.on_key(X, true, 100);
        assert!(!d.swallow);
        e.on_key(X, false, 150);
        assert_eq!(e.on_key(X, true, 200).fire, None);
    }

    #[test]
    fn unrelated_key_in_between_resets_and_passes() {
        let mut e = engine();
        hold_ctrl_shift(&mut e);
        e.on_key(X, true, 100);
        e.on_key(X, false, 150);
        let d = e.on_key(0x41, true, 160); // 'A'
        assert_eq!(d, Decision::default());
        e.on_key(0x41, false, 170);
        assert_eq!(e.on_key(X, true, 200).fire, None);
    }

    #[test]
    fn left_and_right_modifiers_are_equivalent() {
        let mut e = engine();
        e.on_key(VK_RCONTROL, true, 0);
        e.on_key(VK_RSHIFT, true, 0);
        e.on_key(X, true, 100);
        e.on_key(X, false, 120);
        assert!(e.on_key(X, true, 200).fire.is_some());
    }

    #[test]
    fn win_chord_requests_mask() {
        let mut e = engine();
        e.on_key(VK_LWIN, true, 0);
        let d = e.on_key(Y, true, 10);
        assert!(d.swallow && d.inject_mask);
    }

    #[test]
    fn tick_wraparound() {
        let mut e = engine();
        hold_ctrl_shift(&mut e);
        e.on_key(X, true, u32::MAX - 50);
        e.on_key(X, false, u32::MAX - 10);
        let d = e.on_key(X, true, 100); // 151 ms reales
        assert_eq!(d.fire, Some(Fire { profile: 7, delta_ms: 151 }));
    }

    #[test]
    fn plain_typing_untouched() {
        let mut e = engine();
        for (i, vk) in [0x48u32, 0x4F, 0x4C, 0x41, X, X].iter().enumerate() {
            let t = i as u32 * 50;
            assert_eq!(e.on_key(*vk, true, t), Decision::default());
            assert_eq!(e.on_key(*vk, false, t + 20), Decision::default());
        }
    }

    #[test]
    fn swallow_disabled_passes_everything() {
        let mut e = engine();
        e.reconfigure(e.bindings.clone(), EngineConfig { window_ms: 500, swallow: false });
        hold_ctrl_shift(&mut e);
        e.on_key(X, true, 100);
        e.on_key(X, false, 150);
        let d = e.on_key(X, true, 200);
        assert!(d.fire.is_some() && !d.swallow);
    }
}
