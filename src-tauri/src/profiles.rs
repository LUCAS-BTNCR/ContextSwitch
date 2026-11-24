//! Modelo de `profiles.json` (ver docs/profiles.schema.json).
//! Único archivo de estado persistente. Ubicación: %APPDATA%\ContextSwitch\profiles.json
//! (Windows) o ~/Library/Application Support/ContextSwitch/profiles.json (Mac).

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
    /// Al iniciar con Windows: true = ventanita "¿Deseas aplicar algún contexto?";
    /// false = arranca oculta en la bandeja, sin mostrar nada.
    #[serde(default = "d_true")]
    pub show_startup_picker: bool,
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
    /// "Recordar cómo lo dejé": al salir se guarda qué apps había y dónde estaban sus
    /// ventanas; al volver se reabren las que faltan y se restaura el layout.
    #[serde(default)]
    pub restore_session: bool,
    #[serde(default)]
    pub layout: Option<LayoutSnapshot>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
/// En Mac: `alt` = ⌥ Option y `win` = ⌘ Command.
pub enum Modifier { Ctrl, Shift, Alt, Win }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Trigger {
    pub modifiers: Vec<Modifier>,
    /// Etiqueta legible ("G", "1", "´").
    pub key: String,
    /// Virtual-key code de Windows; es lo que compara el hook. En Mac el hook traduce
    /// el keycode de la tecla a este mismo código, así profiles.json sirve en los dos.
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
pub enum AppKind {
    Generic,
    BrowserZen,
    /// Navegador sin workspaces (Chrome, Vivaldi, Brave, Edge, Firefox…): cada modo
    /// tiene sus propias ventanas, que se abren con sus URLs y se guardan al salir.
    Browser,
    Mail,
    Chat,
}

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
    /// Ruta absoluta al .exe (Windows) o a la .app (Mac), para lanzar.
    pub exe_path: String,
    /// Nombre de proceso para encontrar ventanas ("zen.exe"; en Mac el nombre de la app,
    /// "Google Chrome"). Comparación case-insensitive.
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
    /// Zen y navegadores: cada URL de `open` en su propia ventana (si no, pestañas de una misma ventana).
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

/// Resultado de [`Config::load_or_recover`].
#[derive(Debug)]
pub struct Recovery {
    pub config: Config,
    /// Dónde quedó el archivo corrupto, si hubo que apartarlo.
    pub backup: Option<std::path::PathBuf>,
    /// false = no se pudo apartar el archivo corrupto: NO escribir en disco.
    pub persist: bool,
}

impl Config {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        if !path.exists() {
            let c = Self::default_config();
            c.save(path)?;
            return Ok(c);
        }
        let raw = fs::read_to_string(path)?;
        // El Bloc de notas / PowerShell 5.1 guardan UTF-8 con BOM: no es un archivo dañado.
        Ok(serde_json::from_str(raw.trim_start_matches('\u{feff}'))?)
    }

    /// Carga para el arranque, sin riesgo de perder la configuración del usuario.
    ///
    /// Antes: si `profiles.json` estaba corrupto se usaba la config por defecto en memoria
    /// y la primera activación la guardaba ENCIMA del archivo del usuario (bug A).
    /// Ahora el archivo corrupto se aparta (`profiles.corrupt-<fecha>.json`) antes de
    /// escribir nada; si ni siquiera se puede apartar, la app no escribe en disco.
    pub fn load_or_recover(path: &Path) -> Recovery {
        match Self::load(path) {
            Ok(config) => Recovery { config, backup: None, persist: true },
            Err(_) => {
                let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
                let backup = path.with_file_name(format!("profiles.corrupt-{stamp}.json"));
                let moved = fs::rename(path, &backup).is_ok()
                    || (fs::copy(path, &backup).is_ok() && fs::remove_file(path).is_ok());
                let config = Self::default_config();
                if moved { let _ = config.save(path); }
                Recovery { config, backup: moved.then_some(backup), persist: moved }
            }
        }
    }

