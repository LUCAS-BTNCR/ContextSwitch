pub mod audio;
pub mod bridge;
pub mod icons;
pub mod layout;
pub mod window_ops;
pub mod windows_enum;

use crate::profiles::{AppKind, AppRule, EnterAction, ExitAction, LayoutSnapshot, Profile};
use std::{thread, time::Duration};

pub fn ensure_com() {
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
    // S_FALSE / RPC_E_CHANGED_MODE = ya inicializado en este hilo: OK.
    unsafe { let _ = CoInitializeEx(None, COINIT_MULTITHREADED); }
}

#[derive(Default)]
pub struct Report {
    pub undo_layout: Option<LayoutSnapshot>,
    pub warnings: Vec<String>,
}

/// Secuencia de activación. Corre en el hilo worker, nunca en el hook.
pub fn activate(prev: Option<&Profile>, next: &Profile) -> Report {
    ensure_com();
    let mut r = Report::default();
    let uses = |p: &str| next.apps.iter().any(|a| a.process_name.eq_ignore_ascii_case(p));

    // 1) Lista de exclusión: cierre amable.
    for proc_name in &next.close_on_enter {
        window_ops::close_gracefully(proc_name);
    }

    // 1b) "Solo las apps de este modo": cierre amable (WM_CLOSE) de todo lo demás.
    //      Va antes de lanzar nada, así las apps nuevas nunca reciben el cierre.
    if next.close_others {
        let own = std::env::current_exe().ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));
        let mut targets: Vec<String> = windows_enum::top_windows().into_iter()
            .filter(|w| !w.cloaked)
            .map(|w| w.process_name)
            .filter(|p| !keeps_open(next, p, own.as_deref()))
            .collect();
        targets.sort();
        targets.dedup();
        for p in &targets { window_ops::close_gracefully(p); }
    }

    // 2) Acción al salir del perfil anterior (si el nuevo no usa esa app).
    if let Some(prev) = prev {
        for app in prev.apps.iter().filter(|a| !uses(&a.process_name)) {
            match app.on_exit {
                ExitAction::Keep => {}
                ExitAction::Minimize => {
                    window_ops::minimize(&app.process_name);
                    if app.kind == AppKind::BrowserZen {
                        let _ = bridge::send(&bridge::ZenCommand::UnloadInactive);
                    }
                }
                ExitAction::Close => { window_ops::close_gracefully(&app.process_name); }
            }
        }
    }

    // 3) Audio.
    if let Some(a) = &next.audio {
        if let Err(e) = audio::set_default_output(&a.output_device_id, a.set_communications_too) {
            r.warnings.push(format!("Audio '{}': {e}", a.output_device_name));
        }
    }

    // 4) Apps / workspaces.
    let mut launched_any = false;
    for app in &next.apps {
        launched_any |= enter_app(app, &mut r);
    }

    // 5) Layout (si se lanzaron apps, darles tiempo a crear su ventana).
    if let Some(l) = &next.layout {
        if launched_any { thread::sleep(Duration::from_millis(1500)); }
        r.undo_layout = Some(layout::apply(l));
    }
    r
}

/// Procesos del sistema que nunca se cierran con "cerrar todo lo demás":
/// cerrarlos rompe la barra de tareas, el menú Inicio, el teclado táctil o la pantalla de bloqueo.
const NEVER_CLOSE: &[&str] = &[
    "explorer.exe", "contextswitch.exe", "cs-bridge.exe",
    "shellexperiencehost.exe", "startmenuexperiencehost.exe", "searchhost.exe", "searchapp.exe",
    "textinputhost.exe", "lockapp.exe", "dwm.exe", "taskmgr.exe",
];

/// true si `proc` (en minúsculas) debe quedar abierto al activar `next` con close_others.
fn keeps_open(next: &Profile, proc: &str, own_exe: Option<&str>) -> bool {
    NEVER_CLOSE.contains(&proc)
        || own_exe == Some(proc)
        || next.apps.iter().any(|a| a.process_name.eq_ignore_ascii_case(proc))
        || next.keep_open.iter().any(|k| k.eq_ignore_ascii_case(proc))
}

/// Devuelve true si lanzó un proceso nuevo.
fn enter_app(app: &AppRule, r: &mut Report) -> bool {
    if app.on_enter == EnterAction::None { return false; }
    let force = app.on_enter == EnterAction::Launch;

    if app.kind == AppKind::BrowserZen {
        let running = !windows_enum::windows_of(&app.process_name).is_empty();
        if running {
            let sent = bridge::send(&bridge::ZenCommand::Activate {
                workspace: app.workspace.as_deref(),
                container: app.container.as_deref().or(app.workspace.as_deref()),
                urls: &app.open,
                unload_inactive: app.unload_inactive_tabs,
            });
            if sent.is_err() {
                r.warnings.push("Zen: extensión companion no conectada; se abren las URLs sin workspace.".into());
                for u in &app.open { window_ops::shell_open(&app.exe_path, u); }
            }
            window_ops::launch_or_focus(&app.process_name, &app.exe_path, &[], false);
            return false;
        }
        // No estaba abierto: se lanza; la extensión aplicará el workspace al conectarse
        // en el próximo cambio. Las URLs se pasan como argumentos.
        let mut args = app.args.clone();
        args.extend(app.open.iter().cloned());
        window_ops::launch_or_focus(&app.process_name, &app.exe_path, &args, true);
        return true;
    }

    // Si ya corre: se enfoca y cada destino se pasa al exe (ej. `code "C:\carpeta"`).
    // Si hay que lanzarla: los destinos van como argumentos del mismo lanzamiento; abrirlos
    // con ShellExecute(destino) usaría el handler por defecto (una carpeta → Explorador).
    if !force && window_ops::launch_or_focus(&app.process_name, &app.exe_path, &[], false) {
        for target in &app.open {
            window_ops::shell_open(&app.exe_path, &window_ops::quote_args(std::slice::from_ref(target)));
        }
        return false;
    }
    let mut args = app.args.clone();
    args.extend(app.open.iter().cloned());
    window_ops::launch_or_focus(&app.process_name, &app.exe_path, &args, true);
    true
}

#[cfg(test)]
mod tests {
    use super::keeps_open;
    use crate::profiles::Profile;

    fn profile() -> Profile {
        serde_json::from_value(serde_json::json!({
            "id": "gaming", "name": "Gaming",
            "trigger": { "modifiers": ["ctrl", "shift"], "key": "G", "vk": 71 },
            "apps": [{ "id": "a", "label": "Discord", "kind": "chat",
                       "exePath": "C:\\x\\Discord.exe", "processName": "Discord.exe" }],
            "closeOthers": true,
            "keepOpen": ["WindowsTerminal.exe"]
        })).unwrap()
    }

    #[test]
    fn close_others_respeta_apps_shell_y_keep_open() {
        let p = profile();
        assert!(keeps_open(&p, "discord.exe", None));          // app del modo (case-insensitive)
        assert!(keeps_open(&p, "windowsterminal.exe", None));  // keepOpen
        assert!(keeps_open(&p, "explorer.exe", None));         // shell
        assert!(keeps_open(&p, "contextswitch-dev.exe", Some("contextswitch-dev.exe"))); // la propia app
        assert!(!keeps_open(&p, "chrome.exe", None));
        assert!(!keeps_open(&p, "code.exe", Some("contextswitch.exe")));
    }
}
