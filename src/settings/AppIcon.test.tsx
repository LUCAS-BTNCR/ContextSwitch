import { render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

const appIcons = vi.hoisted(() => vi.fn());
vi.mock("../api", () => ({ api: { appIcons } }));

import { AppIcon } from "./AppIcon";

// La caché de AppIcon es del módulo: cada test usa rutas propias para no compartir estado.
describe("<AppIcon/>", () => {
  it("varios íconos a la vez se piden en UN solo IPC", async () => {
    appIcons.mockResolvedValue({ "C:\\a1.exe": "data:image/png;base64,AAA", "C:\\a2.exe": "data:image/png;base64,BBB" });
    render(<>
      <AppIcon exePath="C:\a1.exe" name="a1.exe" />
      <AppIcon exePath="C:\a2.exe" name="a2.exe" />
      <AppIcon exePath="C:\a3.exe" name="a3.exe" />
    </>);
    await waitFor(() => expect(document.querySelectorAll("img.app-icon")).toHaveLength(2));
    expect(appIcons).toHaveBeenCalledTimes(1);
    expect(appIcons.mock.calls[0][0]).toEqual(["C:\\a1.exe", "C:\\a2.exe", "C:\\a3.exe"]);
    expect(screen.getByText("A")).toBeTruthy(); // a3 sin ícono → inicial
  });

  it("si el backend falla, muestra la inicial (nunca rompe la UI)", async () => {
    appIcons.mockRejectedValue(new Error("IPC caído"));
    render(<AppIcon exePath="C:\b1.exe" name="discord.exe" />);
    expect(await screen.findByText("D")).toBeTruthy();
  });

  it("lo ya pedido se sirve de la caché sin volver a llamar al backend", async () => {
    appIcons.mockResolvedValue({ "C:\\c1.exe": "data:image/png;base64,CCC" });
    const first = render(<AppIcon exePath="C:\c1.exe" name="c1.exe" />);
    await waitFor(() => expect(first.container.querySelector("img")).toBeTruthy());
    const calls = appIcons.mock.calls.length;
    const second = render(<AppIcon exePath="C:\c1.exe" name="c1.exe" />);
    expect(second.container.querySelector("img")).toBeTruthy();
    expect(appIcons.mock.calls.length).toBe(calls);
  });
});
