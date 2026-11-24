import { MOD_LABEL } from "./keymap";
import type { Modifier } from "./types";

// Sistema en el que corre la app. Se pregunta una vez al backend antes de dibujar la UI
// (main.tsx); en los tests queda "windows", así se prueban los textos de siempre.
export type Platform = "windows" | "macos";
export const platform: { name: Platform } = { name: "windows" };
export const isMac = () => platform.name === "macos";

/** Modificadores por defecto de un modo nuevo: Ctrl+Shift en Windows, ⌃⌥ en Mac. */
export const defaultModifiers = (): Modifier[] => (isMac() ? ["ctrl", "alt"] : ["ctrl", "shift"]);

export async function initPlatform(get: () => Promise<string>): Promise<void> {
  const name = await get().catch(() => "windows");
  platform.name = name === "macos" ? "macos" : "windows";
  if (isMac()) Object.assign(MOD_LABEL, { ctrl: "⌃", shift: "⇧", alt: "⌥", win: "⌘" });
}
