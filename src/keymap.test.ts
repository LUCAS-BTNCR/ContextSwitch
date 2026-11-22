import { describe, expect, it } from "vitest";
import { chordWarnings, codeToVk, modsFromEvent, MODIFIER_CODES } from "./keymap";

describe("codeToVk (KeyboardEvent.code → Virtual-Key de Windows)", () => {
  it("letras y dígitos usan su código ASCII", () => {
    expect(codeToVk("KeyA")).toBe(0x41);
    expect(codeToVk("KeyZ")).toBe(0x5a);
    expect(codeToVk("Digit0")).toBe(0x30);
    expect(codeToVk("Digit9")).toBe(0x39);
  });

  it("teclado numérico y F1–F24", () => {
    expect(codeToVk("Numpad0")).toBe(0x60);
    expect(codeToVk("Numpad9")).toBe(0x69);
    expect(codeToVk("F1")).toBe(0x70);
    expect(codeToVk("F12")).toBe(0x7b);
    expect(codeToVk("F24")).toBe(0x87);
  });

  it("teclas OEM (posicionales, independientes del layout)", () => {
    expect(codeToVk("Semicolon")).toBe(0xba);
    expect(codeToVk("Backquote")).toBe(0xc0);
    expect(codeToVk("IntlBackslash")).toBe(0xe2);
    expect(codeToVk("Space")).toBe(0x20);
  });

  it("códigos desconocidos o fuera de rango devuelven null (no se puede grabar ese atajo)", () => {
    for (const c of ["", "KeyAA", "Keya", "F0", "F25", "Digit10", "NumpadAdd", "ControlLeft", "Fn"]) {
      expect(codeToVk(c), c).toBeNull();
    }
  });
});

describe("modsFromEvent", () => {
  it("traduce los flags del evento en orden fijo", () => {
    expect(modsFromEvent({ ctrlKey: true, shiftKey: true, altKey: false, metaKey: true } as KeyboardEvent))
      .toEqual(["ctrl", "shift", "win"]);
    expect(modsFromEvent({ ctrlKey: false, shiftKey: false, altKey: false, metaKey: false } as KeyboardEvent)).toEqual([]);
  });

  it("los modificadores solos no son la tecla del atajo", () => {
    for (const c of ["ControlLeft", "ShiftRight", "AltLeft", "MetaRight"]) expect(MODIFIER_CODES.has(c)).toBe(true);
    expect(MODIFIER_CODES.has("KeyG")).toBe(false);
  });
});

describe("chordWarnings", () => {
  it("Ctrl+Alt avisa por AltGr", () => {
    expect(chordWarnings(["ctrl", "alt"]).join()).toMatch(/AltGr/);
    expect(chordWarnings(["ctrl", "alt", "shift"]).join()).not.toMatch(/AltGr/);
  });
  it("Win avisa que Windows reserva combinaciones", () => {
    expect(chordWarnings(["win", "shift"]).join()).toMatch(/Windows reserva/);
  });
  it("solo Shift avisa que interfiere con la escritura", () => {
    expect(chordWarnings(["shift"]).join()).toMatch(/Solo Shift/);
  });
  it("Ctrl+Shift (el default) no tiene avisos", () => {
    expect(chordWarnings(["ctrl", "shift"])).toEqual([]);
  });
});
