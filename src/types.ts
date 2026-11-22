// Espejo TS de profiles.rs / profiles.schema.json
export type Modifier = "ctrl" | "shift" | "alt" | "win";
export type AppKind = "generic" | "browser-zen" | "mail" | "chat";
export type EnterAction = "launch-or-focus" | "launch" | "none";
export type ExitAction = "keep" | "minimize" | "close";

export interface Trigger { modifiers: Modifier[]; key: string; vk: number }

export interface AudioTarget {
  outputDeviceId: string;
  outputDeviceName: string;
  setCommunicationsToo: boolean;
}

export interface AppRule {
  id: string;
  label: string;
  kind: AppKind;
  exePath: string;
  processName: string;
  args?: string[];
  workspace?: string | null;
  container?: string | null;
  open: string[];
  onEnter: EnterAction;
  onExit: ExitAction;
  unloadInactiveTabs?: boolean;
  /** Zen: cada URL de `open` en su propia ventana. */
  separateWindows?: boolean;
  /** Con separateWindows, placements[i] es la ventana de open[i]; si no, [0] es la ventana principal. */
  placements?: (Placement | null)[];
}

/** Zona de un monitor en fracciones (0–1) de su área de trabajo. Monitores de izquierda a derecha. */
export interface Placement { monitor: number; x: number; y: number; w: number; h: number }

export interface MonitorInfo {
  index: number; primary: boolean;
  left: number; top: number; width: number; height: number;
  workLeft: number; workTop: number; workWidth: number; workHeight: number;
}

export interface Rect { left: number; top: number; right: number; bottom: number }
export interface WindowPlacementRec {
  processName: string;
  titleContains?: string | null;
  ordinal: number;
  show: "normal" | "maximized" | "minimized";
  rect: Rect;
}
export interface LayoutSnapshot { capturedAt: string; windows: WindowPlacementRec[] }

export interface Profile {
  id: string;
  name: string;
  icon?: string | null;
  enabled: boolean;
  trigger: Trigger;
  audio?: AudioTarget | null;
  apps: AppRule[];
  closeOnEnter: string[];
  /** Al entrar, cierra (WM_CLOSE) toda app con ventana que no esté en `apps` ni en `keepOpen`. */
  closeOthers?: boolean;
  keepOpen?: string[];
  /** "Recordar cómo lo dejé": al volver se reabren las apps y se restaura el layout de la última vez. */
  restoreSession?: boolean;
  layout?: LayoutSnapshot | null;
}

export interface Settings {
  doubleTapWindowMs: number;
  swallowTrigger: boolean;
  startWithWindows: boolean;
  /** Al iniciar con Windows: preguntar qué contexto aplicar (true) o arrancar oculta (false). */
  showStartupPicker?: boolean;
  showToastOnSwitch: boolean;
  activeProfileId?: string | null;
}

export interface Config { $schema?: string; version: 1; settings: Settings; profiles: Profile[] }

export interface AudioDevice { id: string; name: string; isDefault: boolean }
export interface RunningApp { processName: string; exePath: string }
export interface ChordEvent { profileId: string; profileName: string; deltaMs: number; test: boolean }

export interface UpdateInfo {
  current: string;
  latest: string;
  available: boolean;
  installerUrl?: string | null;
  notes: string;
}
