import type { MonitorInfo, Placement } from "../types";

// Zonas predefinidas, en fracciones del área de trabajo del monitor.
export const ZONES: { id: string; label: string; x: number; y: number; w: number; h: number }[] = [
  { id: "full", label: "Pantalla completa", x: 0, y: 0, w: 1, h: 1 },
  { id: "l50", label: "Mitad izquierda", x: 0, y: 0, w: 0.5, h: 1 },
  { id: "r50", label: "Mitad derecha", x: 0.5, y: 0, w: 0.5, h: 1 },
  { id: "l33", label: "Tercio izquierdo", x: 0, y: 0, w: 1 / 3, h: 1 },
  { id: "c33", label: "Tercio central", x: 1 / 3, y: 0, w: 1 / 3, h: 1 },
  { id: "r33", label: "Tercio derecho", x: 2 / 3, y: 0, w: 1 / 3, h: 1 },
  { id: "l25", label: "Izquierda · 25 %", x: 0, y: 0, w: 0.25, h: 1 },
  { id: "c50", label: "Centro · 50 %", x: 0.25, y: 0, w: 0.5, h: 1 },
  { id: "r25", label: "Derecha · 25 %", x: 0.75, y: 0, w: 0.25, h: 1 },
  { id: "t50", label: "Mitad superior", x: 0, y: 0, w: 1, h: 0.5 },
  { id: "b50", label: "Mitad inferior", x: 0, y: 0.5, w: 1, h: 0.5 },
];

const near = (a: number, b: number) => Math.abs(a - b) < 0.005;
export const zoneOf = (p: Placement) => ZONES.find((z) => near(z.x, p.x) && near(z.y, p.y) && near(z.w, p.w) && near(z.h, p.h));

export const monitorLabel = (m: MonitorInfo) =>
  `Monitor ${m.index + 1} · ${m.width}×${m.height}${m.primary ? " (principal)" : ""}`;

interface PickerProps {
  value: Placement | null | undefined;
  monitors: MonitorInfo[];
  onChange: (p: Placement | null) => void;
}

/** Monitor + zona. "Sin posición" = la ventana queda donde la deje la app. */
export function PlacementPicker({ value, monitors, onChange }: PickerProps) {
  const zone = value ? zoneOf(value) : undefined;
  const defaultMonitor = monitors.find((m) => m.primary)?.index ?? 0;

  const setZone = (id: string) => {
    if (id === "") return onChange(null);
    const z = ZONES.find((z) => z.id === id);
    if (z) onChange({ monitor: value?.monitor ?? defaultMonitor, x: z.x, y: z.y, w: z.w, h: z.h });
  };

  return (
    <div className="placement-picker">
      <select value={zone?.id ?? (value ? "custom" : "")} onChange={(e) => setZone(e.target.value)} aria-label="Zona">
        <option value="">Sin posición fija</option>
        {ZONES.map((z) => <option key={z.id} value={z.id}>{z.label}</option>)}
        {value && !zone && <option value="custom" disabled>Personalizada</option>}
      </select>
      {value && (
        <select value={value.monitor} onChange={(e) => onChange({ ...value, monitor: Number(e.target.value) })} aria-label="Monitor">
          {monitors.map((m) => <option key={m.index} value={m.index}>{monitorLabel(m)}</option>)}
          {!monitors.some((m) => m.index === value.monitor) && (
            <option value={value.monitor}>Monitor {value.monitor + 1} (no conectado)</option>
          )}
        </select>
      )}
    </div>
  );
}

export interface PreviewItem { label: string; place: Placement }

/** Dibujo de los monitores con las zonas asignadas: "así van a quedar las ventanas". */
export function LayoutPreview({ monitors, items }: { monitors: MonitorInfo[]; items: PreviewItem[] }) {
  if (monitors.length === 0 || items.length === 0) return null;
  const minX = Math.min(...monitors.map((m) => m.left));
  const minY = Math.min(...monitors.map((m) => m.top));
  const maxX = Math.max(...monitors.map((m) => m.left + m.width));
  const maxY = Math.max(...monitors.map((m) => m.top + m.height));
  const W = maxX - minX, H = maxY - minY;
  const gap = W * 0.004;

  return (
    <figure className="layout-preview">
      <svg viewBox={`0 0 ${W} ${H}`} role="img" aria-label="Vista previa de posiciones">
        {monitors.map((m) => (
          <g key={m.index}>
            <rect className="mon" x={m.left - minX + gap} y={m.top - minY + gap}
                  width={m.width - 2 * gap} height={m.height - 2 * gap} rx={W * 0.006} />
            <text className="mon-label" x={m.left - minX + m.width / 2} y={m.top - minY + m.height - H * 0.04}
                  fontSize={H * 0.055} textAnchor="middle">{m.index + 1}</text>
          </g>
        ))}
        {items.map((it, i) => {
          const m = monitors[it.place.monitor] ?? monitors.find((x) => x.primary) ?? monitors[0];
          const x = m.workLeft - minX + it.place.x * m.workWidth + gap * 3;
          const y = m.workTop - minY + it.place.y * m.workHeight + gap * 3;
          const w = it.place.w * m.workWidth - gap * 6;
          const h = it.place.h * m.workHeight - gap * 6;
          return (
            <g key={i}>
              <rect className={`zone z${i % 6}`} x={x} y={y} width={Math.max(w, 1)} height={Math.max(h, 1)} rx={W * 0.004} />
              <text className="zone-label" x={x + w / 2} y={y + h / 2} fontSize={Math.min(H * 0.05, w * 0.12)}
                    textAnchor="middle" dominantBaseline="middle">{it.label}</text>
            </g>
          );
        })}
      </svg>
      <figcaption className="muted">Así se acomodan las ventanas al activar el modo.</figcaption>
    </figure>
  );
}
