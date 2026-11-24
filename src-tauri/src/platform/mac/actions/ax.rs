//! Ventanas de otras apps vía la API de Accesibilidad (AXUIElement): listar, mover,
//! redimensionar, minimizar, cerrar y traer al frente. Requiere el permiso de Accesibilidad.
//!
//! ⚠️ Excepción honesta (como IPolicyConfig en Windows): para saber QUÉ ventana es cada
//! una se usa `_AXUIElementGetWindow`, función privada de HIServices que da el
//! CGWindowID de un elemento. Es estable desde 10.6 y la usan Rectangle, yabai y AltTab.
//! Sin ella no hay forma de reconocer una ventana entre dos activaciones sin leer su título.

use core_foundation::array::{CFArray, CFArrayRef};
use core_foundation::base::{CFType, CFTypeRef, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::CFDictionary;
use core_foundation::string::{CFString, CFStringRef};
use std::ffi::c_void;

type AXUIElementRef = *const c_void;
type AXError = i32;
const AX_OK: AXError = 0;
const AX_VALUE_CGPOINT: u32 = 1;
const AX_VALUE_CGSIZE: u32 = 2;

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Point { pub x: f64, pub y: f64 }
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Size { pub w: f64, pub h: f64 }

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrustedWithOptions(options: *const c_void) -> bool;
    static kAXTrustedCheckOptionPrompt: CFStringRef;
    fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
    fn AXUIElementCopyAttributeValue(el: AXUIElementRef, attr: CFStringRef, value: *mut CFTypeRef) -> AXError;
    fn AXUIElementSetAttributeValue(el: AXUIElementRef, attr: CFStringRef, value: CFTypeRef) -> AXError;
    fn AXUIElementPerformAction(el: AXUIElementRef, action: CFStringRef) -> AXError;
    fn AXUIElementSetMessagingTimeout(el: AXUIElementRef, seconds: f32) -> AXError;
    fn AXValueCreate(ty: u32, value: *const c_void) -> CFTypeRef;
    fn AXValueGetValue(value: CFTypeRef, ty: u32, out: *mut c_void) -> bool;
    fn _AXUIElementGetWindow(el: AXUIElementRef, id: *mut u32) -> AXError;
}

/// ¿Tiene la app permiso de Accesibilidad? Con `prompt`, macOS muestra su diálogo
/// (una vez) que lleva a Ajustes del Sistema.
pub fn trusted(prompt: bool) -> bool {
    unsafe {
        let key = CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt);
        let opts = CFDictionary::from_CFType_pairs(&[(key.as_CFType(), CFBoolean::from(prompt).as_CFType())]);
        AXIsProcessTrustedWithOptions(opts.as_concrete_TypeRef() as *const c_void)
    }
}

/// Elemento AX con su propia referencia (CFRetain/CFRelease vía CFType).
#[derive(Clone)]
pub struct Element(CFType);

// Los AXUIElement se pueden usar desde cualquier hilo (son proxies de IPC).
unsafe impl Send for Element {}
unsafe impl Sync for Element {}

