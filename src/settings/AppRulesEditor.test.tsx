import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AppRule } from "../types";

const mock = vi.hoisted(() => ({ api: {} as Record<string, ReturnType<typeof import("vitest").vi.fn>> }));
vi.mock("../api", async () => {
  const { makeApiMock } = await import("../test/apiMock");
  mock.api = makeApiMock().api as never;
  return { api: new Proxy({}, { get: (_t, k) => (mock.api as never)[k] }) };
});

import { AppRulesEditor, guessKind, hostOf, launcherFor, mergeZenRules, previewItems, setAt } from "./AppRulesEditor";

const zen = (id: string, open: string[], extra: Partial<AppRule> = {}): AppRule => ({
  id, label: "zen", kind: "browser-zen", exePath: "C:\\Zen\\zen.exe", processName: "zen.exe",
  open, onEnter: "launch-or-focus", onExit: "close", ...extra,
});
const app = (id: string, proc: string, extra: Partial<AppRule> = {}): AppRule => ({
  id, label: proc.replace(".exe", ""), kind: "generic", exePath: `C:\\x\\${proc}`, processName: proc,
  open: [], onEnter: "launch-or-focus", onExit: "keep", ...extra,
});
const P = (monitor: number, x: number, w: number) => ({ monitor, x, y: 0, w, h: 1 });

describe("helpers puros", () => {
  it("setAt rellena con null hasta el índice y no muta el original", () => {
    const orig = [P(0, 0, 1)];
    expect(setAt(orig, 2, P(1, 0, 0.5))).toEqual([P(0, 0, 1), null, P(1, 0, 0.5)]);
    expect(orig).toHaveLength(1);
    expect(setAt(undefined, 0, null)).toEqual([null]);
  });

  it("hostOf quita www y tolera texto que no es URL", () => {
    expect(hostOf("https://www.youtube.com/watch")).toBe("youtube.com");
    expect(hostOf("no soy url")).toBe("no soy url");
  });

  it("guessKind reconoce Zen, correo y chat", () => {
    expect(guessKind("Zen.exe")).toBe("browser-zen");
    expect(guessKind("spark.exe")).toBe("mail");
    expect(guessKind("Discord.exe")).toBe("chat");
    expect(guessKind("code.exe")).toBe("generic");
  });

  it("launcherFor: steamwebhelper se lanza con steam.exe; el resto, su propio exe", () => {
    expect(launcherFor({ processName: "steamwebhelper.exe", exePath: "C:\\Program Files (x86)\\Steam\\bin\\cef\\cef.win64\\steamwebhelper.exe" }))
      .toBe("C:\\Program Files (x86)\\Steam\\steam.exe");
    expect(launcherFor({ processName: "steamwebhelper.exe", exePath: "D:\\raro\\steamwebhelper.exe" }))
      .toBe("D:\\raro\\steamwebhelper.exe");
    expect(launcherFor({ processName: "code.exe", exePath: "C:\\x\\Code.exe" })).toBe("C:\\x\\Code.exe");
  });
});

describe("mergeZenRules", () => {
  it("une las reglas de Zen en la primera, con una ventana por URL y conservando posiciones", () => {
    const rules = [
      zen("z1", ["https://www.youtube.com/"], { placements: [P(1, 0.25, 0.5)] }),
      app("d", "discord.exe"),
      zen("z2", ["https://music.youtube.com/"], { separateWindows: true, placements: [P(1, 0.75, 0.25)] }),
    ];
    const out = mergeZenRules(rules);
    expect(out.map((r) => r.id)).toEqual(["z1", "d"]);
    expect(out[0].separateWindows).toBe(true);
    expect(out[0].open).toEqual(["https://www.youtube.com/", "https://music.youtube.com/"]);
    expect(out[0].placements).toEqual([P(1, 0.25, 0.5), P(1, 0.75, 0.25)]);
  });

  it("modo pestañas con varias URLs: la posición de la ventana pasa a la primera URL", () => {
    const out = mergeZenRules([zen("a", ["https://a.com", "https://b.com"], { placements: [P(0, 0, 1)] }), zen("b", ["https://c.com"])]);
    expect(out[0].placements).toEqual([P(0, 0, 1), null, null]);
  });

  it("con una sola regla de Zen no cambia nada", () => {
    const rules = [zen("a", ["https://a.com"]), app("d", "discord.exe")];
    expect(mergeZenRules(rules)).toBe(rules);
  });
});

