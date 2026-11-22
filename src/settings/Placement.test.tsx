import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { MonitorInfo } from "../types";
import { LayoutPreview, monitorLabel, PlacementPicker, ZONES, zoneOf } from "./Placement";

const MONS: MonitorInfo[] = [
  { index: 0, primary: false, left: -1920, top: 0, width: 1920, height: 1080, workLeft: -1920, workTop: 0, workWidth: 1920, workHeight: 1040 },
  { index: 1, primary: true, left: 0, top: 0, width: 2560, height: 1440, workLeft: 0, workTop: 0, workWidth: 2560, workHeight: 1392 },
];

describe("zonas", () => {
  it("cada zona predefinida cabe en el monitor (0–1) y se reconoce a sí misma", () => {
    for (const z of ZONES) {
      expect(z.x + z.w).toBeLessThanOrEqual(1 + 1e-9);
      expect(z.y + z.h).toBeLessThanOrEqual(1 + 1e-9);
      expect(zoneOf({ monitor: 0, ...z })?.id).toBe(z.id);
    }
  });

  it("25/50/25 cubre el ancho completo sin huecos", () => {
    const [l, c, r] = ["l25", "c50", "r25"].map((id) => ZONES.find((z) => z.id === id)!);
    expect(l.x + l.w).toBeCloseTo(c.x);
    expect(c.x + c.w).toBeCloseTo(r.x);
    expect(r.x + r.w).toBeCloseTo(1);
  });

  it("una posición que no es predefinida es 'personalizada' (zoneOf = undefined)", () => {
    expect(zoneOf({ monitor: 0, x: 0.1, y: 0, w: 0.3, h: 1 })).toBeUndefined();
  });

  it("monitorLabel numera desde 1 y marca el principal", () => {
    expect(monitorLabel(MONS[1])).toBe("Monitor 2 · 2560×1440 (principal)");
    expect(monitorLabel(MONS[0])).toBe("Monitor 1 · 1920×1080");
  });
});

describe("<PlacementPicker/>", () => {
  it("elegir una zona usa el monitor PRINCIPAL por defecto", () => {
    const onChange = vi.fn();
    render(<PlacementPicker value={null} monitors={MONS} onChange={onChange} />);
    fireEvent.change(screen.getByLabelText("Zona"), { target: { value: "r25" } });
    expect(onChange).toHaveBeenCalledWith({ monitor: 1, x: 0.75, y: 0, w: 0.25, h: 1 });
  });

  it("'Sin posición fija' borra la posición", () => {
    const onChange = vi.fn();
    render(<PlacementPicker value={{ monitor: 0, x: 0, y: 0, w: 1, h: 1 }} monitors={MONS} onChange={onChange} />);
    fireEvent.change(screen.getByLabelText("Zona"), { target: { value: "" } });
    expect(onChange).toHaveBeenCalledWith(null);
  });

  it("cambiar de monitor conserva la zona", () => {
    const onChange = vi.fn();
    render(<PlacementPicker value={{ monitor: 1, x: 0.25, y: 0, w: 0.5, h: 1 }} monitors={MONS} onChange={onChange} />);
    fireEvent.change(screen.getByLabelText("Monitor"), { target: { value: "0" } });
    expect(onChange).toHaveBeenCalledWith({ monitor: 0, x: 0.25, y: 0, w: 0.5, h: 1 });
  });

  it("un monitor guardado que ya no está conectado se muestra como tal", () => {
    render(<PlacementPicker value={{ monitor: 5, x: 0, y: 0, w: 1, h: 1 }} monitors={MONS} onChange={() => {}} />);
    expect(screen.getByText(/Monitor 6 \(no conectado\)/)).toBeTruthy();
  });
});

describe("<LayoutPreview/>", () => {
  it("sin monitores o sin ventanas no dibuja nada", () => {
    const { container: a } = render(<LayoutPreview monitors={[]} items={[{ label: "x", place: { monitor: 0, x: 0, y: 0, w: 1, h: 1 } }]} />);
    const { container: b } = render(<LayoutPreview monitors={MONS} items={[]} />);
    expect(a.innerHTML + b.innerHTML).toBe("");
  });

  it("dibuja cada monitor y cada ventana con su etiqueta (también con monitores en x negativa)", () => {
    const { container } = render(<LayoutPreview monitors={MONS}
      items={[{ label: "YouTube", place: { monitor: 0, x: 0.25, y: 0, w: 0.5, h: 1 } }]} />);
    expect(container.querySelectorAll("rect.mon")).toHaveLength(2);
    const zone = container.querySelector("rect.zone")!;
    expect(Number(zone.getAttribute("x"))).toBeGreaterThan(0); // normalizado al origen del dibujo
    expect(screen.getByText("YouTube")).toBeTruthy();
  });
});
