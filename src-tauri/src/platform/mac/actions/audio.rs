//! Audio de salida en Mac con Core Audio (API pública: a diferencia de Windows, acá
//! cambiar el dispositivo predeterminado está documentado).
//!
//! - `id` del dispositivo = su UID (estable entre reinicios; el AudioObjectID no lo es).
//! - "También comunicaciones" en Mac = la salida de alertas y efectos del sistema.

use core_foundation::base::TCFType;
use core_foundation::string::{CFString, CFStringRef};
use serde::Serialize;
use std::ffi::c_void;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

#[repr(C)]
struct Address { selector: u32, scope: u32, element: u32 }

const fn fourcc(s: &[u8; 4]) -> u32 { u32::from_be_bytes(*s) }
const SYSTEM_OBJECT: u32 = 1;
const SCOPE_GLOBAL: u32 = fourcc(b"glob");
const SCOPE_OUTPUT: u32 = fourcc(b"outp");
const ELEMENT_MAIN: u32 = 0;
const DEVICES: u32 = fourcc(b"dev#");
const DEFAULT_OUTPUT: u32 = fourcc(b"dOut");
const DEFAULT_SYSTEM_OUTPUT: u32 = fourcc(b"sOut");
const NAME: u32 = fourcc(b"lnam");
const UID: u32 = fourcc(b"uid ");
const STREAMS: u32 = fourcc(b"stm#");

#[link(name = "CoreAudio", kind = "framework")]
extern "C" {
    fn AudioObjectGetPropertyDataSize(id: u32, addr: *const Address, qsize: u32, qdata: *const c_void, size: *mut u32) -> i32;
    fn AudioObjectGetPropertyData(id: u32, addr: *const Address, qsize: u32, qdata: *const c_void, size: *mut u32, data: *mut c_void) -> i32;
    fn AudioObjectSetPropertyData(id: u32, addr: *const Address, qsize: u32, qdata: *const c_void, size: u32, data: *const c_void) -> i32;
}

fn addr(selector: u32, scope: u32) -> Address { Address { selector, scope, element: ELEMENT_MAIN } }

fn get_u32(obj: u32, selector: u32) -> Option<u32> {
    let mut v = 0u32;
    let mut size = 4u32;
    let st = unsafe { AudioObjectGetPropertyData(obj, &addr(selector, SCOPE_GLOBAL), 0, std::ptr::null(), &mut size, &mut v as *mut _ as *mut c_void) };
    (st == 0).then_some(v)
}

fn get_string(obj: u32, selector: u32) -> Option<String> {
    let mut s: CFStringRef = std::ptr::null();
    let mut size = std::mem::size_of::<CFStringRef>() as u32;
    let st = unsafe { AudioObjectGetPropertyData(obj, &addr(selector, SCOPE_GLOBAL), 0, std::ptr::null(), &mut size, &mut s as *mut _ as *mut c_void) };
    (st == 0 && !s.is_null()).then(|| unsafe { CFString::wrap_under_create_rule(s) }.to_string())
}

fn device_ids() -> Vec<u32> {
    let a = addr(DEVICES, SCOPE_GLOBAL);
    let mut size = 0u32;
    unsafe {
        if AudioObjectGetPropertyDataSize(SYSTEM_OBJECT, &a, 0, std::ptr::null(), &mut size) != 0 { return vec![]; }
        let mut ids = vec![0u32; size as usize / 4];
        if AudioObjectGetPropertyData(SYSTEM_OBJECT, &a, 0, std::ptr::null(), &mut size, ids.as_mut_ptr() as *mut c_void) != 0 { return vec![]; }
        ids.truncate(size as usize / 4);
        ids
    }
}

/// ¿El dispositivo tiene salida? (los micrófonos no tienen streams de salida).
fn has_output(dev: u32) -> bool {
    let mut size = 0u32;
    let st = unsafe { AudioObjectGetPropertyDataSize(dev, &addr(STREAMS, SCOPE_OUTPUT), 0, std::ptr::null(), &mut size) };
    st == 0 && size > 0
}

pub fn list_outputs() -> anyhow::Result<Vec<AudioDevice>> {
    let default = get_u32(SYSTEM_OBJECT, DEFAULT_OUTPUT);
    Ok(device_ids().into_iter()
        .filter(|&d| has_output(d))
        .filter_map(|d| {
            let id = get_string(d, UID)?;
            let name = get_string(d, NAME).unwrap_or_else(|| id.clone());
            Some(AudioDevice { is_default: Some(d) == default, id, name })
        })
        .collect())
}

pub fn set_default_output(device_uid: &str, system_sounds_too: bool) -> anyhow::Result<()> {
    let dev = device_ids().into_iter()
        .find(|&d| has_output(d) && get_string(d, UID).as_deref() == Some(device_uid))
        .ok_or_else(|| anyhow::anyhow!("el dispositivo no está conectado"))?;
    let set = |selector: u32| unsafe {
        AudioObjectSetPropertyData(SYSTEM_OBJECT, &addr(selector, SCOPE_GLOBAL), 0, std::ptr::null(), 4, &dev as *const _ as *const c_void)
    };
    let st = set(DEFAULT_OUTPUT);
    if st != 0 { anyhow::bail!("Core Audio rechazó el cambio (código {st})"); }
    if system_sounds_too { let _ = set(DEFAULT_SYSTEM_OUTPUT); }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn listar_no_falla_y_hay_a_lo_sumo_un_predeterminado() {
        let v = super::list_outputs().unwrap();
        assert!(v.iter().filter(|d| d.is_default).count() <= 1);
        for d in &v { assert!(!d.id.is_empty()); }
    }
}
