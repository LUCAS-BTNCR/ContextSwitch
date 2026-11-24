//! Actualizaciones bajo demanda desde las Releases de GitHub.
//!
//! Solo se conecta cuando el usuario toca "Buscar actualizaciones": nunca en segundo plano.
//! No hay cliente HTTP en la app: se usa el `curl` del sistema (System32\curl.exe en
//! Windows, /usr/bin/curl en Mac).

use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

const LATEST_API: &str = "https://api.github.com/repos/LUCAS-BTNCR/ContextSwitch/releases/latest";
/// Solo se descargan instaladores de las Releases de este repo.
const DOWNLOAD_PREFIX: &str = "https://github.com/LUCAS-BTNCR/ContextSwitch/releases/download/";
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
#[cfg(windows)]
const DETACHED_PROCESS: u32 = 0x0000_0008;

/// Nombre del archivo de instalación de cada sistema en las Releases.
#[cfg(windows)]
const INSTALLER_SUFFIX: &str = "_x64-setup.exe";
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
const INSTALLER_SUFFIX: &str = "_aarch64.dmg";
#[cfg(all(target_os = "macos", not(target_arch = "aarch64")))]
const INSTALLER_SUFFIX: &str = "_x64.dmg";

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub available: bool,
    /// URL del instalador de este sistema (`*_x64-setup.exe` / `*_aarch64.dmg`) de la última release.
    pub installer_url: Option<String>,
    pub notes: String,
}

pub fn current_version() -> String {
    // Solo para pruebas de punta a punta: simular que esta copia es más vieja.
    std::env::var("CONTEXTSWITCH_UPDATE_AS_VERSION").unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string())
}

#[cfg(windows)]
fn curl() -> PathBuf {
    let root = std::env::var_os("SystemRoot").map(PathBuf::from).unwrap_or_else(|| r"C:\Windows".into());
    root.join("System32").join("curl.exe")
}
#[cfg(not(windows))]
fn curl() -> PathBuf { "/usr/bin/curl".into() }

fn run_curl(args: &[&str]) -> Result<Vec<u8>> {
    let mut cmd = Command::new(curl());
    cmd.args(["-sSfL", "--max-time", "120", "-H", "User-Agent: ContextSwitch-updater"]).args(args);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let out = cmd.output().context("no se pudo ejecutar curl")?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        bail!("no se pudo conectar con GitHub{}", if err.is_empty() { String::new() } else { format!(": {err}") });
    }
    Ok(out.stdout)
}

/// "v0.10.2" → [0, 10, 2]. Lo que no es número se ignora.
fn parse_version(v: &str) -> Vec<u32> {
    v.trim().trim_start_matches(['v', 'V'])
        .split(['.', '-', '+'])
        .map_while(|p| p.parse().ok())
        .collect()
}

pub fn is_newer(latest: &str, current: &str) -> bool {
    let (mut a, mut b) = (parse_version(latest), parse_version(current));
    let n = a.len().max(b.len());
    a.resize(n, 0);
    b.resize(n, 0);
    a > b
}

/// Interpreta la respuesta de `releases/latest`.
pub fn parse_release(json: &[u8], current: &str) -> Result<UpdateInfo> {
    let v: serde_json::Value = serde_json::from_slice(json).context("respuesta de GitHub inválida")?;
    let tag = v["tag_name"].as_str().ok_or_else(|| anyhow!("la release no tiene versión"))?;
    let installer_url = v["assets"].as_array().into_iter().flatten()
        .filter(|a| a["name"].as_str().is_some_and(|n| n.ends_with(INSTALLER_SUFFIX)))
        .filter_map(|a| a["browser_download_url"].as_str())
        .find(|u| u.starts_with(DOWNLOAD_PREFIX))
        .map(String::from);
    let latest = tag.trim_start_matches(['v', 'V']).to_string();
    Ok(UpdateInfo {
        available: is_newer(&latest, current) && installer_url.is_some(),
        current: current.to_string(),
        latest,
        installer_url,
        notes: v["body"].as_str().unwrap_or_default().to_string(),
    })
}

pub fn check() -> Result<UpdateInfo> {
    let body = run_curl(&["-H", "Accept: application/vnd.github+json", LATEST_API])?;
    parse_release(&body, &current_version())
}

/// Ejecutable instalado (instalador NSIS por usuario).
#[cfg(windows)]
fn installed_exe() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_default();
    base.join("ContextSwitch").join("contextswitch.exe")
}

/// Mac: descarga la imagen .dmg y la abre (el Finder la monta y muestra la app para
/// arrastrarla a Aplicaciones). La app sigue corriendo hasta que el usuario la reemplace.
#[cfg(not(windows))]
pub fn download_and_launch(url: &str) -> Result<()> {
    if !url.starts_with(DOWNLOAD_PREFIX) || !url.ends_with(".dmg") {
        bail!("URL de actualización no permitida");
    }
    let dir = std::env::temp_dir().join("ContextSwitch-update");
    std::fs::create_dir_all(&dir)?;
    let dmg = dir.join("ContextSwitch.dmg");
    let _ = std::fs::remove_file(&dmg);
    run_curl(&["-o", &dmg.to_string_lossy(), url])?;
    let bytes = std::fs::read(&dmg).context("no se pudo leer la imagen descargada")?;
    // Una .dmg termina con un bloque de 512 bytes que empieza con "koly".
    if bytes.len() < 100_000 || !bytes[bytes.len() - 512..].starts_with(b"koly") {
        let _ = std::fs::remove_file(&dmg);
        bail!("el archivo descargado no es una imagen de disco válida");
    }
    let ok = Command::new("/usr/bin/open").arg(&dmg).status().map(|s| s.success()).unwrap_or(false);
    if !ok { bail!("no se pudo abrir la imagen descargada"); }
    Ok(())
}

