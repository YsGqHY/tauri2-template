import type { Cleanup } from "../contracts/types";
import type { Translate } from "../i18n";
import { WindowService } from "../services";
import { on } from "../shared/dom";

export interface TitleBarProps {
  t: Translate;
  onError: (error: unknown) => void;
}

export const mountTitleBar = (container: HTMLElement, props: TitleBarProps): Cleanup => {
  container.innerHTML = `
    <header class="title-bar" data-tauri-drag-region>
      <div class="title-bar__brand">
        <span class="title-bar__mark" aria-hidden="true">F</span>
        <span class="title-bar__title">${props.t("app.name")}</span>
      </div>
      <div class="title-bar__controls" data-tauri-no-drag>
        <button class="window-control" data-window-action="minimize" aria-label="${props.t("titleBar.minimize")}" title="${props.t("titleBar.minimize")}">-</button>
        <button class="window-control" data-window-action="maximize" aria-label="${props.t("titleBar.maximize")}" title="${props.t("titleBar.maximize")}">+</button>
        <button class="window-control window-control--close" data-window-action="close" aria-label="${props.t("titleBar.close")}" title="${props.t("titleBar.close")}">x</button>
      </div>
    </header>
  `;
  const handler = (event: Event): void => {
    const target = event.target;
    if (!(target instanceof HTMLElement)) {
      return;
    }
    const action = target.dataset.windowAction;
    if (!action) {
      return;
    }
    const task = action === "minimize" ? WindowService.minimize() : action === "maximize" ? WindowService.toggleMaximize() : WindowService.close();
    task.catch(props.onError);
  };
  const cleanup = on(container, "click", handler);
  return () => {
    cleanup();
    container.replaceChildren();
  };
};
