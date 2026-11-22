import type { Profile } from "./types";

/** Clave del atajo: mismos modificadores (en cualquier orden) + misma tecla. */
export const chordKey = (p: Profile) => [...p.trigger.modifiers].sort().join("+") + "|" + p.trigger.vk;

/**
 * Modos cuyo atajo ya lo usa otro modo anterior: id → nombre del que lo tiene primero.
 * Solo cuentan los modos ACTIVOS: el backend ignora los desactivados (hotkey::bindings_from),
 * así que un desactivado con el mismo atajo no es un conflicto real.
 */
export function findConflicts(profiles: Profile[]): Map<string, string> {
  const seen = new Map<string, string>();
  const out = new Map<string, string>();
  for (const p of profiles) {
    if (!p.enabled || p.trigger.modifiers.length === 0) continue;
    const k = chordKey(p);
    const first = seen.get(k);
    if (first !== undefined) out.set(p.id, first);
    else seen.set(k, p.name);
  }
  return out;
}
