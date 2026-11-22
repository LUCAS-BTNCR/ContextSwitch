import { useEffect, useState } from "react";
import type { Profile, Settings } from "../types";
import { AppRulesEditor } from "./AppRulesEditor";
import { AudioDisplaySection } from "./AudioDisplaySection";
import { PrivacyBadge } from "./PrivacyBadge";
import { TriggerRecorder } from "./TriggerRecorder";

interface Props {
  profile: Profile;
  settings: Settings;
  conflict: string | null;
  onProfile: (patch: Partial<Profile>) => void;
  onSettings: (patch: Partial<Settings>) => void;
  onActivate: () => void;
  onDelete: () => void;
}

export function SettingsTab({ profile, settings, conflict, onProfile, onSettings, onActivate, onDelete }: Props) {
  return (
    <div className="settings">
      <div className="profile-head">
        <input className="emoji" value={profile.icon ?? ""} maxLength={2}
               onChange={(e) => onProfile({ icon: e.target.value })} aria-label="Ícono" />
        <input className="name" value={profile.name} onChange={(e) => onProfile({ name: e.target.value })} aria-label="Nombre del modo" />
        <label className="switch inline">
          <input type="checkbox" checked={profile.enabled} onChange={(e) => onProfile({ enabled: e.target.checked })} />
          <span>Activo</span>
        </label>
        <button className="btn primary" onClick={onActivate}>Activar ahora</button>
        <button className="icon-btn danger" onClick={onDelete} aria-label="Eliminar modo">🗑</button>
      </div>

      {conflict && <p className="warn">⚠ Esta combinación ya la usa “{conflict}”.</p>}

      <TriggerRecorder
        trigger={profile.trigger}
        settings={settings}
        profileId={profile.id}
        onTrigger={(trigger) => onProfile({ trigger })}
        onSettings={onSettings}
      />
      <AppRulesEditor rules={profile.apps} onChange={(apps) => onProfile({ apps })} />

      <section className="card">
        <header className="card-head">
          <span className="step">2b</span>
          <div>
            <h2>Cerrar al entrar</h2>
            <p className="muted">Qué se cierra amablemente al activar este modo. Si hay cambios sin guardar, cada app pregunta.</p>
          </div>
        </header>

        <label className="switch">
          <input type="checkbox" checked={!!profile.closeOthers}
                 onChange={(e) => onProfile({ closeOthers: e.target.checked })} />
          <span>
            Cerrar todo lo demás · dejar solo las apps de este modo
            <small>Cierra toda app con ventana que no esté en “Apps y Workspaces”. El Explorador, la barra de tareas y ContextSwitch nunca se cierran.</small>
          </span>
        </label>

        {profile.closeOthers ? (
          <label className="field">
            <span>Nunca cerrar <small>(además de las apps del modo)</small></span>
            <ProcessListInput value={profile.keepOpen ?? []} placeholder="windowsterminal.exe, obs64.exe"
                              onChange={(keepOpen) => onProfile({ keepOpen })} />
          </label>
        ) : (
          <label className="field">
            <span>Cerrar solo estas apps</span>
            <ProcessListInput value={profile.closeOnEnter} placeholder="spark.exe, code.exe"
                              onChange={(closeOnEnter) => onProfile({ closeOnEnter })} />
          </label>
        )}
        {profile.closeOthers && profile.apps.length === 0 && (
          <p className="warn">⚠ Este modo no tiene apps: al activarlo se cerraría todo lo que tengas abierto.</p>
        )}
      </section>

      <section className="card">
        <header className="card-head">
          <span className="step">2c</span>
          <div>
            <h2>Al volver a este modo</h2>
            <p className="muted">Retomá exactamente donde lo dejaste.</p>
          </div>
        </header>
        <label className="switch">
          <input type="checkbox" checked={!!profile.restoreSession}
                 onChange={(e) => onProfile({ restoreSession: e.target.checked })} />
          <span>
            Recordar cómo lo dejé
            <small>
              Al salir de este modo se guarda qué apps tenías abiertas y dónde estaba cada ventana. Al volver se reabren
              las que se cerraron y todo vuelve a su lugar. Zen ya conserva sus pestañas; los programas de Windows
              (como el Administrador de tareas) no se reabren.
            </small>
          </span>
        </label>
      </section>

      <AudioDisplaySection
        audio={profile.audio}
        layout={profile.layout}
        onAudio={(audio) => onProfile({ audio })}
        onLayout={(layout) => onProfile({ layout })}
      />
      <PrivacyBadge />
    </div>
  );
}

/** Texto libre "a.exe, b.exe" que se normaliza al salir del campo (permite tipear comas). */
function ProcessListInput({ value, onChange, placeholder }: { value: string[]; onChange: (v: string[]) => void; placeholder?: string }) {
  const [text, setText] = useState(value.join(", "));
  useEffect(() => setText(value.join(", ")), [value.join("|")]);
  const commit = () =>
    onChange(text.split(",").map((s) => s.trim().toLowerCase()).filter(Boolean)
      .map((s) => (s.endsWith(".exe") ? s : `${s}.exe`)));
  return (
    <input className="chips-input" placeholder={placeholder} value={text}
           onChange={(e) => setText(e.target.value)} onBlur={commit}
           onKeyDown={(e) => e.key === "Enter" && commit()} />
  );
}
