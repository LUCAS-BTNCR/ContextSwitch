import { useEffect, useMemo, useRef, useState } from "react";
import { api } from "./api";
import { MOD_LABEL } from "./keymap";
import { PrivacyBadge } from "./settings/PrivacyBadge";
import { SettingsTab } from "./settings/SettingsTab";
import type { Config, Profile, Settings } from "./types";
import "./settings/settings.css";

type SaveState = "idle" | "saving" | "saved" | "error";

const chordKey = (p: Profile) => [...p.trigger.modifiers].sort().join("+") + "|" + p.trigger.vk;

export default function App() {
  const [config, setConfig] = useState<Config | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [save, setSave] = useState<SaveState>("idle");
  const timer = useRef<number>();

  useEffect(() => {
    api.getConfig().then((c) => { setConfig(c); setSelected(c.profiles[0]?.id ?? null); });
  }, []);

  // Avisos de la última activación (ej. una app que no se pudo cerrar).
  const [warnings, setWarnings] = useState<string[]>([]);
  useEffect(() => {
    const off = api.onActivated(([, w]) => setWarnings(w));
    return () => { off.then((f) => f()); };
  }, []);

  // Autoguardado con debounce: el hook se reconfigura al instante y el área de prueba
  // siempre refleja lo que ves en pantalla.
  const commit = (next: Config) => {
    setConfig(next);
    setSave("saving");
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => {
      api.saveConfig(next).then(() => setSave("saved")).catch(() => setSave("error"));
    }, 500);
  };

  const conflicts = useMemo(() => {
    const seen = new Map<string, string>();
    const out = new Map<string, string>();
    for (const p of config?.profiles ?? []) {
      const k = chordKey(p);
      if (seen.has(k)) out.set(p.id, seen.get(k)!); else seen.set(k, p.name);
    }
    return out;
  }, [config]);

  if (!config) return <div className="boot">Cargando…</div>;
  const profile = config.profiles.find((p) => p.id === selected);

  const patchProfile = (id: string, patch: Partial<Profile>) =>
    commit({ ...config, profiles: config.profiles.map((p) => (p.id === id ? { ...p, ...patch } : p)) });
  const patchSettings = (patch: Partial<Settings>) => commit({ ...config, settings: { ...config.settings, ...patch } });

  const addProfile = () => {
    const id = `modo-${Date.now().toString(36)}`;
    commit({
      ...config,
      profiles: [...config.profiles, {
        id, name: "Nuevo modo", icon: "✨", enabled: true,
        trigger: { modifiers: ["ctrl", "shift"], key: "N", vk: 0x4e },
        audio: null, apps: [], closeOnEnter: [], layout: null,
      }],
    });
    setSelected(id);
  };

  const deleteProfile = (id: string) => {
    const rest = config.profiles.filter((p) => p.id !== id);
    commit({ ...config, profiles: rest });
    setSelected(rest[0]?.id ?? null);
  };

  return (
    <div className="shell">
      <aside className="sidebar">
        <div className="brand">ContextSwitch</div>
        <nav>
          {config.profiles.map((p) => (
            <button key={p.id} className={`nav-item ${p.id === selected ? "on" : ""} ${p.enabled ? "" : "off"}`}
                    onClick={() => setSelected(p.id)}>
              <span className="nav-icon">{p.icon}</span>
              <span className="nav-name">{p.name}</span>
              {config.settings.activeProfileId === p.id && <span className="badge">activo</span>}
              <span className="nav-chord">
                {p.trigger.modifiers.map((m) => MOD_LABEL[m]).join("+")}+{p.trigger.key}×2
              </span>
            </button>
          ))}
        </nav>
        <button className="btn ghost wide" onClick={addProfile}>+ Nuevo modo</button>
        <label className="switch small">
          <input type="checkbox" checked={config.settings.startWithWindows}
                 onChange={(e) => patchSettings({ startWithWindows: e.target.checked })} />
          <span>Iniciar con Windows</span>
        </label>
        <div className="sidebar-foot">
          <PrivacyBadge compact />
          <span className={`save-state ${save}`}>
            {save === "saving" ? "Guardando…" : save === "saved" ? "Guardado ✓" : save === "error" ? "Error al guardar" : ""}
          </span>
        </div>
      </aside>

      <main className="content">
        {warnings.length > 0 && (
          <div className="warn activation-warn" role="status">
            <ul>{warnings.map((w, i) => <li key={i}>{w}</li>)}</ul>
            <button className="icon-btn" aria-label="Cerrar aviso" onClick={() => setWarnings([])}>✕</button>
          </div>
        )}
        {profile ? (
          <SettingsTab
            key={profile.id}
            profile={profile}
            settings={config.settings}
            conflict={conflicts.get(profile.id) ?? null}
            onProfile={(patch) => patchProfile(profile.id, patch)}
            onSettings={patchSettings}
            onActivate={() => api.activateProfile(profile.id)}
            onDelete={() => deleteProfile(profile.id)}
          />
        ) : (
          <p className="empty">Creá tu primer modo para empezar.</p>
        )}
      </main>
    </div>
  );
}
