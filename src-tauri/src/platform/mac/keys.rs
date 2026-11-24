//! Teclado de Mac → códigos de Windows (puro, testeable en cualquier sistema).
//!
//! El motor de chords y profiles.json hablan en Virtual-Key de Windows. En Mac, el hook
//! traduce el keycode físico (kVK_*) al VK equivalente, así la misma configuración sirve
//! en las dos PCs. Igual que en Windows, el código es posicional (independiente del
//! layout del teclado), y coincide con lo que la UI calcula desde `KeyboardEvent.code`.

/// kVK_* (Carbon, HIToolbox/Events.h) → VK de Windows. None = tecla que no se usa como disparadora.
pub fn mac_keycode_to_vk(code: u16) -> Option<u32> {
    Some(match code {
        // Letras
        0x00 => 0x41, 0x0B => 0x42, 0x08 => 0x43, 0x02 => 0x44, 0x0E => 0x45, 0x03 => 0x46,
        0x05 => 0x47, 0x04 => 0x48, 0x22 => 0x49, 0x26 => 0x4A, 0x28 => 0x4B, 0x25 => 0x4C,
        0x2E => 0x4D, 0x2D => 0x4E, 0x1F => 0x4F, 0x23 => 0x50, 0x0C => 0x51, 0x0F => 0x52,
        0x01 => 0x53, 0x11 => 0x54, 0x20 => 0x55, 0x09 => 0x56, 0x0D => 0x57, 0x07 => 0x58,
        0x10 => 0x59, 0x06 => 0x5A,
        // Números de la fila superior
        0x1D => 0x30, 0x12 => 0x31, 0x13 => 0x32, 0x14 => 0x33, 0x15 => 0x34,
        0x17 => 0x35, 0x16 => 0x36, 0x1A => 0x37, 0x1C => 0x38, 0x19 => 0x39,
        // Teclado numérico
        0x52 => 0x60, 0x53 => 0x61, 0x54 => 0x62, 0x55 => 0x63, 0x56 => 0x64,
        0x57 => 0x65, 0x58 => 0x66, 0x59 => 0x67, 0x5B => 0x68, 0x5C => 0x69,
        // F1–F20
        0x7A => 0x70, 0x78 => 0x71, 0x63 => 0x72, 0x76 => 0x73, 0x60 => 0x74, 0x61 => 0x75,
        0x62 => 0x76, 0x64 => 0x77, 0x65 => 0x78, 0x6D => 0x79, 0x67 => 0x7A, 0x6F => 0x7B,
        0x69 => 0x7C, 0x6B => 0x7D, 0x71 => 0x7E, 0x6A => 0x7F, 0x40 => 0x80, 0x4F => 0x81,
        0x50 => 0x82, 0x5A => 0x83,
        // Puntuación (OEM), mismas equivalencias que keymap.ts
        0x29 => 0xBA, 0x18 => 0xBB, 0x2B => 0xBC, 0x1B => 0xBD, 0x2F => 0xBE, 0x2C => 0xBF,
        0x32 => 0xC0, 0x21 => 0xDB, 0x2A => 0xDC, 0x1E => 0xDD, 0x27 => 0xDE, 0x0A => 0xE2,
        0x31 => 0x20, 0x24 => 0x0D, 0x30 => 0x09, 0x33 => 0x08,
        // Modificadores: ⌘ = Win, ⌥ = Alt (izquierdo/derecho por separado, como el hook de Windows)
        0x3B => 0xA2, 0x3E => 0xA3, // Control
        0x38 => 0xA0, 0x3C => 0xA1, // Shift
        0x3A => 0xA4, 0x3D => 0xA5, // Option
        0x37 => 0x5B, 0x36 => 0x5C, // Command
        _ => return None,
    })
}

/// Para un evento "flagsChanged" (se apretó o soltó un modificador): ¿quedó apretado?
/// Se mira el bit del lado exacto (NX_DEVICE*KEYMASK), no el genérico: con ambos Shift
/// apretados, soltar uno no tiene que soltar el otro.
pub fn modifier_down(code: u16, flags: u64) -> Option<bool> {
    let bit: u64 = match code {
        0x3B => 0x0000_0001, // NX_DEVICELCTLKEYMASK
        0x3E => 0x0000_2000, // NX_DEVICERCTLKEYMASK
        0x38 => 0x0000_0002, // NX_DEVICELSHIFTKEYMASK
        0x3C => 0x0000_0004, // NX_DEVICERSHIFTKEYMASK
        0x37 => 0x0000_0008, // NX_DEVICELCMDKEYMASK
        0x36 => 0x0000_0010, // NX_DEVICERCMDKEYMASK
        0x3A => 0x0000_0020, // NX_DEVICELALTKEYMASK
        0x3D => 0x0000_0040, // NX_DEVICERALTKEYMASK
        _ => return None,    // Caps Lock, fn: no son modificadores del chord
    };
    Some(flags & bit != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letras_y_numeros_coinciden_con_los_vk_de_windows() {
        assert_eq!(mac_keycode_to_vk(0x11), Some(0x54)); // T
        assert_eq!(mac_keycode_to_vk(0x05), Some(0x47)); // G
        assert_eq!(mac_keycode_to_vk(0x00), Some(0x41)); // A
        assert_eq!(mac_keycode_to_vk(0x06), Some(0x5A)); // Z
        assert_eq!(mac_keycode_to_vk(0x12), Some(0x31)); // 1
        assert_eq!(mac_keycode_to_vk(0x1D), Some(0x30)); // 0
        assert_eq!(mac_keycode_to_vk(0x7A), Some(0x70)); // F1
    }

    #[test]
    fn las_26_letras_son_distintas_y_cubren_a_z() {
        let mut vks: Vec<u32> = (0u16..0x80).filter_map(mac_keycode_to_vk).filter(|v| (0x41..=0x5A).contains(v)).collect();
        vks.sort();
        vks.dedup();
        assert_eq!(vks, (0x41..=0x5A).collect::<Vec<_>>());
    }

    #[test]
    fn modificadores_mapean_a_los_vk_izquierdo_y_derecho() {
        assert_eq!(mac_keycode_to_vk(0x3B), Some(0xA2)); // ⌃ izq
        assert_eq!(mac_keycode_to_vk(0x3D), Some(0xA5)); // ⌥ der
        assert_eq!(mac_keycode_to_vk(0x37), Some(0x5B)); // ⌘ izq = Win
    }

    #[test]
    fn modificador_apretado_segun_su_propio_lado() {
        let lshift_y_rshift = 0x02 | 0x04;
        assert_eq!(modifier_down(0x38, lshift_y_rshift), Some(true));
        assert_eq!(modifier_down(0x38, 0x04), Some(false), "soltar Shift izq con el der apretado");
        assert_eq!(modifier_down(0x3C, 0x04), Some(true));
        assert_eq!(modifier_down(0x3A, 0x20), Some(true)); // ⌥ izq
        assert_eq!(modifier_down(0x39, 0xFFFF), None, "Caps Lock no es modificador del chord");
    }
}
