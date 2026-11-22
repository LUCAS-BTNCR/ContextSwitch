//! Actualizaciones bajo demanda desde las Releases de GitHub.
//!
//! Solo se conecta cuando el usuario toca "Buscar actualizaciones": nunca en segundo plano.
//! No hay cliente HTTP en la app: se usa el `curl.exe` que trae Windows (System32).

use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

const LATEST_API: &str = "https://api.github.com/repos/LUCAS-BTNCR/ContextSwitch/releases/latest";
/// Solo se descargan instaladores de las Releases de este repo.
const DOWNLOAD_PREFIX: &str = "https://github.com/LUCAS-BTNCR/ContextSwitch/releases/download/";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const DETACHED_PROCESS: u32 = 0x0000_0008;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub available: bool,
    /// URL del instalador `*_x64-setup.exe` de la última release (si tiene).
    pub installer_url: Option<String>,
    pub notes: String,
}

pub fn current_version() -> String {
    // Solo para pruebas de punta a punta: simular que esta copia es más vieja.
    std::env::var("CONTEXTSWITCH_UPDATE_AS_VERSION").unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string())
}

fn curl() -> PathBuf {
    let root = std::env::var_os("SystemRoot").map(PathBuf::from).unwrap_or_else(|| r"C:\Windows".into());
    root.join("System32").join("curl.exe")
}

fn run_curl(args: &[&str]) -> Result<Vec<u8>> {
    let out = Command::new(curl())
        .args(["-sSfL", "--max-time", "120", "-H", "User-Agent: ContextSwitch-updater"])
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .context("no se pudo ejecutar curl.exe")?;
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
        .filter(|a| a["name"].as_str().is_some_and(|n| n.ends_with("_x64-setup.exe")))
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
fn installed_exe() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_default();
    base.join("ContextSwitch").join("contextswitch.exe")
}

/// Descarga el instalador y deja lanzado (desacoplado) "esperar → instalar en silencio →
/// abrir la app". El llamador debe cerrar la app enseguida para liberar el exe.
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
            {"name": "ContextSwitch_0.5.0_x64-setup.exe", "browser_download_url": "https://github.com/LUCAS-BTNCR/ContextSwitch/releases/download/v0.5.0/ContextSwitch_0.5.0_x64-setup.exe"}
        ]}"#;

    #[test]
    fn release_mas_nueva_con_instalador() {
        let i = parse_release(RELEASE.as_bytes(), "0.4.0").unwrap();
        assert!(i.available);
        assert_eq!(i.latest, "0.5.0");
        assert!(i.installer_url.unwrap().ends_with("ContextSwitch_0.5.0_x64-setup.exe"));
        assert_eq!(i.notes, "Novedades");
    }

    #[test]
    fn misma_version_no_es_actualizacion() {
        assert!(!parse_release(RELEASE.as_bytes(), "0.5.0").unwrap().available);
    }

    #[test]
    fn sin_instalador_o_de_otro_origen_no_se_ofrece() {
        let r = r#"{"tag_name":"v9.0.0","assets":[{"name":"x_x64-setup.exe","browser_download_url":"https://evil.example/x_x64-setup.exe"}]}"#;
        let i = parse_release(r.as_bytes(), "0.4.0").unwrap();
        assert!(!i.available);
        assert!(i.installer_url.is_none());
    }

    #[test]
    fn rechaza_urls_ajenas() {
        assert!(download_and_launch("https://evil.example/ContextSwitch_x64-setup.exe").is_err());
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
