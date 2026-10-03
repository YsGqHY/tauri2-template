import type { AppSettings, Cleanup, Preferences, StorageStats, StorageTableStats, TableStats, ThemeMode, ThemeToken } from "../../contracts/types";
import type { Translate } from "../../i18n";
import { toDisplayError } from "../../api/tauri";
import { NativeDialogs, PreferencesService, SettingsService, StorageService } from "../../services";
import {
  applyTheme,
  isValidCssColor,
  isValidHexColor,
  lightPalette,
  mergePalette,
  normalizeCustomTheme,
  seedCustomTheme,
  type ThemePalette,
} from "../../theme";
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

const paletteKeys: ThemeToken[] = [
  "bg", "surface", "surfaceRaised", "surfaceMuted", "text", "textMuted", "textSubtle", "border",
  "accent", "accentStrong", "accentSoft", "success", "warning", "danger", "info", "focus", "sidebar",
];

export const mountSettingsPage = (container: HTMLElement, props: SettingsPageProps): Cleanup => {
  let activeTab = (container.dataset.activeSettingsTab as SettingsTab | undefined) ?? "personalization";
  let settings = props.settings;
  let preferences = props.preferences;
  let storage: StorageStats | null = null;
  let tableStats: StorageTableStats = { totalBytes: 0, tables: [] };
  let disposed = false;
  let databaseRequest = 0;
  let paletteSaveTimer: number | null = null;
  let paletteRevision = 0;
  let mutationQueue: Promise<unknown> = Promise.resolve();
  const seededTheme = seedCustomTheme(settings.themeChoice, settings.customTheme);
  let customMode: ThemeMode = seededTheme.mode;
  let customPalette: ThemePalette = { ...seededTheme.palette };
  const busy = new Set<string>();

  const errorMessage = (error: unknown): string => {
    const payload = toDisplayError(error, props.t);
    return payload.nextStep ? `${payload.message} ${payload.nextStep}` : payload.message;
  };

  const enqueue = <T,>(task: () => Promise<T>): Promise<T> => {
    const next = mutationQueue.catch(() => undefined).then(task);
    mutationQueue = next.catch(() => undefined);
    return next;
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
    enqueue(task)
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

  const refreshDatabase = async (): Promise<void> => {
    if (disposed) {
      return;
    }
    const request = ++databaseRequest;
    setBusy("refresh", true);
    try {
      const snapshot = await StorageService.getSnapshot();
      if (disposed || request !== databaseRequest) {
        return;
      }
      storage = snapshot.storage;
      tableStats = snapshot.tableStats;
      if (activeTab === "database") {
        render();
      }
    } catch (error: unknown) {
      if (!disposed && request === databaseRequest) {
        props.onToast(errorMessage(error), "error");
      }
    } finally {
      if (!disposed && request === databaseRequest) {
        setBusy("refresh", false);
      }
    }
  };

  const syncGlobalState = (): void => {
    const refresh = props.onRefreshState?.();
    refresh?.catch((error: unknown) => {
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
      const overwrite = await NativeDialogs.confirm({
        title: props.t("settings.overwriteStorageTitle"),
        message: props.t("settings.overwriteStorageMessage"),
        okLabel: props.t("common.apply"),
        cancelLabel: props.t("common.cancel"),
      });
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
      const overwrite = await NativeDialogs.confirm({
        title: props.t("settings.overwriteStorageTitle"),
        message: props.t("settings.overwriteStorageMessage"),
        okLabel: props.t("common.apply"),
        cancelLabel: props.t("common.cancel"),
      });
      return overwrite ? StorageService.resetPath(true) : null;
    }
  };

  const tableLabel = (table: TableStats): string => {
    const translated = props.t(table.labelKey);
    return translated === table.labelKey ? table.name : translated;
  };

  const readCustomTheme = (): ReturnType<typeof normalizeCustomTheme> => {
    const name = qs<HTMLInputElement>(container, "[data-palette-name]")?.value.trim() || props.t("settings.defaultPaletteName");
    return normalizeCustomTheme({ name, mode: customMode, palette: customPalette });
  };

  const applyCustomPreview = (): void => {
    const preview = normalizeCustomTheme({ mode: customMode, palette: customPalette });
    if (preview) {
      applyTheme("custom", preview);
    }
  };

  const persistPalette = (showToast: boolean): void => {
    const request = paletteRevision;
    const customTheme = readCustomTheme();
    if (!customTheme) {
      if (request === paletteRevision && showToast) {
        props.onToast(props.t("settings.invalidPalette"), "error");
      }
      if (request === paletteRevision) {
        setBusy("save-palette", false);
      }
      return;
    }
    setBusy("save-palette", true);
    enqueue(() => SettingsService.setCustomTheme(customTheme))
      .then((next) => {
        if (disposed || request !== paletteRevision) {
          return;
        }
        settings = next;
        customMode = next.customTheme?.mode ?? customMode;
        customPalette = { ...mergePalette(customMode, next.customTheme?.palette) };
        props.onSettingsChange(next);
        if (showToast) {
          props.onToast(props.t("common.success"), "success");
        }
      })
      .catch((error: unknown) => {
        if (!disposed && request === paletteRevision) {
          props.onToast(errorMessage(error), "error");
        }
      })
      .finally(() => {
        if (!disposed && request === paletteRevision) {
          setBusy("save-palette", false);
        }
      });
  };

  const schedulePaletteSave = (): void => {
    paletteRevision += 1;
    if (paletteSaveTimer !== null) {
      window.clearTimeout(paletteSaveTimer);
    }
    paletteSaveTimer = window.setTimeout(() => {
      paletteSaveTimer = null;
      persistPalette(false);
    }, 280);
  };

  const cancelPaletteSave = (): void => {
    paletteRevision += 1;
    if (paletteSaveTimer !== null) {
      window.clearTimeout(paletteSaveTimer);
      paletteSaveTimer = null;
    }
  };

  const renderTabs = (): string => `
    <nav class="settings-tabs" aria-label="${escapeHtml(props.t("settings.title"))}">
      <button type="button" class="settings-tab ${activeTab === "personalization" ? "is-active" : ""}" data-settings-tab="personalization"><strong>${props.t("settings.personalization")}</strong><span>${props.t("settings.personalizationDescription")}</span></button>
      <button type="button" class="settings-tab ${activeTab === "language" ? "is-active" : ""}" data-settings-tab="language"><strong>${props.t("settings.language")}</strong><span>${props.t("settings.languageDescription")}</span></button>
      <button type="button" class="settings-tab ${activeTab === "database" ? "is-active" : ""}" data-settings-tab="database"><strong>${props.t("settings.database")}</strong><span>${props.t("settings.databaseDescription")}</span></button>
    </nav>
  `;

  const renderPersonalization = (): string => {
    const choices: Array<[AppSettings["themeChoice"], string]> = [
      ["system", props.t("settings.themeSystem")],
      ["light", props.t("settings.themeLight")],
      ["dark", props.t("settings.themeDark")],
      ["obsidian", props.t("settings.themeObsidian")],
      ["custom", props.t("settings.themeCustom")],
    ];
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
        <div class="button-row"><button type="button" class="button button--primary" data-busy-key="save-palette" data-save-palette>${props.t("common.save")}</button><button type="button" class="button" data-busy-key="reset-palette" data-reset-palette>${props.t("common.reset")}</button></div>
        <div class="palette-preview" data-palette-preview><span>${props.t("settings.customPaletteTitle")}</span><strong data-palette-preview-name>${escapeHtml(settings.customTheme?.name ?? props.t("settings.themeCustom"))}</strong></div>
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
    const maxRows = Math.max(...tableStats.tables.map((table) => table.rowCount), 1);
    const maxBytes = Math.max(...tableStats.tables.map((table) => table.sizeBytes), 1);
    return tableStats.tables.map((table) => {
      const ratio = table.sizeBytes > 0 ? table.sizeBytes / maxBytes : table.rowCount / maxRows;
      return `
      <div class="table-stat-row">
        <div class="table-stat-row__title"><strong>${escapeHtml(tableLabel(table))}</strong><span>${table.rowCount} ${props.t("settings.rows")} · ${table.estimated ? props.t("settings.estimated") : props.t("settings.exact")}</span></div>
        <div class="bar-track"><span style="width:${Math.min(100, ratio * 100)}%"></span></div>
        <div class="table-stat-row__meta"><span>${formatBytes(table.sizeBytes)}</span>${table.clearable ? `<button type="button" class="button button--danger button--small" data-busy-key="clear-table" data-clear-table="${escapeHtml(table.name)}">${props.t("settings.clearTable")}</button>` : `<span class="muted">${props.t("settings.notClearable")}</span>`}</div>
      </div>
    `;
    }).join("");
  };

  const renderDatabase = (): string => `
    <section class="settings-section">
      <div class="section-heading"><h2>${props.t("settings.databaseTitle")}</h2></div>
      <div class="stats-grid"><div class="stat-card"><span>${props.t("settings.currentPath")}</span><strong>${escapeHtml(storage?.path ?? props.t("common.loading"))}</strong></div><div class="stat-card"><span>${props.t("settings.defaultPath")}</span><strong>${escapeHtml(storage?.defaultPath ?? props.t("common.loading"))}</strong></div><div class="stat-card"><span>${props.t("settings.size")}</span><strong>${storage ? formatBytes(storage.sizeBytes) : props.t("common.loading")}</strong></div><div class="stat-card"><span>${props.t("settings.tableCount")}</span><strong>${storage ? tableStats.tables.length : props.t("common.loading")}</strong></div></div>
      <div class="button-row"><button type="button" class="button button--primary" data-busy-key="choose-path" data-choose-path>${props.t("settings.choosePath")}</button><button type="button" class="button" data-busy-key="reset-path" data-reset-path>${props.t("settings.resetPath")}</button><button type="button" class="button" data-busy-key="refresh" data-refresh-storage>${props.t("common.refresh")}</button></div>
    </section>
    <section class="settings-section"><div class="section-heading"><h2>${props.t("settings.tableStats")}</h2><span class="muted">${formatBytes(tableStats.totalBytes)}</span></div><div class="table-stats">${renderTableStats()}</div></section>
  `;

  const render = (): void => {
    if (disposed) {
      return;
    }
    container.dataset.activeSettingsTab = activeTab;
    container.innerHTML = `<section class="page page--settings"><div class="page-header"><span class="eyebrow">${props.t("settings.eyebrow")}</span><h1>${props.t("settings.title")}</h1><p>${props.t("settings.description")}</p></div><div class="settings-layout">${renderTabs()}<div class="settings-content">${activeTab === "personalization" ? renderPersonalization() : activeTab === "language" ? renderLanguage() : renderDatabase()}</div></div></section>`;
  };

  const restoreTheme = (previous: AppSettings, previousPalette: ThemePalette, previousMode: ThemeMode): void => {
    settings = previous;
    customPalette = previousPalette;
    customMode = previousMode;
    applyTheme(previous.themeChoice, previous.customTheme);
    render();
  };

  const handleThemeChoice = (choice: AppSettings["themeChoice"]): void => {
    const previousSettings = settings;
    const previousPalette = { ...customPalette };
    const previousMode = customMode;
    cancelPaletteSave();
    if (choice === "custom") {
      const seeded = readCustomTheme() ?? seedCustomTheme(settings.themeChoice, settings.customTheme);
      customMode = seeded.mode;
      customPalette = { ...seeded.palette };
      applyTheme("custom", seeded);
      run("theme-choice", () => SettingsService.setCustomTheme(seeded), (next) => {
        settings = next;
        props.onSettingsChange(next);
        render();
      }, () => restoreTheme(previousSettings, previousPalette, previousMode));
      return;
    }
    applyTheme(choice, settings.customTheme);
    run("theme-choice", () => SettingsService.setThemeChoice(choice), (next) => {
      settings = next;
      const seeded = seedCustomTheme(next.themeChoice, next.customTheme);
      customMode = seeded.mode;
      customPalette = { ...seeded.palette };
      props.onSettingsChange(next);
      render();
    }, () => restoreTheme(previousSettings, previousPalette, previousMode));
  };

  render();
  const cleanupClick = on(container, "click", (event) => {
    const target = event.target;
    if (!(target instanceof HTMLElement) || disposed) {
      return;
    }
    const tab = target.closest<HTMLElement>("button[data-settings-tab]")?.dataset.settingsTab as SettingsTab | undefined;
    if (tab) {
      activeTab = tab;
      render();
      if (tab === "database") {
        void refreshDatabase();
      }
      return;
    }
    if (target.closest("[data-save-palette]")) {
      cancelPaletteSave();
      paletteRevision += 1;
      persistPalette(true);
      return;
    }
    if (target.closest("[data-reset-palette]")) {
      cancelPaletteSave();
      const previousSettings = settings;
      const previousPalette = { ...customPalette };
      const previousMode = customMode;
      run("reset-palette", () => SettingsService.resetCustomTheme(), (next) => {
        settings = next;
        const seeded = seedCustomTheme(next.themeChoice, next.customTheme);
        customMode = seeded.mode;
        customPalette = { ...seeded.palette };
        props.onSettingsChange(next);
        render();
      }, () => restoreTheme(previousSettings, previousPalette, previousMode));
      return;
    }
    if (target.closest("[data-choose-path]")) {
      if (busy.has("choose-path")) {
        return;
      }
      setBusy("choose-path", true);
      enqueue(() => NativeDialogs.saveFile([{ name: props.t("settings.databaseFileFilter"), extensions: ["db", "sqlite", "sqlite3"] }], storage?.path))
        .then((path) => path ? setStoragePathWithConfirmation(path) : null)
        .then((next) => {
          if (!disposed && next) {
            storage = next;
            props.onToast(props.t("common.success"), "success");
            void refreshDatabase();
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
      enqueue(() => NativeDialogs.confirm({ title: props.t("settings.resetPathConfirmTitle"), message: props.t("settings.resetPathConfirmMessage"), okLabel: props.t("settings.resetPath"), cancelLabel: props.t("common.cancel") }))
        .then((confirmed) => confirmed ? resetStoragePathWithConfirmation() : null)
        .then((next) => {
          if (!disposed && next) {
            storage = next;
            props.onToast(props.t("common.success"), "success");
            void refreshDatabase();
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
      void refreshDatabase();
      return;
    }
    const table = target.closest<HTMLElement>("[data-clear-table]")?.dataset.clearTable;
    if (table) {
      if (busy.has("clear-table")) {
        return;
      }
      setBusy("clear-table", true);
      enqueue(() => NativeDialogs.confirm({ title: props.t("settings.clearConfirmTitle"), message: props.t("settings.clearConfirmMessage"), okLabel: props.t("settings.clearTable"), cancelLabel: props.t("common.cancel") }))
        .then((confirmed) => confirmed ? StorageService.clearTable(table) : null)
        .then((next) => {
          if (!disposed && next) {
            tableStats = next;
            props.onToast(props.t("settings.clearSuccess"), "success");
            void refreshDatabase();
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
      handleThemeChoice(target.value as AppSettings["themeChoice"]);
      return;
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
      return;
    }
    if (target.name === "customMode") {
      customMode = target.value as ThemeMode;
      customPalette = { ...mergePalette(customMode, customPalette) };
      applyCustomPreview();
      schedulePaletteSave();
      return;
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
      return;
    }
    const colorToken = target.dataset.paletteColor as ThemeToken | undefined;
    if (colorToken) {
      customPalette[colorToken] = target.value;
      const text = qs<HTMLInputElement>(container, `[data-palette-token="${colorToken}"]`);
      if (text) {
        text.value = target.value;
      }
      applyCustomPreview();
      schedulePaletteSave();
    }
  });

  const cleanupInput = on(container, "input", (event) => {
    const target = event.target;
    if (!(target instanceof HTMLInputElement) || disposed) {
      return;
    }
    const paletteToken = target.dataset.paletteToken as ThemeToken | undefined;
    if (paletteToken) {
      const value = target.value.trim();
      customPalette[paletteToken] = value;
      target.classList.toggle("is-invalid", !isValidCssColor(value));
      if (isValidCssColor(value)) {
        applyCustomPreview();
        schedulePaletteSave();
      }
      return;
    }
    if (target.matches("[data-palette-name]")) {
      const preview = qs<HTMLElement>(container, "[data-palette-preview-name]");
      if (preview) {
        preview.textContent = target.value.trim() || props.t("settings.themeCustom");
      }
      schedulePaletteSave();
    }
  });

  if (activeTab === "database") {
    void refreshDatabase();
  }

  return () => {
    disposed = true;
    cancelPaletteSave();
    cleanupClick();
    cleanupChange();
    cleanupInput();
    container.replaceChildren();
  };
};
