//! Mac: hilo dedicado con un CGEventTap de sesión + su run loop.
//!
//! Mismas reglas que el hook de Windows (hook.rs):
//! 1. El callback NO hace trabajo: consulta el motor y, si dispara, manda un `Fire` por canal.
//!    macOS desactiva el tap si el callback tarda (kCGEventTapDisabledByTimeout); si pasa,
//!    se vuelve a activar.
//! 2. El estado vive en un thread_local del hilo del tap: cero locks en el hot path.
//!    La reconfiguración llega por canal y se aplica en el próximo evento.
//! 3. Se traduce el keycode de Mac al VK de Windows (mackeys.rs): el motor es el mismo.
//!
//! Permiso: el tap necesita Accesibilidad (Ajustes del Sistema → Privacidad y seguridad).
//! Sin permiso CGEventTapCreate devuelve NULL; el hilo reintenta cada 2 s, así apenas el
//! usuario lo concede el atajo empieza a funcionar sin reiniciar la app.

use crate::hotkey::chord::{Binding, Engine, EngineConfig, Fire};
use super::keys::{mac_keycode_to_vk, modifier_down};
use core_foundation::base::TCFType;
use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop, CFRunLoopSource};
use std::cell::RefCell;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

type CGEventRef = *mut c_void;
type CFMachPortRef = *mut c_void;
type TapCallback = unsafe extern "C" fn(*mut c_void, u32, CGEventRef, *mut c_void) -> CGEventRef;

const K_CG_SESSION_EVENT_TAP: u32 = 1;
const K_CG_HEAD_INSERT_EVENT_TAP: u32 = 0;
const K_CG_EVENT_TAP_OPTION_DEFAULT: u32 = 0;
const KEY_DOWN: u32 = 10;
const KEY_UP: u32 = 11;
const FLAGS_CHANGED: u32 = 12;
const TAP_DISABLED_BY_TIMEOUT: u32 = 0xFFFF_FFFE;
const TAP_DISABLED_BY_USER_INPUT: u32 = 0xFFFF_FFFF;
const FIELD_KEYCODE: u32 = 9;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn CGEventTapCreate(tap: u32, place: u32, options: u32, mask: u64, cb: TapCallback, user: *mut c_void) -> CFMachPortRef;
    fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
    fn CGEventGetIntegerValueField(ev: CGEventRef, field: u32) -> i64;
    fn CGEventGetFlags(ev: CGEventRef) -> u64;
    fn CGEventGetTimestamp(ev: CGEventRef) -> u64;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFMachPortCreateRunLoopSource(alloc: *const c_void, port: CFMachPortRef, order: isize) -> *mut c_void;
}

type Reconfig = (Vec<Binding>, EngineConfig);

struct Ctx {
    engine: Engine,
    tx: Sender<Fire>,
    reconf: Receiver<Reconfig>,
    tap: CFMachPortRef,
}

thread_local! {
    static CTX: RefCell<Option<Ctx>> = const { RefCell::new(None) };
}

pub struct HookHandle {
    reconf: Sender<Reconfig>,
    stop: Arc<AtomicBool>,
    run_loop: Arc<Mutex<Option<CFRunLoop>>>,
}

impl HookHandle {
    /// Arranca el hilo del tap. Nunca falla: sin permiso de Accesibilidad reintenta solo.
    pub fn start(bindings: Vec<Binding>, cfg: EngineConfig, tx: Sender<Fire>) -> anyhow::Result<Self> {
        let (reconf_tx, reconf_rx) = mpsc::channel::<Reconfig>();
        let stop = Arc::new(AtomicBool::new(false));
        let run_loop: Arc<Mutex<Option<CFRunLoop>>> = Arc::new(Mutex::new(None));
        let (stop2, rl2) = (stop.clone(), run_loop.clone());

        thread::Builder::new().name("cs-keyboard-hook".into()).spawn(move || unsafe {
            CTX.with(|c| *c.borrow_mut() = Some(Ctx {
                engine: Engine::new(bindings, cfg), tx, reconf: reconf_rx, tap: std::ptr::null_mut(),
            }));
            let mask = (1u64 << KEY_DOWN) | (1u64 << KEY_UP) | (1u64 << FLAGS_CHANGED);
            let tap = loop {
                if stop2.load(Ordering::Relaxed) { return; }
                let t = CGEventTapCreate(K_CG_SESSION_EVENT_TAP, K_CG_HEAD_INSERT_EVENT_TAP,
                                         K_CG_EVENT_TAP_OPTION_DEFAULT, mask, tap_proc, std::ptr::null_mut());
                if !t.is_null() { break t; }
                thread::sleep(Duration::from_secs(2)); // sin permiso de Accesibilidad todavía
            };
            CTX.with(|c| if let Some(ctx) = c.borrow_mut().as_mut() { ctx.tap = tap; });
            let src_ref = CFMachPortCreateRunLoopSource(std::ptr::null(), tap, 0);
            let source = CFRunLoopSource::wrap_under_create_rule(src_ref as _);
            let rl = CFRunLoop::get_current();
            rl.add_source(&source, kCFRunLoopCommonModes);
            CGEventTapEnable(tap, true);
            *rl2.lock().unwrap() = Some(rl);
            if !stop2.load(Ordering::Relaxed) { CFRunLoop::run_current(); }
            CGEventTapEnable(tap, false);
            CTX.with(|c| *c.borrow_mut() = None);
        })?;

        Ok(Self { reconf: reconf_tx, stop, run_loop })
    }

    pub fn reconfigure(&self, bindings: Vec<Binding>, cfg: EngineConfig) {
        let _ = self.reconf.send((bindings, cfg));
    }
}

impl Drop for HookHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(rl) = self.run_loop.lock().unwrap().take() { rl.stop(); }
    }
}

unsafe extern "C" fn tap_proc(_proxy: *mut c_void, ty: u32, ev: CGEventRef, _user: *mut c_void) -> CGEventRef {
    if ty == TAP_DISABLED_BY_TIMEOUT || ty == TAP_DISABLED_BY_USER_INPUT {
        CTX.with(|c| if let Some(ctx) = c.borrow().as_ref() { CGEventTapEnable(ctx.tap, true); });
        return ev;
    }
    let code = CGEventGetIntegerValueField(ev, FIELD_KEYCODE) as u16;
    let down = match ty {
        KEY_DOWN => true,
        KEY_UP => false,
        FLAGS_CHANGED => match modifier_down(code, CGEventGetFlags(ev)) { Some(d) => d, None => return ev },
        _ => return ev,
    };
    let Some(vk) = mac_keycode_to_vk(code) else {
        // Tecla que nunca es disparadora: igual corta un doble toque en curso (como en Windows).
        if down { CTX.with(|c| if let Some(ctx) = c.borrow_mut().as_mut() { let _ = ctx.engine.on_key(0xFF, true, 0); }); }
        return ev;
    };
    let time_ms = (CGEventGetTimestamp(ev) / 1_000_000) as u32; // ns desde el arranque → ms (hace wrap como el tick)
    let swallow = CTX.with(|c| {
        let mut b = c.borrow_mut();
        let Some(ctx) = b.as_mut() else { return false };
        while let Ok((bindings, cfg)) = ctx.reconf.try_recv() { ctx.engine.reconfigure(bindings, cfg); }
        let d = ctx.engine.on_key(vk, down, time_ms);
        if let Some(f) = d.fire { let _ = ctx.tx.send(f); }
        d.swallow // `inject_mask` no aplica: en Mac soltar ⌘/⌥ no abre ningún menú
    });
    if swallow { std::ptr::null_mut() } else { ev }
}
