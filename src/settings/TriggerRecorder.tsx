import type React from "react";
import { useEffect, useRef, useState } from "react";
import { api } from "../api";
import { MODIFIER_CODES, MOD_LABEL, chordWarnings, codeToVk, modsFromEvent } from "../keymap";
import { isMac } from "../platform";
import type { ChordEvent, Modifier, Settings, Trigger } from "../types";

type Rec =
  | { phase: "idle" }
  | { phase: "wait1"; live: Modifier[] }
  | { phase: "wait2"; mods: Modifier[]; code: string; label: string; t0: number; released: boolean };

interface Props {
  trigger: Trigger;
  settings: Settings;
  profileId: string;
  onTrigger: (t: Trigger) => void;
  onSettings: (patch: Partial<Settings>) => void;
}

const sameMods = (a: Modifier[], b: Modifier[]) => a.length === b.length && a.every((m) => b.includes(m));

function Keycaps({ mods, keyLabel, double = true }: { mods: Modifier[]; keyLabel?: string; double?: boolean }) {
  return (
    <span className="keycaps">
      {mods.map((m) => <kbd key={m}>{MOD_LABEL[m]}</kbd>)}
      {keyLabel && (
        <>
          <span className="plus">+</span>
          <kbd className="trigger">{keyLabel}</kbd>
          {double && <><span className="arrow">→</span><kbd className="trigger">{keyLabel}</kbd></>}
        </>
      )}
    </span>
  );
}

export function TriggerRecorder({ trigger, settings, profileId, onTrigger, onSettings }: Props) {
  const [rec, setRec] = useState<Rec>({ phase: "idle" });
  const [testing, setTesting] = useState(false);
  const [hits, setHits] = useState<ChordEvent[]>([]);
  const padRef = useRef<HTMLDivElement>(null);
  const windowMs = settings.doubleTapWindowMs;

  // Modo prueba: el hook real detecta y el backend NO ejecuta acciones.
  useEffect(() => {
    if (!testing) return;
    api.setTestMode(true);
    const un = api.onChord((e) => e.test && setHits((h) => [e, ...h].slice(0, 4)));
    return () => { api.setTestMode(false); un.then((f) => f()); };
  }, [testing]);

  const start = () => { setRec({ phase: "wait1", live: [] }); setTimeout(() => padRef.current?.focus(), 0); };
  const cancel = () => setRec({ phase: "idle" });

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (rec.phase === "idle") return;
    e.preventDefault();
    if (e.code === "Escape") return cancel();
    const mods = modsFromEvent(e);

    if (MODIFIER_CODES.has(e.code)) {
      if (rec.phase === "wait1") setRec({ phase: "wait1", live: mods });
      return;
    }
    if (e.repeat) return; // auto-repeat nunca cuenta
    const vk = codeToVk(e.code);
    if (vk === null || mods.length === 0) return;

    const label = e.code.replace(/^(Key|Digit)/, "");
    if (rec.phase === "wait2" && rec.released && rec.code === e.code && sameMods(rec.mods, mods)
        && e.timeStamp - rec.t0 <= windowMs) {
      onTrigger({ modifiers: mods, key: label, vk });
      setRec({ phase: "idle" });
      return;
    }
    setRec({ phase: "wait2", mods, code: e.code, label, t0: e.timeStamp, released: false });
  };

  const onKeyUp = (e: React.KeyboardEvent) => {
    if (rec.phase === "wait2" && e.code === rec.code) setRec({ ...rec, released: true });
    if (rec.phase === "wait1") setRec({ phase: "wait1", live: modsFromEvent(e) });
  };

  const warnings = chordWarnings(trigger.modifiers, isMac());

  return (
    <section className="card">
      <header className="card-head">
        <span className="step">1</span>
        <div>
          <h2>Disparador</h2>
          <p className="muted">Mantené los modificadores y tocá la tecla <b>dos veces</b>.</p>
        </div>
      </header>

      <div
        ref={padRef}
        tabIndex={0}
        className={`recorder ${rec.phase !== "idle" ? "is-recording" : ""}`}
        onKeyDown={onKeyDown}
        onKeyUp={onKeyUp}
        onBlur={cancel}
      >
        {rec.phase === "idle" && (
          <>
            <Keycaps mods={trigger.modifiers} keyLabel={trigger.key} />
            <button className="btn" onClick={start}>Grabar</button>
          </>
        )}
        {rec.phase === "wait1" && (
          <>
            <Keycaps mods={rec.live} />
            <span className="hint">{rec.live.length ? "…ahora la tecla disparadora" : (isMac() ? "Mantené ⌃ / ⇧ / ⌥ / ⌘" : "Mantené Ctrl / Shift / Alt / Win")}</span>
            <span className="dots"><i /><i /></span>
          </>
        )}
        {rec.phase === "wait2" && (
          <>
            <Keycaps mods={rec.mods} keyLabel={rec.label} double={false} />
            <span className="hint">Otra vez, sin soltar los modificadores</span>
            <span className="dots"><i className="on" /><i /></span>
          </>
        )}
        {rec.phase !== "idle" && <button className="btn ghost" onMouseDown={cancel}>Esc</button>}
      </div>

      {warnings.map((w) => <p key={w} className="warn">⚠ {w}</p>)}

      <div className="row">
        <label className="field grow">
          <span>Ventana entre taps <b>{windowMs} ms</b></span>
          <input type="range" min={200} max={800} step={25} value={windowMs}
                 onChange={(e) => onSettings({ doubleTapWindowMs: Number(e.target.value) })} />
        </label>
        <label className="switch">
          <input type="checkbox" checked={settings.swallowTrigger}
                 onChange={(e) => onSettings({ swallowTrigger: e.target.checked })} />
          <span>Reservar combinación<small>La app en foco no recibe la tecla</small></span>
        </label>
      </div>

      <div className={`testpad ${testing ? "on" : ""}`}>
        <button className="btn ghost" onClick={() => setTesting((t) => !t)}>
          {testing ? "Terminar prueba" : "Probar ahora"}
        </button>
        {testing && hits.length === 0 && <span className="muted">Hacé el gesto en cualquier ventana. No se ejecuta ninguna acción.</span>}
        <ul>
          {hits.map((h, i) => (
            <li key={i} className={h.profileId === profileId ? "ok" : ""}>
              ✓ {h.profileName} <span className="muted">· {h.deltaMs} ms entre taps</span>
            </li>
          ))}
        </ul>
      </div>
    </section>
  );
}
