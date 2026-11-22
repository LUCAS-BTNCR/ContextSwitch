import { vi } from "vitest";
import type { Config } from "../types";

/** Config mínima válida para montar la UI. */
export function sampleConfig(): Config {
  return {
    version: 1,
    settings: { doubleTapWindowMs: 450, swallowTrigger: true, startWithWindows: false, showToastOnSwitch: true, activeProfileId: null },
    profiles: [
      { id: "trabajo", name: "Trabajo", icon: "💼", enabled: true,
        trigger: { modifiers: ["ctrl", "shift"], key: "T", vk: 0x54 }, apps: [], closeOnEnter: [], layout: null },
      { id: "gaming", name: "Gaming", icon: "🎮", enabled: true,
        trigger: { modifiers: ["ctrl", "shift"], key: "G", vk: 0x47 }, apps: [], closeOnEnter: [], layout: null },
    ],
  };
}

/** Reemplazo de `api` (src/api.ts) con respuestas inofensivas por defecto. */
export function makeApiMock() {
  const listeners: { activated?: (e: [string, string[]]) => void } = {};
  const api = {
    getConfig: vi.fn(async () => sampleConfig()),
    getStartupWarnings: vi.fn(async () => [] as string[]),
    saveConfig: vi.fn(async (_config: Config) => {}),
    listAudioDevices: vi.fn(async () => []),
    listRunningApps: vi.fn(async () => []),
    appIcons: vi.fn(async () => ({}) as Record<string, string>),
    listMonitors: vi.fn(async () => [
      { index: 0, primary: true, left: 0, top: 0, width: 2560, height: 1440, workLeft: 0, workTop: 0, workWidth: 2560, workHeight: 1392 },
      { index: 1, primary: false, left: 2560, top: 235, width: 1920, height: 1080, workLeft: 2560, workTop: 235, workWidth: 1920, workHeight: 1032 },
    ]),
    zenBridgeConnected: vi.fn(async () => false),
    captureLayout: vi.fn(async () => ({ capturedAt: "t", windows: [] })),
    undoLayout: vi.fn(async () => false),
    setTestMode: vi.fn(async () => {}),
    activateProfile: vi.fn(async () => {}),
    onChord: vi.fn(async () => () => {}),
    onActivated: vi.fn(async (cb: (e: [string, string[]]) => void) => { listeners.activated = cb; return () => {}; }),
  };
  return { api, listeners };
}
