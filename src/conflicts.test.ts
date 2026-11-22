import { describe, expect, it } from "vitest";
import { chordKey, findConflicts } from "./conflicts";
import type { Modifier, Profile } from "./types";

const p = (id: string, mods: Modifier[], vk: number, enabled = true): Profile => ({
  id, name: id.toUpperCase(), enabled, trigger: { modifiers: mods, key: "?", vk }, apps: [], closeOnEnter: [],
});

describe("chordKey", () => {
  it("el orden de los modificadores no importa", () => {
    expect(chordKey(p("a", ["shift", "ctrl"], 71))).toBe(chordKey(p("b", ["ctrl", "shift"], 71)));
  });
  it("otra tecla u otro modificador es otro atajo", () => {
    expect(chordKey(p("a", ["ctrl"], 71))).not.toBe(chordKey(p("b", ["ctrl"], 72)));
    expect(chordKey(p("a", ["ctrl"], 71))).not.toBe(chordKey(p("b", ["ctrl", "alt"], 71)));
  });
});

describe("findConflicts", () => {
  it("marca al segundo modo con el nombre del primero", () => {
    const c = findConflicts([p("a", ["ctrl", "shift"], 71), p("b", ["shift", "ctrl"], 71)]);
    expect([...c]).toEqual([["b", "A"]]);
  });

  it("tres modos con el mismo atajo: los dos últimos apuntan al primero", () => {
    const c = findConflicts([p("a", ["ctrl"], 1), p("b", ["ctrl"], 1), p("c", ["ctrl"], 1)]);
    expect(c.get("b")).toBe("A");
    expect(c.get("c")).toBe("A");
  });

  it("un modo DESACTIVADO no genera conflicto (el backend lo ignora)", () => {
    expect(findConflicts([p("a", ["ctrl"], 1, false), p("b", ["ctrl"], 1)]).size).toBe(0);
    expect(findConflicts([p("a", ["ctrl"], 1), p("b", ["ctrl"], 1, false)]).size).toBe(0);
  });

  it("sin modificadores no cuenta (no es un atajo válido)", () => {
    expect(findConflicts([p("a", [], 1), p("b", [], 1)]).size).toBe(0);
  });

  it("lista vacía", () => {
    expect(findConflicts([]).size).toBe(0);
  });
});
