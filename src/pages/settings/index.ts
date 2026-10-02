import type { AppSettings, Cleanup, Preferences, StorageStats, StorageTableStats, TableStats, ThemeMode, ThemeToken } from "../../contracts/types";
import type { Translate } from "../../i18n";
import { toDisplayError } from "../../api/tauri";
import { NativeDialogs, PreferencesService, SettingsService, StorageService } from "../../services";
import { applyTheme, isValidHexColor, lightPalette, type ThemePalette, validateCustomTheme } from "../../theme";
import { escapeHtml, formatBytes, on, qs } from "../../shared/dom";

export interface SettingsPageProps {
  t: Translate;
  settings: AppSettings;
  preferences: Preferences;
  onSettingsChange: (settings: AppSettings) => void;
  onPreferencesChange: (preferences: Preferences) => void;
  onLocaleChange: () => void;
  onRefreshState?: () => Promise<void>;
  onToast: (message: string, kind?: "info" | "success" | "error") => void;
}

type SettingsTab = "personalization" | "language" | "database";

const paletteKeys: ThemeToken[] = ["bg", "surface", "surfaceRaised", "surfaceMuted", "text", "textMuted", "textSubtle", "border", "accent", "accentStrong", "accentSoft", "success", "warning", "danger", "info", "focus", "sidebar"];