    /// Config que manda la UI al guardar. El modo activo lo maneja el backend: la UI
    /// puede tener un valor viejo (cambiaste de modo con el atajo después de abrir
    /// Ajustes) y antes lo pisaba, rompiendo las acciones de salida del modo (bug B).
    pub fn accept_from_ui(mut self, current: &Config) -> Config {
        self.settings.active_profile_id = current.settings.active_profile_id.clone();
        self
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
        // Windows: Ctrl+Shift. Mac: ⌃⌥ (Ctrl+Shift+T/G chocan con atajos comunes de Mac).
        let mods = if cfg!(target_os = "macos") { vec![Modifier::Ctrl, Modifier::Alt] } else { vec![Modifier::Ctrl, Modifier::Shift] };
        Self {
            schema: Some("./profiles.schema.json".into()),
            version: 1,
            settings: Settings {
                double_tap_window_ms: 450,
                swallow_trigger: true,
                start_with_windows: false,
                show_startup_picker: true,
                show_toast_on_switch: true,
                active_profile_id: None,
            },
            profiles: vec![
                Profile {
                    id: "trabajo".into(), name: "Trabajo".into(), icon: Some("💼".into()), enabled: true,
                    trigger: Trigger { modifiers: mods.clone(), key: "T".into(), vk: 0x54 },
                    audio: None, apps: vec![], close_on_enter: vec![], close_others: false, keep_open: vec![], restore_session: false, layout: None,
                },
                Profile {
                    id: "gaming".into(), name: "Gaming".into(), icon: Some("🎮".into()), enabled: true,
                    trigger: Trigger { modifiers: mods, key: "G".into(), vk: 0x47 },
                    audio: None, apps: vec![], close_on_enter: vec![], close_others: false, keep_open: vec![], restore_session: false, layout: None,
                },
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp() -> tempfile::TempDir { tempfile::tempdir().unwrap() }

    // ── carga / guardado ─────────────────────────────────────────────────────
    #[test]
    fn load_inexistente_crea_el_default_en_disco() {
        let d = tmp();
        let p = d.path().join("sub").join("profiles.json");
        let c = Config::load(&p).unwrap();
        assert_eq!(c.profiles.len(), 2);
        assert!(p.exists(), "tiene que crear el archivo (y la carpeta)");
    }

    #[test]
    fn load_acepta_utf8_con_bom() {
        let d = tmp();
        let p = d.path().join("profiles.json");
        let json = serde_json::to_string(&Config::default_config()).unwrap();
        fs::write(&p, format!("\u{feff}{json}")).unwrap();
        assert!(Config::load(&p).is_ok(), "un BOM al inicio no debe tratarse como archivo dañado");
    }

    #[test]
    fn load_corrupto_es_error_no_default_silencioso() {
        let d = tmp();
        let p = d.path().join("profiles.json");
        fs::write(&p, "{ esto no es json").unwrap();
        assert!(Config::load(&p).is_err());
    }

    #[test]
    fn campos_faltantes_toman_su_default() {
        let d = tmp();
        let p = d.path().join("profiles.json");
        fs::write(&p, r#"{"version":1,"settings":{},"profiles":[
            {"id":"a","name":"A","trigger":{"modifiers":["ctrl"],"key":"A","vk":65}}]}"#).unwrap();
        let c = Config::load(&p).unwrap();
        assert_eq!(c.settings.double_tap_window_ms, 500);
        assert!(c.settings.swallow_trigger);
        let a = &c.profiles[0];
        assert!(a.enabled);
        assert!(a.apps.is_empty() && a.close_on_enter.is_empty() && a.keep_open.is_empty());
        assert!(!a.close_others && !a.restore_session);
    }

    #[test]
    fn campos_desconocidos_se_ignoran() {
        // Un profiles.json de una versión más nueva no tiene que romper la carga.
        let d = tmp();
        let p = d.path().join("profiles.json");
        fs::write(&p, r#"{"version":1,"settings":{"futuro":true},"profiles":[],"otraCosa":1}"#).unwrap();
        assert!(Config::load(&p).is_ok());
    }

    #[test]
    fn save_es_atomico_y_no_deja_tmp() {
        let d = tmp();
        let p = d.path().join("profiles.json");
        let mut c = Config::default_config();
        c.save(&p).unwrap();
        c.profiles[0].name = "Cambiado".into();
        c.save(&p).unwrap();
        assert!(!p.with_extension("json.tmp").exists());
        assert_eq!(Config::load(&p).unwrap().profiles[0].name, "Cambiado");
    }

    #[test]
    fn el_ejemplo_de_docs_coincide_con_el_modelo() {
        // Si cambia el modelo y no el ejemplo (o al revés), este test avisa.
        let raw = include_str!("../../docs/profiles.example.json");
        let c: Config = serde_json::from_str(raw).expect("docs/profiles.example.json no carga");
        let again: Config = serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
        assert_eq!(serde_json::to_value(&c).unwrap(), serde_json::to_value(&again).unwrap());
        assert!(!c.profiles.is_empty());
    }

    // ── bug A: profiles.json dañado no se pisa ───────────────────────────────
    #[test]
    fn bug_a_archivo_danado_se_aparta_antes_de_escribir() {
        let d = tmp();
        let p = d.path().join("profiles.json");
        let original = "{ \"version\": 1, roto";
        fs::write(&p, original).unwrap();

        let rec = Config::load_or_recover(&p);
        let backup = rec.backup.expect("tiene que apartar el archivo dañado");
        assert!(rec.persist);
        assert_eq!(fs::read_to_string(&backup).unwrap(), original, "la copia es el archivo original intacto");
        assert!(backup.file_name().unwrap().to_string_lossy().starts_with("profiles.corrupt-"));
        assert!(Config::load(&p).is_ok(), "en su lugar queda una config válida");
    }

    #[cfg(windows)]
    #[test]
    fn bug_a_si_no_se_puede_apartar_no_se_escribe() {
        use std::os::windows::fs::OpenOptionsExt;
        let d = tmp();
        let p = d.path().join("profiles.json");
        fs::write(&p, "roto").unwrap();
        // Otro proceso lo tiene abierto sin compartir: no se puede leer, mover ni copiar.
        let _lock = fs::OpenOptions::new().read(true).share_mode(0).open(&p).unwrap();

        let rec = Config::load_or_recover(&p);
        assert!(!rec.persist, "sin poder apartarlo, la app NO debe escribir");
        assert!(rec.backup.is_none());
        drop(_lock);
        assert_eq!(fs::read_to_string(&p).unwrap(), "roto", "el archivo del usuario queda intacto");
    }

    #[test]
    fn archivo_valido_no_genera_copia() {
        let d = tmp();
        let p = d.path().join("profiles.json");
        Config::default_config().save(&p).unwrap();
        let rec = Config::load_or_recover(&p);
        assert!(rec.persist && rec.backup.is_none());
        assert_eq!(fs::read_dir(d.path()).unwrap().count(), 1);
    }

    // ── bug B: la UI no pisa el modo activo ──────────────────────────────────
    #[test]
    fn bug_b_la_ui_no_pisa_el_modo_activo() {
        let mut backend = Config::default_config();
        backend.settings.active_profile_id = Some("gaming".into()); // cambiado con el atajo
        let mut desde_ui = Config::default_config();
        desde_ui.settings.active_profile_id = Some("trabajo".into()); // valor viejo de la UI
        desde_ui.profiles[0].name = "Editado en la UI".into();

        let merged = desde_ui.accept_from_ui(&backend);
        assert_eq!(merged.settings.active_profile_id.as_deref(), Some("gaming"));
        assert_eq!(merged.profiles[0].name, "Editado en la UI", "el resto de la edición se conserva");
    }

    #[test]
    fn bug_b_sin_modo_activo_en_backend_queda_sin_modo() {
        let backend = Config::default_config(); // None
        let mut ui = Config::default_config();
        ui.settings.active_profile_id = Some("trabajo".into());
        assert_eq!(ui.accept_from_ui(&backend).settings.active_profile_id, None);
    }

    #[test]
    fn profile_index_busca_por_id() {
        let c = Config::default_config();
        assert_eq!(c.profile_index("gaming"), Some(1));
        assert_eq!(c.profile_index("no-existe"), None);
    }
}
