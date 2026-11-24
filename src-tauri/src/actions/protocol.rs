//! Mensajes de ContextSwitch a la extensión del navegador (igual en Windows y Mac).
//! Viajan como una línea JSON por el pipe (Windows) o el socket Unix (Mac) hasta
//! cs-bridge, que los reenvía a la extensión por Native Messaging.

use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ZenCommand<'a> {
    #[serde(rename_all = "camelCase")]
    Activate {
        workspace: Option<&'a str>,
        container: Option<&'a str>,
        urls: &'a [String],
        unload_inactive: bool,
    },
    #[serde(rename_all = "camelCase")]
    UnloadInactive,
    /// Marca como "de un modo" las ventanas ya abiertas de esos sitios/contenedor.
    #[serde(rename_all = "camelCase")]
    AdoptWindows {
        container: Option<&'a str>,
        urls: &'a [String],
    },
    /// Una ventana nueva por URL, en orden y en el contenedor del workspace.
    #[serde(rename_all = "camelCase")]
    OpenWindows {
        container: Option<&'a str>,
        urls: &'a [String],
    },
}

/// Contrato del protocolo con la extensión (extension-zen/lib.js). Si cambia un nombre
/// acá sin cambiarlo allá, la extensión ignora el mensaje sin avisar: estos tests lo frenan.
/// Los mismos JSON se usan en tests/extension/companion.test.cjs.
#[cfg(test)]
mod tests {
    use super::ZenCommand;
    use serde_json::json;

    fn j(c: &ZenCommand) -> serde_json::Value { serde_json::to_value(c).unwrap() }

    #[test]
    fn activate() {
        let urls = vec!["https://github.com/".to_string()];
        assert_eq!(j(&ZenCommand::Activate { workspace: Some("Work"), container: Some("Work"), urls: &urls, unload_inactive: true }),
                   json!({"type":"activate","workspace":"Work","container":"Work","urls":["https://github.com/"],"unloadInactive":true}));
    }

    #[test]
    fn open_windows_y_adopt() {
        let urls = vec!["https://www.youtube.com/".to_string()];
        assert_eq!(j(&ZenCommand::OpenWindows { container: Some("Personal"), urls: &urls }),
                   json!({"type":"openWindows","container":"Personal","urls":["https://www.youtube.com/"]}));
        assert_eq!(j(&ZenCommand::AdoptWindows { container: None, urls: &urls }),
                   json!({"type":"adoptWindows","container":null,"urls":["https://www.youtube.com/"]}));
    }

    #[test]
    fn unload_inactive() {
        assert_eq!(j(&ZenCommand::UnloadInactive), json!({"type":"unloadInactive"}));
    }

    #[test]
    fn cada_mensaje_es_una_sola_linea() {
        // cs-bridge separa mensajes por '\n': un salto dentro del JSON partiría el mensaje.
        let urls = vec!["https://a.com/x\ny".to_string()];
        let s = serde_json::to_string(&ZenCommand::OpenWindows { container: Some("A\nB"), urls: &urls }).unwrap();
        assert!(!s.contains('\n'));
    }
}