fn attr(name: &'static str) -> CFString { CFString::from_static_string(name) }

impl Element {
    fn raw(&self) -> AXUIElementRef { self.0.as_CFTypeRef() as AXUIElementRef }

    pub fn application(pid: i32) -> Self {
        let el = unsafe { CFType::wrap_under_create_rule(AXUIElementCreateApplication(pid) as CFTypeRef) };
        // Una app colgada no puede trabar la activación entera.
        unsafe { AXUIElementSetMessagingTimeout(el.as_CFTypeRef() as AXUIElementRef, 1.0); }
        Self(el)
    }

    fn get(&self, name: &'static str) -> Option<CFType> {
        let mut v: CFTypeRef = std::ptr::null();
        let err = unsafe { AXUIElementCopyAttributeValue(self.raw(), attr(name).as_concrete_TypeRef(), &mut v) };
        (err == AX_OK && !v.is_null()).then(|| unsafe { CFType::wrap_under_create_rule(v) })
    }

    fn set(&self, name: &'static str, value: &CFType) -> bool {
        unsafe { AXUIElementSetAttributeValue(self.raw(), attr(name).as_concrete_TypeRef(), value.as_CFTypeRef()) == AX_OK }
    }

    fn act(&self, action: &'static str) -> bool {
        unsafe { AXUIElementPerformAction(self.raw(), attr(action).as_concrete_TypeRef()) == AX_OK }
    }

    fn get_bool(&self, name: &'static str) -> Option<bool> {
        self.get(name)?.downcast::<CFBoolean>().map(bool::from)
    }

    fn get_string(&self, name: &'static str) -> Option<String> {
        self.get(name)?.downcast::<CFString>().map(|s| s.to_string())
    }

    /// Ventanas de una app (de este Space y minimizadas).
    pub fn windows(&self) -> Vec<Element> {
        let Some(v) = self.get("AXWindows") else { return vec![] };
        if !v.instance_of::<CFArray>() { return vec![]; }
        let arr: CFArray<CFType> = unsafe { CFArray::wrap_under_get_rule(v.as_CFTypeRef() as CFArrayRef) };
        arr.iter().map(|w| Element(w.clone())).collect()
    }

    /// Ventana "de verdad": no paletas, diálogos sueltos ni popovers.
    pub fn is_standard(&self) -> bool {
        self.get_string("AXSubrole").as_deref() == Some("AXStandardWindow")
    }

    /// CGWindowID de la ventana (ver la excepción al principio del archivo).
    pub fn window_id(&self) -> Option<u32> {
        let mut id = 0u32;
        let err = unsafe { _AXUIElementGetWindow(self.raw(), &mut id) };
        (err == AX_OK && id != 0).then_some(id)
    }

    pub fn position(&self) -> Option<Point> {
        let v = self.get("AXPosition")?;
        let mut p = Point::default();
        unsafe { AXValueGetValue(v.as_CFTypeRef(), AX_VALUE_CGPOINT, &mut p as *mut _ as *mut c_void) }.then_some(p)
    }

    pub fn size(&self) -> Option<Size> {
        let v = self.get("AXSize")?;
        let mut s = Size::default();
        unsafe { AXValueGetValue(v.as_CFTypeRef(), AX_VALUE_CGSIZE, &mut s as *mut _ as *mut c_void) }.then_some(s)
    }

    pub fn set_position(&self, p: Point) -> bool {
        let v = unsafe { CFType::wrap_under_create_rule(AXValueCreate(AX_VALUE_CGPOINT, &p as *const _ as *const c_void)) };
        self.set("AXPosition", &v)
    }

    pub fn set_size(&self, s: Size) -> bool {
        let v = unsafe { CFType::wrap_under_create_rule(AXValueCreate(AX_VALUE_CGSIZE, &s as *const _ as *const c_void)) };
        self.set("AXSize", &v)
    }

    pub fn is_minimized(&self) -> bool { self.get_bool("AXMinimized").unwrap_or(false) }

    pub fn is_fullscreen(&self) -> bool { self.get_bool("AXFullScreen").unwrap_or(false) }

    pub fn set_minimized(&self, on: bool) -> bool {
        self.set("AXMinimized", &CFBoolean::from(on).as_CFType())
    }

    /// Como clic en el botón rojo: la app decide (si hay cambios sin guardar, pregunta).
    pub fn close(&self) -> bool {
        self.get("AXCloseButton")
            .map(|b| Element(b).act("AXPress"))
            .unwrap_or(false)
    }

    /// Trae la ventana al frente dentro de su app (la app se activa aparte).
    pub fn raise(&self) {
        if self.is_minimized() { self.set_minimized(false); }
        self.set("AXMain", &CFBoolean::true_value().as_CFType());
        self.act("AXRaise");
    }
}

/// Ventanas estándar de un proceso, con su CGWindowID.
pub fn windows_of_pid(pid: i32) -> Vec<(u32, Element)> {
    Element::application(pid).windows().into_iter()
        .filter(|w| w.is_standard())
        .filter_map(|w| w.window_id().map(|id| (id, w)))
        .collect()
}
