import { useEffect, useState } from "react";
import { api } from "../api";
import type { AppKind, AppRule, ExitAction, RunningApp } from "../types";
import { AppIcon } from "./AppIcon";

const MAIL = ["spark.exe", "outlook.exe", "olk.exe", "thunderbird.exe", "mailbird.exe", "hxoutlook.exe"];
const CHAT = ["discord.exe", "slack.exe", "teams.exe", "ms-teams.exe", "telegram.exe", "whatsapp.exe"];

function guessKind(proc: string): AppKind {
  const p = proc.toLowerCase();
  if (p === "zen.exe") return "browser-zen";
  if (MAIL.includes(p)) return "mail";
  if (CHAT.includes(p)) return "chat";
  return "generic";
}

const EXIT_OPTIONS: { value: ExitAction; label: string; hint: string }[] = [
  { value: "keep", label: "Dejar abierto", hint: "No se toca" },
  { value: "minimize", label: "Minimizar / liberar RAM", hint: "En Zen también descarga pestañas" },
  { value: "close", label: "Cerrar pidiendo guardar", hint: "WM_CLOSE: la app muestra su diálogo" },
];

const newId = () => Math.random().toString(36).slice(2, 9);

interface Props { rules: AppRule[]; onChange: (rules: AppRule[]) => void }

export function AppRulesEditor({ rules, onChange }: Props) {
  const [running, setRunning] = useState<RunningApp[]>([]);
  const [picking, setPicking] = useState(false);

  useEffect(() => { if (picking) api.listRunningApps().then(setRunning); }, [picking]);

  const update = (id: string, patch: Partial<AppRule>) =>
    onChange(rules.map((r) => (r.id === id ? { ...r, ...patch } : r)));
  const remove = (id: string) => onChange(rules.filter((r) => r.id !== id));

  const add = (app: RunningApp) => {
    const kind = guessKind(app.processName);
    onChange([...rules, {
      id: newId(),
      label: app.processName.replace(/\.exe$/i, ""),
      kind,
      exePath: app.exePath,
      processName: app.processName,
      open: [],
      onEnter: "launch-or-focus",
      onExit: "keep",
      unloadInactiveTabs: kind === "browser-zen",
    }]);
    setPicking(false);
  };

  const hasMail = rules.some((r) => r.kind === "mail");
  const hasZen = rules.some((r) => r.kind === "browser-zen");

  return (
    <section className="card">
      <header className="card-head">
        <span className="step">2</span>
        <div>
          <h2>Apps y Workspaces</h2>
          <p className="muted">Qué se abre al entrar y qué pasa al salir. Se gestionan ventanas, nunca cuentas.</p>
        </div>
      </header>

      {rules.length === 0 && <p className="empty">Todavía no hay apps en este modo.</p>}

      <div className="rules">
        {rules.map((r) => (
          <article key={r.id} className="rule">
            <div className="rule-top">
              <AppIcon exePath={r.exePath} name={r.processName} size={28} />
              <input className="title-input" value={r.label} onChange={(e) => update(r.id, { label: e.target.value })} />
              <code className="proc">{r.processName}</code>
              <button className="icon-btn" aria-label="Quitar" onClick={() => remove(r.id)}>✕</button>
            </div>

            <div className="grid2">
              <label className="field">
                <span>{r.kind === "browser-zen" ? "Workspace de Zen" : "Workspace / Perfil (opcional)"}</span>
                <input placeholder="ej. Entretenimiento" value={r.workspace ?? ""}
                       onChange={(e) => update(r.id, { workspace: e.target.value || null })} />
              </label>
              {r.kind === "browser-zen" ? (
                <label className="field">
                  <span>Contenedor por defecto del workspace</span>
                  <input placeholder={r.workspace || "ej. Personal"} value={r.container ?? ""}
                         onChange={(e) => update(r.id, { container: e.target.value || null })} />
                </label>
              ) : (
                <label className="field">
                  <span>Ruta del ejecutable</span>
                  <input value={r.exePath} onChange={(e) => update(r.id, { exePath: e.target.value })} />
                </label>
              )}
            </div>

            <label className="field">
              <span>URLs o rutas a abrir <small>(una por línea)</small></span>
              <LinesInput value={r.open} onChange={(open) => update(r.id, { open })} />
            </label>

            <div className="field">
              <span>Al salir de este modo</span>
              <div className="segmented" role="radiogroup">
                {EXIT_OPTIONS.map((o) => (
                  <button key={o.value} role="radio" aria-checked={r.onExit === o.value} title={o.hint}
                          className={r.onExit === o.value ? "on" : ""} onClick={() => update(r.id, { onExit: o.value })}>
                    {o.label}
                  </button>
                ))}
              </div>
            </div>

            {r.kind === "browser-zen" && (
              <>
                <label className="switch">
                  <input type="checkbox" checked={!!r.unloadInactiveTabs}
                         onChange={(e) => update(r.id, { unloadInactiveTabs: e.target.checked })} />
                  <span>Descargar pestañas inactivas al entrar<small>Libera RAM; las pestañas siguen ahí</small></span>
                </label>
                <p className="tip">
                  En Zen: Ajustes → Tab Management → Workspaces → activá <i>“Switch to workspace where container is set as default”</i> y
                  asigná este contenedor como default del workspace.
                </p>
              </>
            )}
          </article>
        ))}
      </div>

      {hasMail && (
        <p className="tip subtle">
          💡 Si tu cliente de correo es solo laboral, usá un workspace del navegador para tu correo personal:
          ContextSwitch no necesita entrar a tus cuentas para separarlas.
        </p>
      )}
      {hasZen && rules.filter((r) => r.kind === "browser-zen").length > 1 && (
        <p className="warn">⚠ Hay más de una regla de Zen en este modo: se aplica la última.</p>
      )}

      {picking ? (
        <div className="picker">
          <div className="picker-head"><b>Apps abiertas ahora</b><button className="btn ghost" onClick={() => setPicking(false)}>Cancelar</button></div>
          <ul>
            {running.map((a) => (
              <li key={a.processName}>
                <button onClick={() => add(a)}>
                  <AppIcon exePath={a.exePath} name={a.processName} />
                  <code>{a.processName}</code><span className="muted">{a.exePath}</span></button>
              </li>
            ))}
          </ul>
        </div>
      ) : (
        <button className="btn" onClick={() => setPicking(true)}>+ Agregar app</button>
      )}
    </section>
  );
}

/** Textarea "una por línea"; normaliza al salir para no comerse los Enter mientras se tipea. */
function LinesInput({ value, onChange }: { value: string[]; onChange: (v: string[]) => void }) {
  const [text, setText] = useState(value.join("\n"));
  useEffect(() => setText(value.join("\n")), [value.join("\n")]);
  return (
    <textarea rows={2} placeholder="https://…" value={text} onChange={(e) => setText(e.target.value)}
              onBlur={() => onChange(text.split("\n").map((l) => l.trim()).filter(Boolean))} />
  );
}
