import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const m = vi.hoisted(() => ({ appVersion: vi.fn(), checkUpdate: vi.fn(), installUpdate: vi.fn() }));
vi.mock("../api", () => ({ api: m }));

import { UpdateButton } from "./UpdateButton";

const info = (available: boolean) => ({
  current: "0.4.0", latest: available ? "0.5.0" : "0.4.0", available, notes: "",
  installerUrl: "https://github.com/LUCAS-BTNCR/ContextSwitch/releases/download/v0.5.0/ContextSwitch_0.5.0_x64-setup.exe",
});

describe("<UpdateButton/>", () => {
  beforeEach(() => { vi.clearAllMocks(); m.appVersion.mockResolvedValue("0.4.0"); });

  it("no se conecta hasta que se toca el botón", async () => {
    render(<UpdateButton />);
    expect(await screen.findByText("Versión v0.4.0")).toBeTruthy();
    expect(m.checkUpdate).not.toHaveBeenCalled();
  });

  it("al día", async () => {
    m.checkUpdate.mockResolvedValue(info(false));
    render(<UpdateButton />);
    fireEvent.click(screen.getByText("Buscar actualizaciones"));
    expect(await screen.findByText("Estás al día · v0.4.0")).toBeTruthy();
  });

  it("hay versión nueva → Actualizar la instala con la URL del instalador", async () => {
    m.checkUpdate.mockResolvedValue(info(true));
    m.installUpdate.mockReturnValue(new Promise(() => {}));
    render(<UpdateButton />);
    fireEvent.click(screen.getByText("Buscar actualizaciones"));
    fireEvent.click(await screen.findByText("Actualizar a v0.5.0"));
    expect(m.installUpdate).toHaveBeenCalledWith(info(true).installerUrl);
    expect(screen.getByText(/Descargando v0.5.0/)).toBeTruthy();
  });

  it("muestra el error si no hay conexión", async () => {
    m.checkUpdate.mockRejectedValue("no se pudo conectar con GitHub");
    render(<UpdateButton />);
    fireEvent.click(screen.getByText("Buscar actualizaciones"));
    expect(await screen.findByText("no se pudo conectar con GitHub")).toBeTruthy();
  });
});
