import type React from "react";
import type { Modifier } from "./types";

// KeyboardEvent.code → Virtual-Key de Windows. `code` es posicional (independiente del
// layout), igual que el VK que reporta WH_KEYBOARD_LL para letras/números.
const OEM: Record<string, number> = {
  Semicolon: 0xba, Equal: 0xbb, Comma: 0xbc, Minus: 0xbd, Period: 0xbe, Slash: 0xbf,
  Backquote: 0xc0, BracketLeft: 0xdb, Backslash: 0xdc, BracketRight: 0xdd, Quote: 0xde,
  IntlBackslash: 0xe2, Space: 0x20, Enter: 0x0d, Tab: 0x09, Backspace: 0x08,
};

export function codeToVk(code: string): number | null {
  if (/^Key[A-Z]$/.test(code)) return code.charCodeAt(3);
  if (/^Digit[0-9]$/.test(code)) return code.charCodeAt(5);
  if (/^Numpad[0-9]$/.test(code)) return 0x60 + Number(code[6]);
  const f = /^F([1-9]|1[0-9]|2[0-4])$/.exec(code);
  if (f) return 0x6f + Number(f[1]);
  return OEM[code] ?? null;
}

export const MODIFIER_CODES = new Set([
  "ControlLeft", "ControlRight", "ShiftLeft", "ShiftRight",
  "AltLeft", "AltRight", "MetaLeft", "MetaRight",
]);

export function modsFromEvent(e: KeyboardEvent | React.KeyboardEvent): Modifier[] {
  const m: Modifier[] = [];
  if (e.ctrlKey) m.push("ctrl");
  if (e.shiftKey) m.push("shift");
  if (e.altKey) m.push("alt");
  if (e.metaKey) m.push("win");
  return m;
}

/** Etiquetas de los modificadores. En Mac, initPlatform las cambia por ⌃ ⇧ ⌥ ⌘. */
export const MOD_LABEL: Record<Modifier, string> = { ctrl: "Ctrl", shift: "Shift", alt: "Alt", win: "Win" };

/** Advertencias de UX para combinaciones problemáticas. `mac`: reglas del teclado de Mac. */
export function chordWarnings(mods: Modifier[], mac = false): string[] {
  const w: string[] = [];
  const s = new Set(mods);
  if (mac) {
    if (s.has("win") && !s.has("ctrl") && !s.has("alt"))
      w.push("Muchos ⌘+tecla y ⌘⇧+tecla ya son atajos de macOS y de las apps; probá con el área de prueba antes de guardar.");
    if (s.has("alt") && !s.has("ctrl") && !s.has("win") && mods.length <= 2)
      w.push("⌥+tecla escribe caracteres especiales (∂, ©, ¡…): dejá activado “Reservar combinación” para que no se escriban.");
    if (mods.length === 1 && s.has("shift"))
      w.push("Solo ⇧ interfiere con la escritura normal (mayúsculas). Sumá otro modificador.");
    return w;
  }
  if (s.has("ctrl") && s.has("alt") && !s.has("shift") && !s.has("win"))
    w.push("Ctrl+Alt equivale a AltGr en teclados en español: puede chocar con @, #, [ ] …");
  if (s.has("win"))
    w.push("Windows reserva muchos Win+tecla; probá con el área de prueba antes de guardar.");
  if (mods.length === 1 && s.has("shift"))
    w.push("Solo Shift interfiere con la escritura normal (mayúsculas). Sumá otro modificador.");
  return w;
}
