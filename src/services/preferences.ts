import type { Preferences } from "../contracts/types";
import { invokeCommand } from "../api/tauri";

export const PreferencesService = {
  get(): Promise<Preferences> {
    return invokeCommand<Preferences>("get_preferences");
  },
  set(key: keyof Preferences, value: boolean): Promise<Preferences> {
    return invokeCommand<Preferences>("set_preference", { key, value });
  },
  reset(): Promise<Preferences> {
    return invokeCommand<Preferences>("reset_preferences");
  },
};
