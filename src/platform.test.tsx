import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { AppRule } from "./types";

const mock = vi.hoisted(() => ({ api: {} as Record<string, ReturnType<typeof import("vitest").vi.fn>> }));
vi.mock("./api", async () => {
  const { makeApiMock } = await import("./test/apiMock");
  mock.api = makeApiMock().api as never;
  return { api: new Proxy({}, { get: (_t, k) => (mock.api as never)[k] }) };
});

import { chordWarnings, MOD_LABEL } from "./keymap";
import { defaultModifiers, initPlatform, isMac, platform } from "./platform";
import { AppRulesEditor, guessKind, previewItems } from "./settings/AppRulesEditor";
import { normalizeProcessList } from "./settings/SettingsTab";

const WIN_LABELS = { ctrl: "Ctrl", shift: "Shift", alt: "Alt", win: "Win" };
afterEach(() => { platform.name = "windows"; Object.assign(MOD_LABEL, WIN_LABELS); });

const chrome = (extra: Partial<AppRule> = {}): AppRule => ({
  id: "c", label: "Google Chrome", kind: "browser", exePath: "/Applications/Google Chrome.app", processName: "Google Chrome",
  open: ["https://mail.google.com/", "https://calendar.google.com/"], onEnter: "launch-or-focus", onExit: "minimize", ...extra,
});

describe("plataforma", () => {
  it("por defecto es Windows y no toca nada", () => {
    expect(isMac()).toBe(false);
    expect(defaultModifiers()).toEqual(["ctrl", "shift"]);
    expect(MOD_LABEL.win).toBe("Win");
  });

  it("en Mac usa ⌃⌥ por defecto y los símbolos de las teclas", async () => {
    await initPlatform(async () => "macos");
    expect(isMac()).toBe(true);
    expect(defaultModifiers()).toEqual(["ctrl", "alt"]);
    expect(MOD_LABEL).toEqual({ ctrl: "⌃", shift: "⇧", alt: "⌥", win: "⌘" });
  });

  it("si el backend no responde, queda en Windows", async () => {
    await initPlatform(() => Promise.reject(new Error("sin backend")));
    expect(isMac()).toBe(false);
  });
});

describe("Mac", () => {
  it("reconoce navegadores sin workspaces por su nombre de app", () => {
    for (const n of ["Google Chrome", "Vivaldi", "Brave Browser", "Microsoft Edge", "Firefox", "Arc"]) expect(guessKind(n)).toBe("browser");
    expect(guessKind("Zen")).toBe("browser-zen");
    expect(guessKind("Slack")).toBe("chat");
    expect(guessKind("Spark")).toBe("mail");
    expect(guessKind("Visual Studio Code")).toBe("generic");
  });

  it("las listas de apps quedan con el nombre de la app, sin .exe", () => {
    expect(normalizeProcessList("Terminal, Visual Studio Code.app ,spotify", true)).toEqual(["Terminal", "Visual Studio Code", "spotify"]);
    expect(normalizeProcessList("Terminal, Code", false)).toEqual(["terminal.exe", "code.exe"]);
  });

  it("avisa de ⌥ solo (escribe caracteres) y de ⌘ (atajos del sistema); ⌃⌥ no molesta", () => {
    expect(chordWarnings(["ctrl", "alt"], true)).toEqual([]);
    expect(chordWarnings(["alt"], true)[0]).toMatch(/caracteres especiales/);
    expect(chordWarnings(["win", "shift"], true)[0]).toMatch(/macOS/);
    expect(chordWarnings(["ctrl", "alt"], false)[0]).toMatch(/AltGr/); // en Windows sigue el aviso de AltGr
  });

  it("la vista previa muestra cada ventana de un navegador con una ventana por URL", () => {
    const P = { monitor: 0, x: 0, y: 0, w: 0.5, h: 1 };
    expect(previewItems([chrome({ separateWindows: true, placements: [P, null] })]).map((i) => i.label)).toEqual(["mail.google.com"]);
  });

  it("un navegador ofrece ventanas por modo y opciones de salida de ventanas", async () => {
    await initPlatform(async () => "macos");
    render(<AppRulesEditor rules={[chrome()]} onChange={() => {}} />);
    expect(screen.getByText(/sus propias ventanas/)).toBeTruthy();
    expect(screen.getByText("Abrir cada URL en su propia ventana")).toBeTruthy();
    expect(screen.getByRole("radio", { name: "Minimizar sus ventanas" })).toBeTruthy();
    expect(screen.queryByText("Workspace / Perfil (opcional)")).toBeNull();
    expect(screen.getByText("Ruta de la app")).toBeTruthy();
  });

  it("al marcar 'una ventana por URL' en Chrome se guarda en la regla", async () => {
    await initPlatform(async () => "macos");
    const onChange = vi.fn();
    render(<AppRulesEditor rules={[chrome()]} onChange={onChange} />);
    fireEvent.click(screen.getByRole("checkbox", { name: /propia ventana/ }));
    expect(onChange.mock.calls[0][0][0].separateWindows).toBe(true);
  });

  it("en Windows un navegador común no muestra opciones de ventanas por modo", () => {
    render(<AppRulesEditor rules={[chrome({ kind: "browser" })]} onChange={() => {}} />);
    expect(screen.queryByText("Abrir cada URL en su propia ventana")).toBeNull();
  });
});
