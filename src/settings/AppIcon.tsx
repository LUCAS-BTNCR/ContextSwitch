import { useEffect, useState } from "react";
import { api } from "../api";

// Caché de la sesión: exePath → data URL ("" = sin ícono). Los pedidos se agrupan en un
// solo IPC por tick, así abrir el selector con 30 apps hace una llamada, no 30.
const cache = new Map<string, string>();
const waiting = new Map<string, Array<() => void>>();
let pending: string[] = [];

function request(path: string, done: () => void) {
  const list = waiting.get(path);
  if (list) { list.push(done); return; }
  waiting.set(path, [done]);
  pending.push(path);
  if (pending.length === 1) {
    queueMicrotask(() => {
      const batch = pending;
      pending = [];
      api.appIcons(batch)
        .catch(() => ({} as Record<string, string>))
        .then((icons) => {
          for (const p of batch) {
            cache.set(p, icons[p] ?? "");
            waiting.get(p)?.forEach((cb) => cb());
            waiting.delete(p);
          }
        });
    });
  }
}

/** Ícono real del .exe; si no se puede leer, la inicial del nombre. */
export function AppIcon({ exePath, name, size = 24 }: { exePath: string; name: string; size?: number }) {
  const [src, setSrc] = useState(() => cache.get(exePath));
  useEffect(() => {
    if (!exePath) { setSrc(""); return; }
    if (cache.has(exePath)) { setSrc(cache.get(exePath)); return; }
    let alive = true;
    request(exePath, () => alive && setSrc(cache.get(exePath)));
    return () => { alive = false; };
  }, [exePath]);

  const style = { width: size, height: size };
  if (src) return <img className="app-icon" src={src} alt="" style={style} draggable={false} />;
  return (
    <span className="app-icon fallback" style={{ ...style, fontSize: size * 0.5 }} aria-hidden>
      {src === undefined ? "" : name.replace(/\.exe$/i, "").charAt(0).toUpperCase()}
    </span>
  );
}
