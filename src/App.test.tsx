import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { makeApiMock, sampleConfig } from "./test/apiMock";

const h = vi.hoisted(() => ({ mock: null as unknown as ReturnType<typeof import("./test/apiMock").makeApiMock> }));
vi.mock("./api", () => ({ api: new Proxy({}, { get: (_t, k) => (h.mock.api as never)[k] }) }));

import App from "./App";

beforeEach(() => { h.mock = makeApiMock(); });
afterEach(() => { vi.useRealTimers(); });

describe("<App/> (flujo de Ajustes con backend simulado)", () => {
  it("carga la config del backend y muestra los modos", async () => {
    render(<App />);
    expect(await screen.findByDisplayValue("Trabajo")).toBeTruthy();
    expect(screen.getAllByText("Gaming").length).toBeGreaterThan(0);
  });

  it("autoguardado: varias ediciones seguidas = UN guardado, 500 ms después de la última", async () => {
    render(<App />);
    const name = await screen.findByLabelText("Nombre del modo");
    vi.useFakeTimers();
    fireEvent.change(name, { target: { value: "Trab" } });
    fireEvent.change(name, { target: { value: "Trabajo 2" } });
    await act(async () => { vi.advanceTimersByTime(499); });
    expect(h.mock.api.saveConfig).not.toHaveBeenCalled();
    await act(async () => { vi.advanceTimersByTime(1); });
    expect(h.mock.api.saveConfig).toHaveBeenCalledTimes(1);
    const saved = (h.mock.api.saveConfig.mock.calls as unknown[][])[0][0] as ReturnType<typeof sampleConfig>;
    expect(saved.profiles[0].name).toBe("Trabajo 2");
  });

  it("si guardar falla, no se cae la UI", async () => {
    h.mock.api.saveConfig.mockRejectedValue(new Error("disco lleno"));
    render(<App />);
    const name = await screen.findByLabelText("Nombre del modo");
    vi.useFakeTimers();
    fireEvent.change(name, { target: { value: "X" } });
    await act(async () => { vi.advanceTimersByTime(600); });
    vi.useRealTimers();
    expect(screen.getByLabelText("Nombre del modo")).toBeTruthy();
  });

  it("muestra los avisos del arranque (ej. config recuperada) y se pueden cerrar", async () => {
    h.mock.api.getStartupWarnings.mockResolvedValue(["profiles.json estaba dañado: se guardó una copia"]);
    render(<App />);
    expect(await screen.findByText(/estaba dañado/)).toBeTruthy();
    fireEvent.click(screen.getByLabelText("Cerrar aviso"));
    expect(screen.queryByText(/estaba dañado/)).toBeNull();
  });

  it("muestra los avisos que llegan al activar un modo", async () => {
    render(<App />);
    await screen.findByDisplayValue("Trabajo");
    await waitFor(() => expect(h.mock.listeners.activated).toBeDefined());
    act(() => h.mock.listeners.activated!(["gaming", ["No se pudo cerrar taskmgr.exe porque se ejecuta como administrador"]]));
    expect(await screen.findByText(/taskmgr\.exe/)).toBeTruthy();
  });

  it("avisa si dos modos activos usan el mismo atajo", async () => {
    const cfg = sampleConfig();
    cfg.profiles[1].trigger = { ...cfg.profiles[0].trigger };
    h.mock.api.getConfig.mockResolvedValue(cfg);
    render(<App />);
    fireEvent.click(await screen.findByText("Gaming"));
    expect(await screen.findByText(/ya la usa “Trabajo”/)).toBeTruthy();
  });

  it("'Activar ahora' le pide al backend activar ESE modo", async () => {
    render(<App />);
    fireEvent.click(await screen.findByText("Activar ahora"));
    expect(h.mock.api.activateProfile).toHaveBeenCalledWith("trabajo");
  });
});
