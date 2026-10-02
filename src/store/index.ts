import type { AppSettings, AppState, Preferences } from "../contracts/types";

export type StoreListener = (state: AppState) => void;

const defaultSettings: AppSettings = {
  themeChoice: "system",
  customTheme: null,
  localeChoice: "auto",
};

const defaultPreferences: Preferences = {
  showLogo: true,
  showTooltip: true,
};

class AppStore {
  private state: AppState = {
    route: "home",
    appInfo: null,
    settings: defaultSettings,
    preferences: defaultPreferences,
    runtimeAvailable: false,
  };
  private listeners = new Set<StoreListener>();

  getState(): AppState {
    return this.state;
  }

  setState(patch: Partial<AppState>): void {
    this.state = { ...this.state, ...patch };
    this.listeners.forEach((listener) => listener(this.state));
  }

  subscribe(listener: StoreListener): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }
}

export const appStore = new AppStore();
