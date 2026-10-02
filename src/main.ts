import type { AppSettings, ChildWindowType, Preferences } from "./contracts/types";
import { isTauriRuntime, toDisplayError } from "./api/tauri";
import { AppShell } from "./components/app-shell";
import { i18n } from "./i18n";
import { Router, type RouteDefinition } from "./router";
import { appStore } from "./store";
import { applyTheme, disposeTheme } from "./theme";
import { AppService, PreferencesService, SettingsService, TrayService } from "./services";
import { mountChildPage } from "./pages/child";
import { mountHomePage } from "./pages/home";
import { mountSettingsPage } from "./pages/settings";
import { mountSubprocessPage } from "./pages/subprocess";
import { mountXProPage } from "./pages/x-pro";
import { escapeHtml } from "./shared/dom";
import { installScrollBehavior } from "./shared/scroll";

const defaultSettings: AppSettings = {
  themeChoice: "system",
  customTheme: null,
  localeChoice: "auto",
};

const defaultPreferences: Preferences = {
  showLogo: true,
  showTooltip: true,
};

interface BootstrapQuery {
  child: ChildWindowType | null;
  id: string;
  message?: string;
}

const parseQuery = (): BootstrapQuery => {
  const params = new URLSearchParams(window.location.search);
  const rawChild = params.get("child") ?? params.get("window");
  const child = rawChild === "confirm" || rawChild === "message" || rawChild === "blank" ? rawChild : null;
  return {
    child,
    id: params.get("id") ?? "child",
    message: params.get("message") ?? undefined,
  };
};

const hideStartupSkeleton = (): void => {
  const skeleton = document.querySelector<HTMLElement>("#startup-skeleton");
  if (!skeleton) {
    return;
  }
  skeleton.classList.add("is-hidden");
  window.setTimeout(() => skeleton.remove(), 180);
};

const showFatal = (container: HTMLElement, message: string): void => {
  container.innerHTML = `<main class="child-window"><section class="child-window__content"><span class="eyebrow">${i18n.t("errors.title")}</span><h1>${escapeHtml(message)}</h1><p>${i18n.t("errors.next")}</p></section></main>`;
};

const bootstrapChild = (container: HTMLElement, query: BootstrapQuery): void => {
  applyTheme("system");
  const cleanup = mountChildPage(container, {
    t: i18n.t,
    kind: query.child ?? "blank",
    id: query.id,
    message: query.message,
    onError: (error) => showFatal(container, toDisplayError(error).message),
  });
  const scrollCleanup = installScrollBehavior(container);
  window.addEventListener("beforeunload", () => {
    cleanup();
    scrollCleanup();
  }, { once: true });
};

const bootstrapMain = (container: HTMLElement): void => {
  let settings: AppSettings = { ...defaultSettings };
  const preferences: Preferences = { ...defaultPreferences };
  const runtimeAvailable = isTauriRuntime();
  appStore.setState({ settings, preferences, runtimeAvailable });
  applyTheme(settings.themeChoice, settings.customTheme);

  const routes: RouteDefinition[] = [
    {
      id: "home",
      labelKey: "route.home",
      slot: "primary",
      keepAlive: true,
      mount: (page) => mountHomePage(page, { t: i18n.t, onToast: (message, kind) => shell?.showToast(message, kind) }),
    },
    {
      id: "x-pro-demo",
      labelKey: "route.xPro",
      slot: "primary",
      keepAlive: true,
      mount: (page) => mountXProPage(page, { t: i18n.t }),
    },
    {
      id: "subprocess",
      labelKey: "route.subprocess",
      slot: "primary",
      keepAlive: false,
      mount: (page) => mountSubprocessPage(page, { t: i18n.t, onToast: (message, kind) => shell?.showToast(message, kind) }),
    },
    {
      id: "settings",
      labelKey: "route.settings",
      slot: "footer",
      keepAlive: true,
      mount: (page) => mountSettingsPage(page, {
        t: i18n.t,
        settings,
        preferences,
        onSettingsChange: (next) => {
          settings = next;
          appStore.setState({ settings: next });
          i18n.setChoice(next.localeChoice);
          applyTheme(next.themeChoice, next.customTheme);
        },
        onPreferencesChange: (next) => {
          Object.assign(preferences, next);
          appStore.setState({ preferences: next });
          shell?.refreshChrome();
        },
        onLocaleChange: () => {
          i18n.setChoice(settings.localeChoice);
        },
        onRefreshState: () => refreshState(),
        onToast: (message, kind) => shell?.showToast(message, kind),
      }),
    },
  ];
  const router = new Router(routes);
  let shell: AppShell | null = null;
  shell = new AppShell(container, {
    t: i18n.t,
    router,
    preferences,
    onTogglePreference: (key, value) => {
      PreferencesService.set(key, value)
        .then((next) => {
          Object.assign(preferences, next);
          appStore.setState({ preferences: next });
          shell?.refreshChrome();
        })
        .catch((error: unknown) => shell?.showToast(toDisplayError(error, i18n.t).message, "error"));
    },
    onError: (error) => shell?.showToast(toDisplayError(error).message, "error"),
  });
  const shellCleanup = shell.mount();

  const applySettings = (next: AppSettings): void => {
    settings = next;
    appStore.setState({ settings: next });
    i18n.setChoice(next.localeChoice);
    applyTheme(next.themeChoice, next.customTheme);
    shell?.refreshChrome();
    router.refresh();
  };

  const refreshState = (): Promise<void> => Promise.all([SettingsService.get(), PreferencesService.get()]).then(([nextSettings, nextPreferences]) => {
    applySettings(nextSettings);
    Object.assign(preferences, nextPreferences);
    appStore.setState({ preferences: nextPreferences });
    shell?.refreshChrome();
  });

  SettingsService.get().then(applySettings).catch((error: unknown) => shell?.showToast(toDisplayError(error, i18n.t).message, "error"));
  PreferencesService.get().then((next) => {
    Object.assign(preferences, next);
    appStore.setState({ preferences: next });
    shell?.refreshChrome();
  }).catch((error: unknown) => shell?.showToast(toDisplayError(error, i18n.t).message, "error"));
  AppService.getInfo().then((info) => appStore.setState({ appInfo: info })).catch((error: unknown) => shell?.showToast(toDisplayError(error, i18n.t).message, "error"));

  let trayUnlisten: () => void = () => undefined;
  TrayService.subscribe((payload) => {
    if (payload.action === "home" || payload.action === "settings") {
      router.navigate(payload.action);
    } else if (payload.action === "show") {
      shell?.showToast(i18n.t("common.success"), "info");
    }
  }).then((unlisten) => {
    trayUnlisten = unlisten;
  }).catch((error: unknown) => shell?.showToast(toDisplayError(error, i18n.t).message, "error"));

  const unsubscribeI18n = i18n.subscribe(() => {
    shell?.refreshChrome();
    router.refresh();
  });
  window.addEventListener("beforeunload", () => {
    unsubscribeI18n();
    trayUnlisten();
    disposeTheme();
    shellCleanup();
  }, { once: true });
};

const bootstrap = (): void => {
  const container = document.querySelector<HTMLElement>("#app");
  if (!container) {
    return;
  }
  document.documentElement.dataset.runtime = isTauriRuntime() ? "tauri" : "browser";
  const query = parseQuery();
  if (query.child) {
    bootstrapChild(container, query);
  } else {
    bootstrapMain(container);
  }
  hideStartupSkeleton();
};

window.addEventListener("DOMContentLoaded", bootstrap, { once: true });
