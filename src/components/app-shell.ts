import type { Cleanup, Preferences } from "../contracts/types";
import type { Translate } from "../i18n";
import { Router } from "../router";
import { mountSidebar } from "./sidebar";
import { mountTitleBar } from "./title-bar";
import { installScrollBehavior, type ScrollCleanup } from "../shared/scroll";

export interface AppShellProps {
  t: Translate;
  router: Router;
  preferences: Preferences;
  onTogglePreference: (key: keyof Preferences, value: boolean) => void;
  onError: (error: unknown) => void;
}

export class AppShell {
  private titleBarCleanup: Cleanup = () => undefined;
  private sidebarCleanup: Cleanup = () => undefined;
  private routerCleanup: Cleanup = () => undefined;
  private scrollCleanup: ScrollCleanup = () => undefined;
  private readonly toastTimers = new Set<number>();
  private titleBarHost: HTMLElement | null = null;
  private sidebarHost: HTMLElement | null = null;
  private readonly props: AppShellProps;

  constructor(private readonly root: HTMLElement, props: AppShellProps) {
    this.props = props;
  }

  mount(): Cleanup {
    this.root.innerHTML = `
      <div class="app-shell">
        <div class="app-shell__titlebar" data-titlebar></div>
        <div class="app-shell__body">
          <div class="app-shell__sidebar" data-sidebar-host></div>
          <main class="app-shell__main" data-main-content data-scroll-container="page">
            <div class="router-outlet" data-router-outlet></div>
          </main>
        </div>
        <div class="toast-stack" data-toast-stack aria-live="polite"></div>
      </div>
    `;
    this.titleBarHost = this.root.querySelector<HTMLElement>("[data-titlebar]");
    this.sidebarHost = this.root.querySelector<HTMLElement>("[data-sidebar-host]");
    if (!this.titleBarHost || !this.sidebarHost) {
      return () => undefined;
    }
    this.renderChrome();
    this.scrollCleanup = installScrollBehavior(this.root);
    const outlet = this.root.querySelector<HTMLElement>("[data-router-outlet]");
    this.routerCleanup = outlet ? this.props.router.mount(outlet) : () => undefined;
    return () => this.cleanup();
  }

  refreshChrome(): void {
    this.renderChrome();
  }

  showToast(message: string, kind: "info" | "success" | "error" = "info"): void {
    const stack = this.root.querySelector<HTMLElement>("[data-toast-stack]");
    if (!stack) {
      return;
    }
    const item = document.createElement("div");
    item.className = `toast toast--${kind}`;
    item.textContent = message;
    stack.append(item);
    const timer = window.setTimeout(() => {
      item.remove();
      this.toastTimers.delete(timer);
    }, 3600);
    this.toastTimers.add(timer);
  }

  private renderChrome(): void {
    if (!this.titleBarHost || !this.sidebarHost) {
      return;
    }
    this.titleBarCleanup();
    this.sidebarCleanup();
    this.titleBarCleanup = mountTitleBar(this.titleBarHost, {
      t: this.props.t,
      onError: this.props.onError,
    });
    this.sidebarCleanup = mountSidebar(this.sidebarHost, {
      t: this.props.t,
      router: this.props.router,
      preferences: this.props.preferences,
      onTogglePreference: this.props.onTogglePreference,
    });
  }

  private cleanup(): void {
    this.titleBarCleanup();
    this.sidebarCleanup();
    this.routerCleanup();
    this.scrollCleanup();
    this.toastTimers.forEach((timer) => window.clearTimeout(timer));
    this.toastTimers.clear();
    this.root.replaceChildren();
  }
}
