import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { HoverInfo } from "./HoverInfo";

describe("<HoverInfo/>", () => {
  beforeEach(() => { vi.useFakeTimers(); });
  afterEach(() => { vi.useRealTimers(); });

  const setup = () => render(<HoverInfo text="Qué hace la opción"><span>Contexto al Iniciar</span></HoverInfo>);

  it("no muestra la descripción fija: solo tras 400 ms con el mouse encima", () => {
    setup();
    expect(screen.queryByRole("tooltip")).toBeNull();
    fireEvent.mouseEnter(screen.getByText("Contexto al Iniciar").parentElement!);
    act(() => { vi.advanceTimersByTime(399); });
    expect(screen.queryByRole("tooltip")).toBeNull();
    act(() => { vi.advanceTimersByTime(1); });
    expect(screen.getByRole("tooltip").textContent).toBe("Qué hace la opción");
  });

  it("si el mouse sale antes de los 400 ms, no aparece; al salir se oculta", () => {
    setup();
    const box = screen.getByText("Contexto al Iniciar").parentElement!;
    fireEvent.mouseEnter(box);
    act(() => { vi.advanceTimersByTime(200); });
    fireEvent.mouseLeave(box);
    act(() => { vi.advanceTimersByTime(500); });
    expect(screen.queryByRole("tooltip")).toBeNull();

    fireEvent.mouseEnter(box);
    act(() => { vi.advanceTimersByTime(400); });
    expect(screen.getByRole("tooltip")).toBeTruthy();
    fireEvent.mouseLeave(box);
    expect(screen.queryByRole("tooltip")).toBeNull();
  });

  it("el ⓘ con el teclado (foco) la muestra al instante", () => {
    setup();
    fireEvent.focus(screen.getByRole("img", { name: "Qué hace la opción" }));
    expect(screen.getByRole("tooltip")).toBeTruthy();
  });
});