export const mountSettingsPage = (container: HTMLElement, props: SettingsPageProps): Cleanup => {
  let activeTab = (container.dataset.settingsTab as SettingsTab | undefined) ?? "personalization";
  let settings = props.settings;
  let preferences = props.preferences;
  let storage: StorageStats | null = null;
  let tableStats: StorageTableStats = { totalBytes: 0, tables: [] };
  let disposed = false;
  let customMode: ThemeMode = settings.customTheme?.mode ?? "light";
  let customPalette: ThemePalette = { ...applyTheme(settings.themeChoice, settings.customTheme) };
  const busy = new Set<string>();

  const errorMessage = (error: unknown): string => {
    const payload = toDisplayError(error, props.t);
    return payload.nextStep ? `${payload.message} ${payload.nextStep}` : payload.message;
  };

  const setBusy = (key: string, value: boolean): void => {
    if (value) {
      busy.add(key);
    } else {
      busy.delete(key);
    }
    container.querySelectorAll<HTMLElement>("[data-busy-key]").forEach((element) => {
      const current = element.dataset.busyKey;
      if (current) {
        element.toggleAttribute("disabled", busy.has(current));
        element.classList.toggle("is-busy", busy.has(current));
      }
    });
  };

  const run = <T>(key: string, task: () => Promise<T>, onSuccess: (value: T) => void, onFailure?: () => void): void => {
    if (disposed || busy.has(key)) {
      return;
    }
    setBusy(key, true);
    task()
      .then((value) => {
        if (!disposed) {
          onSuccess(value);
        }
      })
      .catch((error: unknown) => {
        if (!disposed) {
          onFailure?.();
          props.onToast(errorMessage(error), "error");
        }
      })
      .finally(() => {
        if (!disposed) {
          setBusy(key, false);
        }
      });
  };

  const refreshDatabase = (): void => {
    if (disposed || busy.has("refresh")) {
      return;
    }
    setBusy("refresh", true);
    Promise.all([StorageService.getStats(), StorageService.getTableStats()])
      .then(([nextStorage, nextTableStats]) => {
        if (disposed) {
          return;
        }
        storage = nextStorage;
        tableStats = nextTableStats;
        if (activeTab === "database") {
          render();
        }
      })
      .catch((error: unknown) => {
        if (!disposed) {
          props.onToast(errorMessage(error), "error");
        }
      })
      .finally(() => {
        if (!disposed) {
          setBusy("refresh", false);
        }
      });
  };

  const syncGlobalState = (): void => {
    props.onRefreshState?.().catch((error: unknown) => {
      if (!disposed) {
        props.onToast(errorMessage(error), "error");
      }
    });
  };

  const setStoragePathWithConfirmation = async (path: string): Promise<StorageStats | null> => {
    try {
      return await StorageService.setCustomPath(path);
    } catch (error: unknown) {
      const payload = toDisplayError(error, props.t);
      if (payload.code !== "STORAGE_TARGET_EXISTS") {
        throw error;
      }
      const overwrite = await NativeDialogs.confirm({ title: props.t("settings.overwriteStorageTitle"), message: props.t("settings.overwriteStorageMessage"), okLabel: props.t("common.apply"), cancelLabel: props.t("common.cancel") });
      return overwrite ? StorageService.setCustomPath(path, true) : null;
    }
  };

  const resetStoragePathWithConfirmation = async (): Promise<StorageStats | null> => {
    try {
      return await StorageService.resetPath();
    } catch (error: unknown) {
      const payload = toDisplayError(error, props.t);
      if (payload.code !== "STORAGE_TARGET_EXISTS") {
        throw error;
      }
      const overwrite = await NativeDialogs.confirm({ title: props.t("settings.overwriteStorageTitle"), message: props.t("settings.overwriteStorageMessage"), okLabel: props.t("common.apply"), cancelLabel: props.t("common.cancel") });
      return overwrite ? StorageService.resetPath(true) : null;
    }
  };

  const tableLabel = (table: TableStats): string => {
    const translated = props.t(table.labelKey);
    return translated === table.labelKey ? table.name : translated;
  };

  const renderTabs = (): string => `
    <nav class="settings-tabs" aria-label="${escapeHtml(props.t("settings.title"))}">
      <button class="settings-tab ${activeTab === "personalization" ? "is-active" : ""}" data-settings-tab="personalization"><strong>${props.t("settings.personalization")}</strong><span>${props.t("settings.personalizationDescription")}</span></button>
      <button class="settings-tab ${activeTab === "language" ? "is-active" : ""}" data-settings-tab="language"><strong>${props.t("settings.language")}</strong><span>${props.t("settings.languageDescription")}</span></button>
      <button class="settings-tab ${activeTab === "database" ? "is-active" : ""}" data-settings-tab="database"><strong>${props.t("settings.database")}</strong><span>${props.t("settings.databaseDescription")}</span></button>
    </nav>
  `;

  const renderPersonalization = (): string => {
    const choices: Array<[AppSettings["themeChoice"], string]> = [["system", props.t("settings.themeSystem")], ["light", props.t("settings.themeLight")], ["dark", props.t("settings.themeDark")], ["obsidian", props.t("settings.themeObsidian")], ["custom", props.t("settings.themeCustom")]];
    return `
      <section class="settings-section">
        <div class="section-heading"><h2>${props.t("settings.themeTitle")}</h2><p>${props.t("settings.themeDescription")}</p></div>
        <div class="choice-grid" role="radiogroup" aria-label="${escapeHtml(props.t("settings.themeTitle"))}">
          ${choices.map(([value, label]) => `<label class="choice-card ${settings.themeChoice === value ? "is-selected" : ""}"><input type="radio" name="themeChoice" value="${value}" ${settings.themeChoice === value ? "checked" : ""}/><span>${label}</span><small>${value}</small></label>`).join("")}
        </div>
      </section>
      <section class="settings-section">
        <div class="section-heading"><h2>${props.t("settings.customPaletteTitle")}</h2><p>${props.t("settings.customPaletteDescription")}</p></div>
        <label class="field"><span>${props.t("settings.paletteName")}</span><input data-palette-name value="${escapeHtml(settings.customTheme?.name ?? props.t("settings.defaultPaletteName"))}" /></label>
        <div class="choice-stack choice-stack--inline" role="radiogroup" aria-label="${escapeHtml(props.t("settings.customMode"))}">
          <label class="choice-row"><input type="radio" name="customMode" value="light" ${customMode === "light" ? "checked" : ""}/><span>${props.t("settings.customModeLight")}</span></label>
          <label class="choice-row"><input type="radio" name="customMode" value="dark" ${customMode === "dark" ? "checked" : ""}/><span>${props.t("settings.customModeDark")}</span></label>
        </div>
        <div class="palette-grid" data-palette-grid>${paletteKeys.map((token) => `<label class="color-field"><span>${props.t("settings.paletteToken")} · ${escapeHtml(props.t(`settings.palette.${token}`))}</span><input type="text" data-palette-token="${token}" value="${escapeHtml(customPalette[token])}"/><input type="color" aria-label="${escapeHtml(props.t(`settings.palette.${token}`))}" data-palette-color="${token}" value="${isValidHexColor(customPalette[token]) ? customPalette[token] : lightPalette[token]}"/></label>`).join("")}</div>
        <div class="button-row"><button class="button button--primary" data-busy-key="save-palette" data-save-palette>${props.t("common.save")}</button><button class="button" data-busy-key="reset-palette" data-reset-palette>${props.t("common.reset")}</button></div>
        <div class="palette-preview" data-palette-preview><span>${props.t("settings.customPaletteTitle")}</span><strong>${escapeHtml(settings.customTheme?.name ?? props.t("settings.themeCustom"))}</strong></div>
      </section>
      <section class="settings-section settings-section--compact">
        <label class="toggle-row"><span>${props.t("settings.showLogo")}</span><input type="checkbox" data-pref="showLogo" ${preferences.showLogo ? "checked" : ""}/></label>
        <label class="toggle-row"><span>${props.t("settings.showTooltip")}</span><input type="checkbox" data-pref="showTooltip" ${preferences.showTooltip ? "checked" : ""}/></label>
      </section>
    `;
  };

  const renderLanguage = (): string => `
    <section class="settings-section"><div class="section-heading"><h2>${props.t("settings.languageTitle")}</h2></div><div class="choice-stack" role="radiogroup" aria-label="${escapeHtml(props.t("settings.languageTitle"))}">
      <label class="choice-row"><input type="radio" name="localeChoice" value="auto" ${settings.localeChoice === "auto" ? "checked" : ""}/><span>${props.t("settings.languageAuto")}</span></label>
      <label class="choice-row"><input type="radio" name="localeChoice" value="zh-CN" ${settings.localeChoice === "zh-CN" ? "checked" : ""}/><span>${props.t("settings.languageZh")}</span></label>
      <label class="choice-row"><input type="radio" name="localeChoice" value="en-US" ${settings.localeChoice === "en-US" ? "checked" : ""}/><span>${props.t("settings.languageEn")}</span></label>
    </div></section>
  `;

  const renderTableStats = (): string => {
    if (!tableStats.tables.length) {
      return `<div class="empty-state">${props.t("common.noData")}</div>`;
    }
    const max = Math.max(...tableStats.tables.map((table) => table.rowCount), 1);
    return tableStats.tables.map((table) => `
      <div class="table-stat-row">
        <div class="table-stat-row__title"><strong>${escapeHtml(tableLabel(table))}</strong><span>${table.rowCount} ${props.t("settings.rows")} · ${table.estimated ? props.t("settings.estimated") : props.t("settings.exact")}</span></div>
        <div class="bar-track"><span style="width:${Math.min(100, (table.rowCount / max) * 100)}%"></span></div>
        <div class="table-stat-row__meta"><span>${formatBytes(table.sizeBytes)}</span>${table.clearable ? `<button class="button button--danger button--small" data-busy-key="clear-table" data-clear-table="${escapeHtml(table.name)}">${props.t("settings.clearTable")}</button>` : `<span class="muted">${props.t("settings.notClearable")}</span>`}</div>
      </div>
    `).join("");
  };

  const renderDatabase = (): string => `
    <section class="settings-section">
      <div class="section-heading"><h2>${props.t("settings.databaseTitle")}</h2></div>
      <div class="stats-grid"><div class="stat-card"><span>${props.t("settings.currentPath")}</span><strong>${escapeHtml(storage?.path ?? props.t("common.loading"))}</strong></div><div class="stat-card"><span>${props.t("settings.defaultPath")}</span><strong>${escapeHtml(storage?.defaultPath ?? props.t("common.loading"))}</strong></div><div class="stat-card"><span>${props.t("settings.size")}</span><strong>${storage ? formatBytes(storage.sizeBytes) : props.t("common.loading")}</strong></div><div class="stat-card"><span>${props.t("settings.tableCount")}</span><strong>${storage ? tableStats.tables.length : props.t("common.loading")}</strong></div></div>
      <div class="button-row"><button class="button button--primary" data-busy-key="choose-path" data-choose-path>${props.t("settings.choosePath")}</button><button class="button" data-busy-key="reset-path" data-reset-path>${props.t("settings.resetPath")}</button><button class="button" data-busy-key="refresh" data-refresh-storage>${props.t("common.refresh")}</button></div>
    </section>
    <section class="settings-section"><div class="section-heading"><h2>${props.t("settings.tableStats")}</h2><span class="muted">${formatBytes(tableStats.totalBytes)}</span></div><div class="table-stats">${renderTableStats()}</div></section>
  `;

  const render = (): void => {
    if (disposed) {
      return;
    }
    container.dataset.settingsTab = activeTab;
    container.innerHTML = `<section class="page page--settings"><div class="page-header"><span class="eyebrow">${props.t("settings.eyebrow")}</span><h1>${props.t("settings.title")}</h1><p>${props.t("settings.description")}</p></div><div class="settings-layout">${renderTabs()}<div class="settings-content">${activeTab === "personalization" ? renderPersonalization() : activeTab === "language" ? renderLanguage() : renderDatabase()}</div></div></section>`;
  };

  render();
  const cleanupClick = on(container, "click", (event) => {
    const target = event.target;
    if (!(target instanceof HTMLElement) || disposed) {
      return;
    }
    const tab = target.closest<HTMLElement>("[data-settings-tab]")?.dataset.settingsTab as SettingsTab | undefined;
    if (tab) {
      activeTab = tab;
      render();
      if (tab === "database") {
        refreshDatabase();
      }
      return;
    }
    if (target.closest("[data-save-palette]")) {
      if (!paletteKeys.every((key) => isValidHexColor(customPalette[key]))) {
        props.onToast(props.t("settings.invalidPalette"), "error");
        return;
      }
      const customTheme = { name: qs<HTMLInputElement>(container, "[data-palette-name]")?.value || props.t("settings.defaultPaletteName"), mode: customMode, palette: customPalette };
      if (!validateCustomTheme(customTheme)) {
        props.onToast(props.t("settings.invalidPalette"), "error");
        return;
      }
      const previousSettings = settings;
      const previousPalette = { ...customPalette };
      const previousMode = customMode;
      run("save-palette", () => SettingsService.setCustomTheme(customTheme), (next) => {
        settings = next;
        customMode = next.customTheme?.mode ?? customMode;
        props.onSettingsChange(next);
        props.onToast(props.t("common.success"), "success");
        render();
      }, () => {
        settings = previousSettings;
        customPalette = previousPalette;
        customMode = previousMode;
        applyTheme(previousSettings.themeChoice, previousSettings.customTheme);
        render();
      });
      return;
    }
    if (target.closest("[data-reset-palette]")) {
      const previousSettings = settings;
      const previousPalette = { ...customPalette };
      const previousMode = customMode;
      run("reset-palette", () => SettingsService.resetCustomTheme(), (next) => {
        settings = next;
        customPalette = { ...applyTheme(next.themeChoice, next.customTheme) };
        props.onSettingsChange(next);
        render();
      }, () => {
        settings = previousSettings;
        customPalette = previousPalette;
        customMode = previousMode;
        applyTheme(previousSettings.themeChoice, previousSettings.customTheme);
        render();
      });
      return;
    }
    if (target.closest("[data-choose-path]")) {
      if (busy.has("choose-path")) {
        return;
      }
      setBusy("choose-path", true);
      NativeDialogs.saveFile([{ name: props.t("settings.databaseFileFilter"), extensions: ["db", "sqlite", "sqlite3"] }], storage?.path)
        .then((path) => path ? setStoragePathWithConfirmation(path) : null)
        .then((next) => {
          if (!disposed && next) {
            storage = next;
            props.onToast(props.t("common.success"), "success");
            refreshDatabase();
            syncGlobalState();
          }
        })
        .catch((error: unknown) => {
          if (!disposed) {
            props.onToast(errorMessage(error), "error");
          }
        })
        .finally(() => {
          if (!disposed) {
            setBusy("choose-path", false);
          }
        });
      return;
    }
    if (target.closest("[data-reset-path]")) {
      if (busy.has("reset-path")) {
        return;
      }
      setBusy("reset-path", true);
      NativeDialogs.confirm({ title: props.t("settings.resetPathConfirmTitle"), message: props.t("settings.resetPathConfirmMessage"), okLabel: props.t("settings.resetPath"), cancelLabel: props.t("common.cancel") })
        .then((confirmed) => confirmed ? resetStoragePathWithConfirmation() : null)
        .then((next) => {
          if (!disposed && next) {
            storage = next;
            props.onToast(props.t("common.success"), "success");
            refreshDatabase();
            syncGlobalState();
          }
        })
        .catch((error: unknown) => {
          if (!disposed) {
            props.onToast(errorMessage(error), "error");
          }
        })
        .finally(() => {
          if (!disposed) {
            setBusy("reset-path", false);
          }
        });
      return;
    }
    if (target.closest("[data-refresh-storage]")) {
      refreshDatabase();
      return;
    }
    const table = target.closest<HTMLElement>("[data-clear-table]")?.dataset.clearTable;
    if (table) {
      if (busy.has("clear-table")) {
        return;
      }
      setBusy("clear-table", true);
      NativeDialogs.confirm({ title: props.t("settings.clearConfirmTitle"), message: props.t("settings.clearConfirmMessage"), okLabel: props.t("settings.clearTable"), cancelLabel: props.t("common.cancel") })
        .then((confirmed) => confirmed ? StorageService.clearTable(table) : null)
        .then((next) => {
          if (!disposed && next) {
            tableStats = next;
            props.onToast(props.t("settings.clearSuccess"), "success");
            refreshDatabase();
            syncGlobalState();
          }
        })
        .catch((error: unknown) => {
          if (!disposed) {
            props.onToast(errorMessage(error), "error");
          }
        })
        .finally(() => {
          if (!disposed) {
            setBusy("clear-table", false);
          }
        });
    }
  });

  const cleanupChange = on(container, "change", (event) => {
    const target = event.target;
    if (!(target instanceof HTMLInputElement) || disposed) {
      return;
    }
    if (target.name === "themeChoice") {
      const previous = settings;
      const choice = target.value as AppSettings["themeChoice"];
      applyTheme(choice, settings.customTheme);
      const previousPalette = { ...customPalette };
      const previousMode = customMode;
      run("theme-choice", () => SettingsService.setThemeChoice(choice), (next) => {
        settings = next;
        customPalette = { ...applyTheme(next.themeChoice, next.customTheme) };
        props.onSettingsChange(next);
        render();
      }, () => {
        settings = previous;
        customPalette = previousPalette;
        customMode = previousMode;
        applyTheme(previous.themeChoice, previous.customTheme);
        render();
      });
      if (busy.has("theme-choice")) {
        target.closest(".choice-card")?.classList.add("is-pending");
      }
      if (previous.themeChoice === choice) {
        applyTheme(previous.themeChoice, previous.customTheme);
      }
    }
    if (target.name === "localeChoice") {
      const choice = target.value as AppSettings["localeChoice"];
      const previous = settings;
      run("locale-choice", () => SettingsService.setLocaleChoice(choice), (next) => {
        settings = next;
        props.onSettingsChange(next);
        props.onLocaleChange();
      }, () => {
        settings = previous;
        render();
      });
    }
    if (target.name === "customMode") {
      customMode = target.value as ThemeMode;
      applyTheme("custom", { mode: customMode, palette: customPalette });
    }
    const pref = target.dataset.pref as keyof Preferences | undefined;
    if (pref) {
      const previous = preferences;
      const nextValue = target.checked;
      run(`pref-${pref}`, () => PreferencesService.set(pref, nextValue), (next) => {
        preferences = next;
        props.onPreferencesChange(next);
        render();
      }, () => {
        preferences = previous;
        render();
      });
    }
    const paletteToken = target.dataset.paletteToken as ThemeToken | undefined;
    if (paletteToken) {
      const value = target.value.trim();
      customPalette[paletteToken] = value;
      if (isValidHexColor(value)) {
        target.classList.remove("is-invalid");
        applyTheme("custom", { mode: customMode, palette: customPalette });
      } else {
        target.classList.add("is-invalid");
      }
    }
    const colorToken = target.dataset.paletteColor as ThemeToken | undefined;
    if (colorToken) {
      customPalette[colorToken] = target.value;
      const text = qs<HTMLInputElement>(container, `[data-palette-token="${colorToken}"]`);
      if (text) {
        text.value = target.value;
      }
      applyTheme("custom", { mode: customMode, palette: customPalette });
    }
  });

  if (activeTab === "database") {
    refreshDatabase();
  }

  return () => {
    disposed = true;
    cleanupClick();
    cleanupChange();
    container.replaceChildren();
  };
};
