//! Modelo de `profiles.json` (ver docs/profiles.schema.json).
//! Único archivo de estado persistente. Ubicación: %APPDATA%\ContextSwitch\profiles.json

use serde::{Deserialize, Serialize};
use std::{fs, io::Write, path::Path};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    #[serde(rename = "$schema", default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub version: u32,
    pub settings: Settings,
    pub profiles: Vec<Profile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default = "d_window")]
    pub double_tap_window_ms: u32,
    #[serde(default = "d_true")]
    pub swallow_trigger: bool,
    #[serde(default)]
    pub start_with_windows: bool,
    #[serde(default = "d_true")]
    pub show_toast_on_switch: bool,
    #[serde(default)]
    pub active_profile_id: Option<String>,
}
fn d_window() -> u32 { 500 }
fn d_true() -> bool { true }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default = "d_true")]
    pub enabled: bool,
    pub trigger: Trigger,
    #[serde(default)]
    pub audio: Option<AudioTarget>,
    #[serde(default)]
    pub apps: Vec<AppRule>,
    /// Lista de exclusión: procesos que se cierran (WM_CLOSE) al ENTRAR al perfil.
    #[serde(default)]
    pub close_on_enter: Vec<String>,
    /// "Solo este modo": al entrar, WM_CLOSE a todo proceso con ventana que no esté en `apps`.
    #[serde(default)]
    pub close_others: bool,
    /// Procesos que `close_others` nunca cierra (además del shell de Windows).
    #[serde(default)]
    pub keep_open: Vec<String>,
    #[serde(default)]
    pub layout: Option<LayoutSnapshot>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Modifier { Ctrl, Shift, Alt, Win }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Trigger {
    pub modifiers: Vec<Modifier>,
    /// Etiqueta legible ("G", "1", "´").
    pub key: String,
    /// Virtual-key code de Windows; es lo que compara el hook.
    pub vk: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioTarget {
    /// ID de endpoint de Core Audio (IMMDevice::GetId).
    pub output_device_id: String,
    /// Solo para mostrar / re-vincular si cambia el ID.
    pub output_device_name: String,
    #[serde(default = "d_true")]
    pub set_communications_too: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum AppKind { Generic, BrowserZen, Mail, Chat }

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum EnterAction { LaunchOrFocus, Launch, None }

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ExitAction { Keep, Minimize, Close }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppRule {
    pub id: String,
    pub label: String,
    pub kind: AppKind,
    /// Ruta absoluta al .exe (para lanzar).
    pub exe_path: String,
    /// Nombre de proceso para encontrar ventanas ("zen.exe"). Comparación case-insensitive.
    pub process_name: String,
    #[serde(default)]
    pub args: Vec<String>,
    /// Nombre libre de Workspace/Perfil dentro de la app (no tiene que coincidir con el modo).
    #[serde(default)]
    pub workspace: Option<String>,
    /// Zen: contenedor asignado como default de ese workspace (puente local).
    #[serde(default)]
    pub container: Option<String>,
    /// URLs o rutas a abrir al entrar.
    #[serde(default)]
    pub open: Vec<String>,
    #[serde(default = "d_enter")]
    pub on_enter: EnterAction,
    #[serde(default = "d_exit")]
    pub on_exit: ExitAction,
    /// Zen: descargar pestañas inactivas al entrar.
    #[serde(default)]
    pub unload_inactive_tabs: bool,
    /// Zen: cada URL de `open` en su propia ventana (si no, pestañas de una misma ventana).
    #[serde(default)]
    pub separate_windows: bool,
    /// Posición de cada ventana. Con `separate_windows`, el índice i es la ventana de `open[i]`;
    /// si no, el índice 0 es la ventana principal de la app.
    #[serde(default)]
    pub placements: Vec<Option<Placement>>,
}

/// Zona de un monitor, en fracciones (0–1) de su área de trabajo.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Placement {
    /// Índice del monitor, ordenados de izquierda a derecha (0 = el de más a la izquierda).
    pub monitor: u32,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}
fn d_enter() -> EnterAction { EnterAction::LaunchOrFocus }
fn d_exit() -> ExitAction { ExitAction::Keep }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutSnapshot {
    pub captured_at: String,
    pub windows: Vec<WindowPlacementRec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowPlacementRec {
    pub process_name: String,
    /// Subcadena opcional del título para desambiguar varias ventanas del mismo proceso.
    #[serde(default)]
    pub title_contains: Option<String>,
    /// Ordinal entre ventanas del mismo proceso (0 = primera encontrada).
    #[serde(default)]
    pub ordinal: u32,
    pub show: ShowState,
    pub rect: Rect,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ShowState { Normal, Maximized, Minimized }

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Rect { pub left: i32, pub top: i32, pub right: i32, pub bottom: i32 }

impl Config {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        if !path.exists() {
            let c = Self::default_config();
            c.save(path)?;
            return Ok(c);
        }
        Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
    }

    /// Escritura atómica: tmp + rename (un crash nunca deja el JSON a medias).
    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(dir) = path.parent() { fs::create_dir_all(dir)?; }
        let tmp = path.with_extension("json.tmp");
        let mut f = fs::File::create(&tmp)?;
        f.write_all(serde_json::to_string_pretty(self)?.as_bytes())?;
        f.sync_all()?;
        drop(f);
        fs::rename(tmp, path)?;
        Ok(())
    }

    pub fn profile_index(&self, id: &str) -> Option<usize> {
        self.profiles.iter().position(|p| p.id == id)
    }

    pub fn default_config() -> Self {
        Self {
            schema: Some("./profiles.schema.json".into()),
            version: 1,
            settings: Settings {
                double_tap_window_ms: 450,
                swallow_trigger: true,
                start_with_windows: false,
                show_toast_on_switch: true,
                active_profile_id: None,
            },
            profiles: vec![
                Profile {
                    id: "trabajo".into(), name: "Trabajo".into(), icon: Some("💼".into()), enabled: true,
                    trigger: Trigger { modifiers: vec![Modifier::Ctrl, Modifier::Shift], key: "T".into(), vk: 0x54 },
                    audio: None, apps: vec![], close_on_enter: vec![], close_others: false, keep_open: vec![], layout: None,
                },
                Profile {
                    id: "gaming".into(), name: "Gaming".into(), icon: Some("🎮".into()), enabled: true,
                    trigger: Trigger { modifiers: vec![Modifier::Ctrl, Modifier::Shift], key: "G".into(), vk: 0x47 },
                    audio: None, apps: vec![], close_on_enter: vec![], close_others: false, keep_open: vec![], layout: None,
                },
            ],
        }
    }
}
