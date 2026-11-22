import { useEffect, useState } from "react";
import { api } from "../api";
import type { AppKind, AppRule, ExitAction, MonitorInfo, Placement, RunningApp } from "../types";
import { AppIcon } from "./AppIcon";
import { LayoutPreview, PlacementPicker, type PreviewItem } from "./Placement";

/** Copia de `arr` con `arr[i] = v`, rellenando con null si hace falta. */
export function setAt(arr: (Placement | null)[] | undefined, i: number, v: Placement | null) {
  const out = [...(arr ?? [])];
  while (out.length <= i) out.push(null);
  out[i] = v;
  return out;
}

export const hostOf = (url: string) => {
  try { return new URL(url).hostname.replace(/^www\./, ""); } catch { return url; }
};

/** Une todas las reglas de Zen en la primera, con una ventana por URL (conserva posiciones). */
export function mergeZenRules(rules: AppRule[]): AppRule[] {
  const zen = rules.filter((r) => r.kind === "browser-zen");
  if (zen.length < 2) return rules;
  const open: string[] = [];
  const placements: (Placement | null)[] = [];
  for (const r of zen) {
    r.open.forEach((url, i) => {
      open.push(url);
      // En modo pestañas, placements[0] era la ventana entera: pasa a su primera URL.
      placements.push((r.separateWindows ? r.placements?.[i] : i === 0 ? r.placements?.[0] : null) ?? null);
    });
  }
  const merged: AppRule = { ...zen[0], open, placements, separateWindows: true };
  return rules.filter((r) => r.kind !== "browser-zen" || r === zen[0]).map((r) => (r === zen[0] ? merged : r));
}

/** Qué dibujar en la vista previa: cada ventana con posición fija. */
export function previewItems(rules: AppRule[]): PreviewItem[] {
  const out: PreviewItem[] = [];
  for (const r of rules) {
    if (r.kind === "browser-zen" && r.separateWindows) {
      r.open.forEach((url, i) => { const p = r.placements?.[i]; if (p) out.push({ label: hostOf(url), place: p }); });
    } else {
      const p = r.placements?.[0];
      if (p) out.push({ label: r.label, place: p });
    }
  }
  return out;
}

const MAIL = ["spark.exe", "outlook.exe", "olk.exe", "thunderbird.exe", "mailbird.exe", "hxoutlook.exe"];
const CHAT = ["discord.exe", "slack.exe", "teams.exe", "ms-teams.exe", "telegram.exe", "whatsapp.exe"];

export function guessKind(proc: string): AppKind {
  const p = proc.toLowerCase();
  if (p === "zen.exe") return "browser-zen";
  if (MAIL.includes(p)) return "mail";
  if (CHAT.includes(p)) return "chat";
  return "generic";
}

// Procesos cuya ventana es de un "helper" que no se puede lanzar solo: para abrir la app
// hay que ejecutar otro exe. La ventana se sigue buscando por el proceso del helper.
const LAUNCHERS: Record<string, { label: string; from: RegExp; to: string }> = {
  "steamwebhelper.exe": { label: "Steam", from: /\\bin\\cef\\[^\\]+\\steamwebhelper\.exe$/i, to: "\\steam.exe" },
};
export function launcherFor(app: RunningApp): string {
  const l = LAUNCHERS[app.processName.toLowerCase()];
  return l && l.from.test(app.exePath) ? app.exePath.replace(l.from, l.to) : app.exePath;
}

const EXIT_OPTIONS:{ value: ExitAction; label: string; hint: string }[] = [
  { value: "keep", label: "Dejar abierto", hint: "No se toca" },
  { value: "minimize", label: "Minimizar / liberar RAM", hint: "En Zen también descarga pestañas" },
  { value: "close", label: "Cerrar pidiendo guardar", hint: "WM_CLOSE: la app muestra su diálogo" },
];

const newId = () => Math.random().toString(36).slice(2, 9);

interface Props { rules: AppRule[]; onChange: (rules: AppRule[]) => void }

