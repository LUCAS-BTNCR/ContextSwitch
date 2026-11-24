//! Audio de salida.
//!
//! - Enumerar dispositivos: Core Audio público (IMMDeviceEnumerator).
//! - Cambiar el predeterminado: Windows NO expone API pública para esto.
//!   Se usa IPolicyConfig (COM no documentado, estable desde Vista; lo usan
//!   SoundVolumeView, AudioDeviceCmdlets, EarTrumpet). Es la única excepción
//!   al principio de "solo contratos públicos" y queda aislada en este archivo.

use serde::Serialize;
use windows::core::{interface, IUnknown, IUnknown_Vtbl, HRESULT, HSTRING, PCWSTR};
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Media::Audio::{
    eCommunications, eConsole, eMultimedia, eRender, ERole, IMMDeviceEnumerator, MMDeviceEnumerator,
    DEVICE_STATE_ACTIVE,
};
use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_ALL, STGM_READ};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

/// CLSID_PolicyConfigClient
const CLSID_POLICY_CONFIG: windows::core::GUID =
    windows::core::GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);

/// Solo nos importa el slot 11 (SetDefaultEndpoint); los anteriores se declaran
/// para respetar el orden de la vtable y nunca se invocan.
#[interface("f8679f50-850a-41cf-9c72-430f290290c8")]
unsafe trait IPolicyConfig: IUnknown {
    unsafe fn _get_mix_format(&self) -> HRESULT;
    unsafe fn _get_device_format(&self) -> HRESULT;
    unsafe fn _reset_device_format(&self) -> HRESULT;
    unsafe fn _set_device_format(&self) -> HRESULT;
    unsafe fn _get_processing_period(&self) -> HRESULT;
    unsafe fn _set_processing_period(&self) -> HRESULT;
    unsafe fn _get_share_mode(&self) -> HRESULT;
    unsafe fn _set_share_mode(&self) -> HRESULT;
    unsafe fn _get_property_value(&self) -> HRESULT;
    unsafe fn _set_property_value(&self) -> HRESULT;
    unsafe fn set_default_endpoint(&self, device_id: PCWSTR, role: ERole) -> HRESULT;
}

/// Requiere COM inicializado en el hilo que llama (ver actions::worker).
pub fn list_outputs() -> anyhow::Result<Vec<AudioDevice>> {
    unsafe {
        let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let default_id = en
            .GetDefaultAudioEndpoint(eRender, eConsole)
            .and_then(|d| d.GetId())
            .map(|p| { let s = p.to_string().unwrap_or_default(); CoTaskMemFree(Some(p.0 as _)); s })
            .unwrap_or_default();

        let coll = en.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)?;
        let mut out = Vec::new();
        for i in 0..coll.GetCount()? {
            let dev = coll.Item(i)?;
            let pid = dev.GetId()?;
            let id = pid.to_string()?;
            CoTaskMemFree(Some(pid.0 as _));
            let name = dev
                .OpenPropertyStore(STGM_READ)
                .and_then(|s| s.GetValue(&PKEY_Device_FriendlyName))
                .map(|v| v.to_string())
                .unwrap_or_else(|_| id.clone());
            out.push(AudioDevice { is_default: id == default_id, id, name });
        }
        Ok(out)
    }
}

pub fn set_default_output(device_id: &str, communications_too: bool) -> anyhow::Result<()> {
    unsafe {
        let pc: IPolicyConfig = CoCreateInstance(&CLSID_POLICY_CONFIG, None, CLSCTX_ALL)?;
        let id = HSTRING::from(device_id);
        pc.set_default_endpoint(PCWSTR(id.as_ptr()), eConsole).ok()?;
        pc.set_default_endpoint(PCWSTR(id.as_ptr()), eMultimedia).ok()?;
        if communications_too {
            pc.set_default_endpoint(PCWSTR(id.as_ptr()), eCommunications).ok()?;
        }
        Ok(())
    }
}
