import type { Cleanup, Mount } from "../contracts/types";
import { appStore } from "../store";

export type RouteId = "home" | "settings" | "x-pro-demo" | "subprocess";

export interface RouteDefinition {
  id: RouteId;
  labelKey: string;
  slot: "primary" | "footer";
  keepAlive: boolean;
  mount: Mount;
}

interface RouteEntry {
  route: RouteDefinition;
  host: HTMLElement;
  cleanup: Cleanup;
}

export class Router {
  private readonly entries = new Map<RouteId, RouteEntry>();
  private currentId: RouteId = "home";
  private readonly listeners = new Set<(route: RouteDefinition) => void>();
  private outlet: HTMLElement | null = null;
  private disposed = false;

  constructor(private readonly routes: RouteDefinition[]) {}

  getRoutes(): RouteDefinition[] {
    return this.routes;
  }

  getActive(): RouteDefinition {
    return this.routes.find((route) => route.id === this.currentId) ?? this.routes[0]!;
  }

  subscribe(listener: (route: RouteDefinition) => void): Cleanup {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  mount(container: HTMLElement): Cleanup {
    this.outlet = container;
    this.navigate(this.currentId);
    return () => this.dispose();
  }

  navigate(id: RouteId): void {
    if (this.disposed) {
      return;
    }
    const route = this.routes.find((candidate) => candidate.id === id);
    if (!route || !this.outlet) {
      return;
    }
    const previous = this.entries.get(this.currentId);
    if (this.currentId !== id && previous) {
      if (previous.route.keepAlive) {
        previous.host.hidden = true;
      } else {
        previous.cleanup();
        previous.host.remove();
        this.entries.delete(this.currentId);
      }
    }

    this.currentId = id;
    appStore.setState({ route: id });
    let entry = this.entries.get(id);
    if (!entry) {
      const host = document.createElement("section");
      host.className = "route-host";
      host.dataset.route = id;
      host.hidden = false;
      this.outlet.append(host);
      entry = { route, host, cleanup: route.mount(host) };
      this.entries.set(id, entry);
    } else {
      entry.host.hidden = false;
    }
    this.entries.forEach((candidate, candidateId) => {
      candidate.host.hidden = candidateId !== id;
    });
    this.listeners.forEach((listener) => listener(route));
  }

  refresh(): void {
    if (this.disposed) {
      return;
    }
    const entry = this.entries.get(this.currentId);
    if (!entry) {
      this.navigate(this.currentId);
      return;
    }
    entry.cleanup();
    entry.host.replaceChildren();
    entry.cleanup = entry.route.mount(entry.host);
  }

  private dispose(): void {
    if (this.disposed) {
      return;
    }
    this.disposed = true;
    this.entries.forEach((entry) => entry.cleanup());
    this.entries.clear();
    this.listeners.clear();
    this.outlet = null;
  }
}
