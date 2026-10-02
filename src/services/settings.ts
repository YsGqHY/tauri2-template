import type { AppSettings, CustomTheme, LocaleChoice, ThemeChoice } from "../contracts/types";
import { invokeCommand } from "../api/tauri";

export const SettingsService = {
  get(): Promise<AppSettings> {
    return invokeCommand<AppSettings>("get_app_settings");
  },
  setThemeChoice(themeChoice: ThemeChoice): Promise<AppSettings> {
    return invokeCommand<AppSettings>("set_theme_choice", { themeChoice });
  },
  setCustomTheme(customTheme: CustomTheme): Promise<AppSettings> {
    return invokeCommand<AppSettings>("set_custom_theme", { customTheme });
  },
  resetCustomTheme(): Promise<AppSettings> {
    return invokeCommand<AppSettings>("reset_custom_theme");
  },
  setLocaleChoice(localeChoice: LocaleChoice): Promise<AppSettings> {
    return invokeCommand<AppSettings>("set_locale_choice", { localeChoice });
  },
};
