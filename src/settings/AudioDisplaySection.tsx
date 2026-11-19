import { useEffect, useState } from "react";
import { api } from "../api";
import type { AudioDevice, AudioTarget, LayoutSnapshot } from "../types";

interface Props {
  audio?: AudioTarget | null;
  layout?: LayoutSnapshot | null;
  onAudio: (a: AudioTarget | null) => void;
  onLayout: (l: LayoutSnapshot | null) => void;
}

const fmt = (iso: string) =>
  new Date(iso).toLocaleString(undefined, { dateStyle: "short", timeStyle: "short" });

export function AudioDisplaySection({ audio, layout, onAudio, onLayout }: Props) {
  const [devices, setDevices] = useState<AudioDevice[]>([]);
  const [busy, setBusy] = useState(false);
  const [undoMsg, setUndoMsg] = useState<string | null>(null);

  useEffect(() => { api.listAudioDevices().then(setDevices).catch(() => setDevices([])); }, []);

  const missing = audio && devices.length > 0 && !devices.some((d) => d.id === audio.outputDeviceId);

  const capture = async () => {
    setBusy(true);
    try { onLayout(await api.captureLayout()); } finally { setBusy(false); }
  };

  const undo = async () => {
    const ok = await api.undoLayout();
    setUndoMsg(ok ? "Layout anterior restaurado" : "No hay cambios para deshacer");
    setTimeout(() => setUndoMsg(null), 2500);
  };

  return (
    <section className="card">
      <header className="card-head">
        <span className="step">3</span>
        <div>
          <h2>Audio y Pantallas</h2>
          <p className="muted">Salida de sonido y posición de ventanas para este modo.</p>
        </div>
      </header>

      <label className="field">
        <span>Salida de audio</span>
        <select
          value={audio?.outputDeviceId ?? ""}
          onChange={(e) => {
            const d = devices.find((x) => x.id === e.target.value);
            onAudio(d ? { outputDeviceId: d.id, outputDeviceName: d.name, setCommunicationsToo: audio?.setCommunicationsToo ?? true } : null);
          }}
        >
          <option value="">No cambiar</option>
          {devices.map((d) => (
            <option key={d.id} value={d.id}>{d.name}{d.isDefault ? " (actual)" : ""}</option>
          ))}
          {missing && <option value={audio!.outputDeviceId}>{audio!.outputDeviceName} (desconectado)</option>}
        </select>
      </label>
      {audio && (
        <label className="switch">
          <input type="checkbox" checked={audio.setCommunicationsToo}
                 onChange={(e) => onAudio({ ...audio, setCommunicationsToo: e.target.checked })} />
          <span>Usar también para llamadas<small>Discord, Teams, Meet…</small></span>
        </label>
      )}

      <div className="layout-box">
        <div>
          <b>Layout de ventanas</b>
          <p className="muted">
            {layout
              ? `${layout.windows.length} ventanas guardadas · ${fmt(layout.capturedAt)}`
              : "Acomodá tus ventanas como las querés en este modo y capturalas."}
          </p>
        </div>
        <div className="actions">
          <button className="btn primary" disabled={busy} onClick={capture}>
            {busy ? "Capturando…" : layout ? "Volver a capturar" : "Capturar layout actual"}
          </button>
          {layout && <button className="btn ghost" onClick={() => onLayout(null)}>Quitar</button>}
          <button className="btn ghost" onClick={undo} title="También disponible en el ícono de la bandeja">↶ Deshacer último cambio</button>
        </div>
      </div>
      {undoMsg && <p className="toast">{undoMsg}</p>}
    </section>
  );
}
