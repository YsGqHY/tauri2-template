export type ThemeChoice = "system" | "light" | "dark" | "obsidian" | "custom";
export type ThemeMode = "light" | "dark";
export type LocaleChoice = "auto" | "zh-CN" | "en-US";

export interface AppInfo {
  name: string;
  version: string;
  identifier: string;
  platform: string;
  arch: string;
}

export interface AppSettings {
  themeChoice: ThemeChoice;
  customTheme: CustomThemeSnapshot | null;
  localeChoice: LocaleChoice;
}

export interface Preferences {
  showLogo: boolean;
  showTooltip: boolean;
}

export interface CustomTheme {
  name?: string;
  mode?: ThemeMode;
  palette: Partial<Record<ThemeToken, string>>;
}

export interface CustomThemeSnapshot {
  name: string;
  mode: ThemeMode;
  palette: Record<ThemeToken, string>;
}

export type ThemeToken =
  | "bg"
  | "surface"
  | "surfaceRaised"
  | "surfaceMuted"
  | "text"
  | "textMuted"
  | "textSubtle"
  | "border"
  | "accent"
  | "accentStrong"
  | "accentSoft"
  | "success"
  | "warning"
  | "danger"
  | "info"
  | "focus"
  | "sidebar";

export interface StorageStats {
  path: string;
  isCustom: boolean;
  defaultPath: string;
  sizeBytes: number;
}

export interface TableStats {
  name: string;
  labelKey: string;
  clearable: boolean;
  rowCount: number;
  sizeBytes: number;
  estimated: boolean;
}

export interface StorageTableStats {
  totalBytes: number;
  tables: TableStats[];
}

export interface StorageSnapshot {
  storage: StorageStats;
  tableStats: StorageTableStats;
}

export type ChildWindowType = "confirm" | "message" | "blank";
export type ChildEventStatus = "opened" | "result" | "closed" | "message" | string;

export interface ChildWindow {
  id: string;
  label: string;
  kind: ChildWindowType;
  title: string;
}

export interface ChildWindowOptions {
  id: string;
  kind: ChildWindowType;
  title: string;
  message?: string;
  width?: number;
  height?: number;
}

export interface TimePayload {
  formatted: string;
  unixMs: number;
}

export interface ChildEventPayload {
  id: string;
  kind: ChildWindowType;
  status: ChildEventStatus;
  message?: string | null;
  data?: unknown;
  from?: string | null;
}

export interface ChildClosedPayload {
  id: string;
}

export interface ChildMessagePayload {
  from: string;
  data: unknown;
}

export interface Process {
  id: string;
  command: string;
  args: string[];
  cwd?: string | null;
  running: boolean;
  exitCode: number | null;
  success: boolean | null;
}

export interface ArgPattern {
  kind: string;
  value?: string;
}

export interface AvailableCommand {
  id: string;
  executable: string;
  argPatterns: ArgPattern[];
  maxArgs: number;
  cwdRoot?: string | null;
}

export interface SubprocessRequest {
  command: string;
  args: string[];
  cwd?: string;
}

export interface ProcessOutput {
  id: string;
  line: string;
}

export interface ProcessExit {
  id: string;
  code: number | null;
  success: boolean;
}

export interface SubprocessSnapshot {
  info: Process;
  stdout: string[];
  stderr: string[];
  exit: ProcessExit | null;
}


export interface ErrorPayload {
  code?: string;
  message: string;
  detail?: string;
  nextStep?: string;
}

export interface TrayActionPayload {
  action: string;
}

export interface AppState {
  route: string;
  appInfo: AppInfo | null;
  settings: AppSettings;
  preferences: Preferences;
  runtimeAvailable: boolean;
}

export type Cleanup = () => void;
export type Mount = (container: HTMLElement) => Cleanup;
