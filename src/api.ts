import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { AudioDevice, ChordEvent, Config, LayoutSnapshot, MonitorInfo, RunningApp } from "./types";

// Único canal de la UI: IPC local con el backend Rust. La CSP bloquea cualquier red.
export const api = {
  getConfig: () => invoke<Config>("get_config"),
  saveConfig: (config: Config) => invoke<void>("save_config", { config }),
  listAudioDevices: () => invoke<AudioDevice[]>("list_audio_devices"),
  listRunningApps: () => invoke<RunningApp[]>("list_running_apps"),
  appIcons: (paths: string[]) => invoke<Record<string, string>>("app_icons", { paths }),
  listMonitors: () => invoke<MonitorInfo[]>("list_monitors"),
  captureLayout: () => invoke<LayoutSnapshot>("capture_layout"),
  undoLayout: () => invoke<boolean>("undo_layout"),
  setTestMode: (on: boolean) => invoke<void>("set_test_mode", { on }),
  activateProfile: (id: string) => invoke<void>("activate_profile", { id }),
  onChord: (cb: (e: ChordEvent) => void): Promise<UnlistenFn> =>
    listen<ChordEvent>("chord-detected", (ev) => cb(ev.payload)),
};
