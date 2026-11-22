import { useEffect, useState } from "react";
import { api } from "./api";
import { MOD_LABEL } from "./keymap";
import type { Config } from "./types";
import "./settings/settings.css";

/**
 * Ventanita que aparece al iniciar con Windows: "¿Deseas aplicar algún contexto?".
 * Elegir un modo lo activa; "No, gracias", Esc o cerrar la ventana = no hacer nada.
 */
export function StartupPicker() {
  const [config, setConfig] = useState<Config | null>(null);

  useEffect(() => { api.getConfig().then(setConfig).catch(() => {}); }, []);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") api.pickerChoose(null); };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  if (!config) return <div className="boot">Cargando…</div>;
  const profiles = config.profiles.filter((p) => p.enabled);

  return (
    <div className="startup">
      <h1>¿Deseas aplicar algún contexto?</h1>
      <p className="muted">Elegí uno para empezar, o cerrá esta ventana para seguir como estás.</p>
      <div className="startup-list">
        {profiles.map((p, i) => (
          <button key={p.id} className="startup-item" autoFocus={i === 0} onClick={() => api.pickerChoose(p.id)}>
            <span className="nav-icon">{p.icon}</span>
            <span className="nav-name">{p.name}</span>
            {config.settings.activeProfileId === p.id && <span className="badge">último usado</span>}
            <span className="nav-chord">
              {p.trigger.modifiers.map((m) => MOD_LABEL[m]).join("+")}+{p.trigger.key}×2
            </span>
          </button>
        ))}
      </div>
      <div className="startup-foot">
        <button className="btn ghost" onClick={() => api.pickerOpenSettings()}>Abrir ajustes</button>
        <button className="btn" onClick={() => api.pickerChoose(null)}>No, gracias</button>
      </div>
    </div>
  );
}
