import type { Cleanup, Preferences } from "../contracts/types";
import type { RouteDefinition, RouteId, Router } from "../router";
import type { Translate } from "../i18n";
import { on } from "../shared/dom";

export interface SidebarProps {
  t: Translate;
  router: Router;
  preferences: Preferences;
  onTogglePreference: (key: keyof Preferences, value: boolean) => void;
}

export const mountSidebar = (container: HTMLElement, props: SidebarProps): Cleanup => {
  const render = (activeId: RouteId): void => {
    const primary = props.router.getRoutes().filter((route) => route.slot === "primary");
    const footer = props.router.getRoutes().filter((route) => route.slot === "footer");
    const nav = (routes: RouteDefinition[]): string => routes.map((route) => `
      <button class="sidebar-item ${route.id === activeId ? "is-active" : ""}" data-route="${route.id}" title="${props.preferences.showTooltip ? props.t(route.labelKey) : ""}" aria-label="${props.t(route.labelKey)}">
        <span class="sidebar-item__glyph" aria-hidden="true">${route.id === "home" ? "H" : route.id === "settings" ? "S" : route.id === "x-pro-demo" ? "X" : "P"}</span>
        <span class="sidebar-item__label">${props.t(route.labelKey)}</span>
      </button>
    `).join("");
    container.innerHTML = `
      <aside class="sidebar" data-sidebar>
        <div class="sidebar__top">
          <div class="sidebar__logo ${props.preferences.showLogo ? "" : "is-hidden"}" aria-label="${props.t("app.name")}">F</div>
          <div class="sidebar__nav" aria-label="${props.t("sidebar.navigation")}">${nav(primary)}</div>
        </div>
        <div class="sidebar__footer">
          <label class="sidebar-preference">
            <input type="checkbox" data-preference="showTooltip" ${props.preferences.showTooltip ? "checked" : ""} />
            <span>${props.t("sidebar.tooltipHint")}</span>
          </label>
          ${nav(footer)}
        </div>
      </aside>
    `;
  };

  render(props.router.getActive().id);
  const cleanupClick = on(container, "click", (event) => {
    const target = event.target;
    if (!(target instanceof HTMLElement)) {
      return;
    }
    const routeElement = target.closest<HTMLElement>("[data-route]");
    const routeId = routeElement?.dataset.route as RouteId | undefined;
    if (routeId) {
      props.router.navigate(routeId);
    }
  });
  const cleanupChange = on(container, "change", (event) => {
    const target = event.target;
    if (!(target instanceof HTMLInputElement)) {
      return;
    }
    const key = target.dataset.preference as keyof Preferences | undefined;
    if (key) {
      props.onTogglePreference(key, target.checked);
    }
  });
  const cleanupRoute = props.router.subscribe((route) => render(route.id));
  return () => {
    cleanupClick();
    cleanupChange();
    cleanupRoute();
    container.replaceChildren();
  };
};