/// Descarga el instalador y deja lanzado (desacoplado) "esperar → instalar en silencio →
/// abrir la app". El llamador debe cerrar la app enseguida para liberar el exe.
#[cfg(windows)]
pub fn download_and_launch(url: &str) -> Result<()> {
    if !url.starts_with(DOWNLOAD_PREFIX) || !url.ends_with(".exe") {
        bail!("URL de actualización no permitida");
    }
    let dir = std::env::temp_dir().join("ContextSwitch-update");
    std::fs::create_dir_all(&dir)?;
    let setup = dir.join("ContextSwitch-setup.exe");
    let _ = std::fs::remove_file(&setup);
    run_curl(&["-o", &setup.to_string_lossy(), url])?;

    let bytes = std::fs::read(&setup).context("no se pudo leer el instalador descargado")?;
    if bytes.len() < 100_000 || !bytes.starts_with(b"MZ") {
        let _ = std::fs::remove_file(&setup);
        bail!("el archivo descargado no es un instalador válido");
    }

    // ping = espera ~2 s a que esta instancia termine de cerrarse.
    let script = format!(
        r#"/C ping -n 3 127.0.0.1 >nul & "{}" /S & start "" "{}""#,
        setup.display(), installed_exe().display()
    );
    Command::new("cmd.exe")
        .raw_arg(script)
        .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
        .spawn()
        .context("no se pudo lanzar el instalador")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compara_versiones() {
        assert!(is_newer("0.4.1", "0.4.0"));
        assert!(is_newer("v0.10.0", "0.9.9"));
        assert!(is_newer("1.0", "0.99.99"));
        assert!(!is_newer("0.4.0", "0.4.0"));
        assert!(!is_newer("v0.3.0", "0.4.0"));
        assert!(!is_newer("0.4", "0.4.0"));
    }

    const RELEASE: &str = r#"{
        "tag_name": "v0.5.0", "body": "Novedades",
        "assets": [
            {"name": "contextswitch_companion-0.3.2.xpi", "browser_download_url": "https://github.com/LUCAS-BTNCR/ContextSwitch/releases/download/v0.5.0/c.xpi"},
            {"name": "ContextSwitch_0.5.0_x64-setup.exe", "browser_download_url": "https://github.com/LUCAS-BTNCR/ContextSwitch/releases/download/v0.5.0/ContextSwitch_0.5.0_x64-setup.exe"},
            {"name": "ContextSwitch_0.5.0_aarch64.dmg", "browser_download_url": "https://github.com/LUCAS-BTNCR/ContextSwitch/releases/download/v0.5.0/ContextSwitch_0.5.0_aarch64.dmg"},
            {"name": "ContextSwitch_0.5.0_x64.dmg", "browser_download_url": "https://github.com/LUCAS-BTNCR/ContextSwitch/releases/download/v0.5.0/ContextSwitch_0.5.0_x64.dmg"}
        ]}"#;

    #[test]
    fn release_mas_nueva_con_instalador() {
        let i = parse_release(RELEASE.as_bytes(), "0.4.0").unwrap();
        assert!(i.available);
        assert_eq!(i.latest, "0.5.0");
        assert!(i.installer_url.unwrap().ends_with(&format!("ContextSwitch_0.5.0{INSTALLER_SUFFIX}")), "el instalador de ESTE sistema");
        assert_eq!(i.notes, "Novedades");
    }

    #[test]
    fn misma_version_no_es_actualizacion() {
        assert!(!parse_release(RELEASE.as_bytes(), "0.5.0").unwrap().available);
    }

    #[test]
    fn sin_instalador_o_de_otro_origen_no_se_ofrece() {
        let r = format!(r#"{{"tag_name":"v9.0.0","assets":[{{"name":"x{INSTALLER_SUFFIX}","browser_download_url":"https://evil.example/x{INSTALLER_SUFFIX}"}}]}}"#);
        let i = parse_release(r.as_bytes(), "0.4.0").unwrap();
        assert!(!i.available);
        assert!(i.installer_url.is_none());
    }

    #[test]
    fn rechaza_urls_ajenas() {
        assert!(download_and_launch("https://evil.example/ContextSwitch_x64-setup.exe").is_err());
        assert!(download_and_launch("https://evil.example/ContextSwitch_aarch64.dmg").is_err());
    }

    /// Contra GitHub real: `cargo test -- --ignored updater`.
    #[test]
    #[ignore]
    fn check_contra_github() {
        let i = check().unwrap();
        assert!(!i.latest.is_empty());
        assert!(i.installer_url.is_some(), "la última release debería tener instalador");
    }
}