export function AppRulesEditor({ rules, onChange }: Props) {
  const [running, setRunning] = useState<RunningApp[]>([]);
  const [picking, setPicking] = useState(false);
  const [monitors, setMonitors] = useState<MonitorInfo[]>([]);

  useEffect(() => { api.listMonitors().then(setMonitors).catch(() => setMonitors([])); }, []);

  useEffect(() => { if (picking) api.listRunningApps().then(setRunning); }, [picking]);

  const update = (id: string, patch: Partial<AppRule>) =>
    onChange(rules.map((r) => (r.id === id ? { ...r, ...patch } : r)));
  const remove = (id: string) => onChange(rules.filter((r) => r.id !== id));

  const add = (app: RunningApp) => {
    const kind = guessKind(app.processName);
    onChange([...rules, {
      id: newId(),
      label: LAUNCHERS[app.processName.toLowerCase()]?.label ?? app.processName.replace(/\.exe$/i, ""),
      kind,
      exePath: launcherFor(app),
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

            {r.kind === "browser-zen" && (
              <label className="switch">
                <input type="checkbox" checked={!!r.separateWindows}
                       onChange={(e) => update(r.id, { separateWindows: e.target.checked })} />
                <span>Abrir cada URL en su propia ventana<small>Si no, se abren como pestañas de una misma ventana</small></span>
              </label>
            )}

            {r.kind === "browser-zen" && r.separateWindows ? (
              <div className="field">
                <span>Ventanas <small>(una por URL, cada una con su posición)</small></span>
                <div className="win-rows">
                  {r.open.map((url, i) => (
                    <div key={i} className="win-row">
                      <UrlInput value={url} onChange={(u) => update(r.id, { open: r.open.map((x, j) => (j === i ? u : x)) })} />
                      <PlacementPicker value={r.placements?.[i]} monitors={monitors}
                                       onChange={(p) => update(r.id, { placements: setAt(r.placements, i, p) })} />
                      <button className="icon-btn" aria-label="Quitar ventana" onClick={() => update(r.id, {
                        open: r.open.filter((_, j) => j !== i),
                        placements: (r.placements ?? []).filter((_, j) => j !== i),
                      })}>✕</button>
                    </div>
                  ))}
                </div>
                <button className="btn ghost small" onClick={() => update(r.id, { open: [...r.open, ""] })}>+ Agregar ventana</button>
              </div>
            ) : (
              <>
                <label className="field">
                  <span>URLs o rutas a abrir <small>(una por línea)</small></span>
                  <LinesInput value={r.open} onChange={(open) => update(r.id, { open })} />
                </label>
                <div className="field">
                  <span>Posición de la ventana</span>
                  <PlacementPicker value={r.placements?.[0]} monitors={monitors}
                                   onChange={(p) => update(r.id, { placements: setAt(r.placements, 0, p) })} />
                </div>
              </>
            )}

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

            {r.kind === "browser-zen" && r.onExit === "close" && (
              <p className="tip">
                En Zen, “Cerrar” <b>oculta</b> las ventanas de este modo (salen de la pantalla y de la barra de tareas) y
                descarga sus pestañas, sin cerrarlas: cerrar una ventana de Zen borra los Essentials que tiene. Al volver
                al modo reaparecen en su lugar.
              </p>
            )}

            {r.kind === "browser-zen" && (
              <>
                <label className="switch">
                  <input type="checkbox" checked={!!r.unloadInactiveTabs}
                         onChange={(e) => update(r.id, { unloadInactiveTabs: e.target.checked })} />
                  <span>Descargar pestañas inactivas al entrar<small>Libera RAM; las pestañas siguen ahí</small></span>
                </label>
                <ZenBridgeStatus workspace={r.workspace} container={r.container ?? r.workspace} />
                <p className="tip">
                  En Zen: Ajustes → Tab Management → Workspaces → activá <i>“Switch to workspace where container is set as default”</i> y
                  asigná este contenedor como default del workspace.
                </p>
              </>
            )}
          </article>
        ))}
      </div>

      <LayoutPreview monitors={monitors} items={previewItems(rules)} />

      {hasMail && (
        <p className="tip subtle">
          💡 Si tu cliente de correo es solo laboral, usá un workspace del navegador para tu correo personal:
          ContextSwitch no necesita entrar a tus cuentas para separarlas.
        </p>
      )}
      {hasZen && rules.filter((r) => r.kind === "browser-zen").length > 1 && (
        <div className="warn merge-warn">
          <span>⚠ Hay varias reglas de Zen: así sus URLs se abren como pestañas de la misma ventana.</span>
          <button className="btn small" onClick={() => onChange(mergeZenRules(rules))}>Unir en una regla · una ventana por URL</button>
        </div>
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

/** ¿Está conectada la extensión de Zen? Sin ella no se puede elegir el workspace. */
function ZenBridgeStatus({ workspace, container }: { workspace?: string | null; container?: string | null }) {
  const [ok, setOk] = useState<boolean | null>(null);
  useEffect(() => {
    let alive = true;
    const check = () => api.zenBridgeConnected().then((v) => alive && setOk(v)).catch(() => alive && setOk(false));
    check();
    const t = window.setInterval(check, 3000);
    return () => { alive = false; window.clearInterval(t); };
  }, []);
  if (ok === null) return null;
  return ok ? (
    <p className="status ok">● Extensión de Zen conectada: las ventanas se abren en el workspace
      {workspace ? <> “{workspace}”</> : ""} (contenedor “{container || "—"}”).</p>
  ) : (
    <p className="warn">
      ● Extensión de Zen <b>no conectada</b>: las ventanas se abren igual, pero en el workspace que Zen tenga activo
      {workspace ? <>, no en “{workspace}”</> : ""}. Instalá la extensión firmada (<code>npm run ext:sign</code>) o
      cargala en <code>about:debugging</code> (temporal: se pierde al cerrar Zen).
    </p>
  );
}

/** Input de una URL que confirma al salir del campo (no guarda en cada tecla). */
function UrlInput({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  const [text, setText] = useState(value);
  useEffect(() => setText(value), [value]);
  return (
    <input className="url-input" placeholder="https://…" value={text} onChange={(e) => setText(e.target.value)}
           onBlur={() => text.trim() !== value && onChange(text.trim())}
           onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()} />
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
