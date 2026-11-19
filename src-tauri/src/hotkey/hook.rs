//! Hilo dedicado con WH_KEYBOARD_LL + message loop.
//!
//! Reglas de oro del hook de bajo nivel:
//! 1. El callback NO hace trabajo: consulta la máquina de estados (O(n) con n≈perfiles)
//!    y, si dispara, manda un mensaje por canal. Windows elimina en silencio los hooks
//!    que superan `LowLevelHooksTimeout` (~1 s).
//! 2. El estado vive en un thread_local del hilo del hook → cero locks en el hot path.
//! 3. La reconfiguración llega por `PostThreadMessageW` al mismo hilo.
//! 4. Se ignoran eventos inyectados (LLKHF_INJECTED) → sin bucles con SendInput.
//!
//! CPU: el hilo duerme en GetMessageW; solo despierta por evento de teclado.

use super::chord::{Binding, Engine, EngineConfig, Fire};
use std::cell::RefCell;
use std::sync::mpsc::{self, Sender};
use std::thread;

use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Threading::{
    GetCurrentThread, GetCurrentThreadId, SetThreadPriority, THREAD_PRIORITY_ABOVE_NORMAL,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, PeekMessageW, PostThreadMessageW,
    SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, HC_ACTION, HHOOK, KBDLLHOOKSTRUCT,
    LLKHF_INJECTED, MSG, PM_NOREMOVE, WH_KEYBOARD_LL, WM_APP, WM_KEYDOWN, WM_QUIT,
    WM_SYSKEYDOWN, WM_USER,
};

const WM_RECONFIGURE: u32 = WM_APP + 1;
/// vkE8: código sin asignar; misma técnica de "mask key" que usa AutoHotkey.
const VK_MASK: u16 = 0xE8;

struct Reconfig {
    bindings: Vec<Binding>,
    cfg: EngineConfig,
}

struct Ctx {
    engine: Engine,
    tx: Sender<Fire>,
}

thread_local! {
    static CTX: RefCell<Option<Ctx>> = const { RefCell::new(None) };
}

pub struct HookHandle {
    thread_id: u32,
    join: Option<thread::JoinHandle<()>>,
}

impl HookHandle {
    /// Arranca el hilo del hook. `on_fire` recibe los disparos (en el hilo worker).
    pub fn start(bindings: Vec<Binding>, cfg: EngineConfig, tx: Sender<Fire>) -> anyhow::Result<Self> {
        let (ready_tx, ready_rx) = mpsc::channel::<anyhow::Result<u32>>();

        let join = thread::Builder::new().name("cs-keyboard-hook".into()).spawn(move || unsafe {
            let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_ABOVE_NORMAL);

            // Fuerza la creación de la cola de mensajes antes de publicar el thread id.
            let mut msg = MSG::default();
            let _ = PeekMessageW(&mut msg, None, WM_USER, WM_USER, PM_NOREMOVE);

            CTX.with(|c| *c.borrow_mut() = Some(Ctx { engine: Engine::new(bindings, cfg), tx }));

            let hook = match SetWindowsHookExW(WH_KEYBOARD_LL, Some(ll_proc), HINSTANCE::default(), 0) {
                Ok(h) => h,
                Err(e) => {
                    let _ = ready_tx.send(Err(e.into()));
                    return;
                }
            };
            let _ = ready_tx.send(Ok(GetCurrentThreadId()));

            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                if msg.hwnd.0.is_null() && msg.message == WM_RECONFIGURE {
                    let rc = Box::from_raw(msg.lParam.0 as *mut Reconfig);
                    CTX.with(|c| {
                        if let Some(ctx) = c.borrow_mut().as_mut() {
                            ctx.engine.reconfigure(rc.bindings, rc.cfg);
                        }
                    });
                    continue;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            let _ = UnhookWindowsHookEx(hook);
            CTX.with(|c| *c.borrow_mut() = None);
        })?;

        let thread_id = ready_rx.recv()??;
        Ok(Self { thread_id, join: Some(join) })
    }

    pub fn reconfigure(&self, bindings: Vec<Binding>, cfg: EngineConfig) {
        let ptr = Box::into_raw(Box::new(Reconfig { bindings, cfg }));
        unsafe {
            if PostThreadMessageW(self.thread_id, WM_RECONFIGURE, WPARAM(0), LPARAM(ptr as isize)).is_err() {
                drop(Box::from_raw(ptr)); // no se entregó: liberar
            }
        }
    }
}

impl Drop for HookHandle {
    fn drop(&mut self) {
        unsafe { let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0)); }
        if let Some(j) = self.join.take() { let _ = j.join(); }
    }
}

unsafe extern "system" fn ll_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let k = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        if k.flags.0 & LLKHF_INJECTED.0 == 0 {
            let down = matches!(wparam.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
            // `k` (vk + timestamp) se evalúa y se descarta en este mismo frame.
            let decision = CTX.with(|c| {
                c.borrow_mut().as_mut().map(|ctx| {
                    let d = ctx.engine.on_key(k.vkCode, down, k.time);
                    if let Some(f) = d.fire { let _ = ctx.tx.send(f); }
                    d
                })
            });
            if let Some(d) = decision {
                if d.inject_mask { send_mask_key(); }
                if d.swallow { return LRESULT(1); }
            }
        }
    }
    CallNextHookEx(HHOOK::default(), code, wparam, lparam)
}

/// Evita que soltar Win abra Inicio (o que soltar Alt active la barra de menú)
/// cuando tragamos la tecla disparadora.
unsafe fn send_mask_key() {
    let mk = |flags| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT { wVk: VIRTUAL_KEY(VK_MASK), dwFlags: flags, ..Default::default() },
        },
    };
    let inputs = [mk(Default::default()), mk(KEYEVENTF_KEYUP)];
    SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
}