describe("previewItems", () => {
  it("una entrada por ventana con posición fija; sin posición no se dibuja", () => {
    const items = previewItems([
      zen("z", ["https://www.youtube.com/", "https://music.youtube.com/"], { separateWindows: true, placements: [P(1, 0.25, 0.5), null] }),
      app("d", "discord.exe", { placements: [P(1, 0, 0.25)] }),
      app("s", "steam.exe"),
    ]);
    expect(items.map((i) => i.label)).toEqual(["youtube.com", "discord"]);
  });
});

describe("<AppRulesEditor/>", () => {
  beforeEach(() => vi.clearAllMocks());

  it("en Zen, el interruptor de ventanas separadas cambia la regla", () => {
    const onChange = vi.fn();
    render(<AppRulesEditor rules={[zen("z", ["https://a.com/"])]} onChange={onChange} />);
    fireEvent.click(screen.getByLabelText(/Abrir cada URL en su propia ventana/));
    expect(onChange).toHaveBeenCalledWith([expect.objectContaining({ id: "z", separateWindows: true })]);
  });

  it("'+ Agregar ventana' agrega un campo VACÍO (no 'https://', que rompía la activación)", () => {
    const onChange = vi.fn();
    render(<AppRulesEditor rules={[zen("z", ["https://a.com/"], { separateWindows: true })]} onChange={onChange} />);
    fireEvent.click(screen.getByText("+ Agregar ventana"));
    expect(onChange.mock.calls[0][0][0].open).toEqual(["https://a.com/", ""]);
  });

  it("quitar una ventana también quita su posición (no se desalinean)", () => {
    const onChange = vi.fn();
    render(<AppRulesEditor
      rules={[zen("z", ["https://a.com/", "https://b.com/"], { separateWindows: true, placements: [P(0, 0, 0.5), P(0, 0.5, 0.5)] })]}
      onChange={onChange} />);
    fireEvent.click(screen.getAllByLabelText("Quitar ventana")[0]);
    const r = onChange.mock.calls[0][0][0] as AppRule;
    expect(r.open).toEqual(["https://b.com/"]);
    expect(r.placements).toEqual([P(0, 0.5, 0.5)]);
  });

  it("con dos reglas de Zen ofrece unirlas", () => {
    const onChange = vi.fn();
    render(<AppRulesEditor rules={[zen("a", ["https://a.com"]), zen("b", ["https://b.com"])]} onChange={onChange} />);
    fireEvent.click(screen.getByText(/Unir en una regla/));
    expect(onChange.mock.calls[0][0]).toHaveLength(1);
  });

  it("muestra si la extensión de Zen está conectada o no", async () => {
    mock.api.zenBridgeConnected.mockResolvedValue(false);
    render(<AppRulesEditor rules={[zen("z", [], { workspace: "Work" })]} onChange={() => {}} />);
    expect(await screen.findByText(/no conectada/)).toBeTruthy();
  });

  it("'Cerrar' en Zen explica que oculta en vez de cerrar (protege los Essentials)", () => {
    render(<AppRulesEditor rules={[zen("z", [], { onExit: "close" })]} onChange={() => {}} />);
    expect(screen.getByText(/oculta/)).toBeTruthy();
  });

  it("el selector de apps se abre y agrega Steam con steam.exe como lanzador", async () => {
    mock.api.listRunningApps.mockResolvedValue([
      { processName: "steamwebhelper.exe", exePath: "C:\\Program Files (x86)\\Steam\\bin\\cef\\cef.win64\\steamwebhelper.exe" },
    ]);
    const onChange = vi.fn();
    render(<AppRulesEditor rules={[]} onChange={onChange} />);
    fireEvent.click(screen.getByText("+ Agregar app"));
    fireEvent.click(await screen.findByText("steamwebhelper.exe"));
    await waitFor(() => expect(onChange).toHaveBeenCalled());
    const added = onChange.mock.calls[0][0][0] as AppRule;
    expect(added.label).toBe("Steam");
    expect(added.exePath).toBe("C:\\Program Files (x86)\\Steam\\steam.exe");
    expect(added.processName).toBe("steamwebhelper.exe");
  });
});
