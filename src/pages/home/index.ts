import type { ChildClosedPayload, ChildEventPayload, ChildMessagePayload, Cleanup, TimePayload } from "../../contracts/types";
import type { Translate } from "../../i18n";
import { listenEvent, type Unlisten } from "../../api/events";
import { toDisplayError } from "../../api/tauri";
import { AppService, ChildWindowService, type ChildOpenResult } from "../../services";
import { AppEvents } from "../../shared/events";
import { escapeHtml, on, qs } from "../../shared/dom";

export interface HomePageProps {
  t: Translate;
  onToast: (message: string, kind?: "info" | "success" | "error") => void;
}

export const mountHomePage = (container: HTMLElement, props: HomePageProps): Cleanup => {
  let latestTime = props.t("home.noTime");
  let greeting = "";
  let disposed = false;
  let greetingRequest = 0;
  let greetBusy = false;
  let childBusy = false;
  let timeUnlisten: Unlisten = () => undefined;
  const childHandles = new Map<string, ChildOpenResult>();
  let blankChildId: string | null = null;

  const displayError = (error: unknown): string => {
    const payload = toDisplayError(error, props.t);
    return payload.nextStep ? `${payload.message} ${payload.nextStep}` : payload.message;
  };

  const formatLocalTime = (unixMs: number): string => {
    const date = new Date(unixMs);
    if (Number.isNaN(date.getTime())) {
      return props.t("home.noTime");
    }
    const pad = (value: number): string => String(value).padStart(2, "0");
    return `${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`;
  };

  const setBusy = (selector: string, busy: boolean): void => {
    container.querySelectorAll<HTMLElement>(selector).forEach((element) => element.toggleAttribute("disabled", busy));
  };

  const render = (): void => {
    if (disposed) {
      return;
    }
    container.innerHTML = `
      <section class="page page--home">
        <div class="page-header"><span class="eyebrow">${props.t("home.eyebrow")}</span><h1>${props.t("home.title")}</h1><p>${props.t("home.description")}</p></div>
        <div class="notice notice--info" data-runtime-notice hidden>${props.t("home.runtimeNotice")}</div>
        <div class="home-grid">
          <article class="card card--hero"><div class="card__eyebrow">${props.t("home.greetTitle")}</div><h2>${props.t("home.greetTitle")}</h2><p>${props.t("home.greetDescription")}</p>
            <form class="form-stack" data-greet-form><label for="greet-input">${props.t("home.nameLabel")}</label><div class="form-row"><input id="greet-input" name="name" placeholder="${escapeHtml(props.t("home.namePlaceholder"))}" autocomplete="name"/><button class="button button--primary" type="submit" data-greet-submit>${props.t("home.greet")}</button></div></form>
            <div class="result-box" data-greeting-result aria-live="polite">${escapeHtml(greeting || props.t("home.greeting"))}</div>
          </article>
          <article class="card"><div class="card__eyebrow">${props.t("home.timeTitle")}</div><h2>${props.t("home.timeTitle")}</h2><p>${props.t("home.timeDescription")}</p><div class="metric" data-time-value>${escapeHtml(latestTime)}</div></article>
          <article class="card card--wide"><div class="card__eyebrow">${props.t("home.childTitle")}</div><h2>${props.t("home.childTitle")}</h2><p>${props.t("home.childDescription")}</p>
            <div class="button-row"><button class="button" data-child="confirm" ${childBusy ? "disabled" : ""}>${props.t("home.confirm")}</button><button class="button" data-child="message" ${childBusy ? "disabled" : ""}>${props.t("home.message")}</button><button class="button" data-child="blank" ${childBusy ? "disabled" : ""}>${props.t("home.blank")}</button></div>
            <div class="form-row child-message-row"><input data-child-message placeholder="${escapeHtml(props.t("home.childMessagePlaceholder"))}"/><button class="button button--small" data-child-send>${props.t("home.sendToChild")}</button><button class="button button--small" data-child-broadcast>${props.t("home.broadcastToChild")}</button></div>
            <div class="result-box" data-child-result aria-live="polite">${props.t("home.childHint")}</div>
          </article>
          <article class="card card--wide"><div class="card__eyebrow">${props.t("home.appInfoTitle")}</div><h2>${props.t("home.appInfoTitle")}</h2><button class="button" data-load-info>${props.t("home.loadInfo")}</button><dl class="info-list" data-app-info><div><dt>${props.t("common.noData")}</dt><dd>${props.t("common.noData")}</dd></div></dl></article>
        </div>
      </section>
    `;
    if (document.documentElement.dataset.runtime !== "tauri") {
      qs<HTMLElement>(container, "[data-runtime-notice]")?.removeAttribute("hidden");
    }
  };

  const setChildResult = (payload: ChildEventPayload | ChildClosedPayload | ChildMessagePayload): void => {
    const result = qs<HTMLElement>(container, "[data-child-result]");
    if (!result || disposed) {
      return;
    }
    if ("status" in payload) {
      const detail = payload.message ?? (payload.data === undefined ? "" : typeof payload.data === "string" ? payload.data : JSON.stringify(payload.data));
      const status = payload.status === "opened" ? props.t("home.opened") : payload.status === "closed" ? props.t("home.childClosed") : props.t("home.childResult");
      result.textContent = detail ? `${status}: ${detail}` : status;
      return;
    }
    if ("data" in payload) {
      const detail = typeof payload.data === "string" ? payload.data : JSON.stringify(payload.data);
      result.textContent = detail ? `${props.t("home.childResult")}: ${detail}` : props.t("home.childResult");
      return;
    }
    result.textContent = props.t("home.childClosed");
  };

  render();
  const form = qs<HTMLFormElement>(container, "[data-greet-form]");
  const input = qs<HTMLInputElement>(container, "#greet-input");
  const result = qs<HTMLElement>(container, "[data-greeting-result]");
  const info = qs<HTMLElement>(container, "[data-app-info]");

  const cleanupForm = form ? on(form, "submit", (event) => {
    event.preventDefault();
    const name = input?.value.trim() ?? "";
    if (!name || !result) {
      if (result) {
        result.textContent = props.t("settings.emptyName");
      }
      return;
    }
    if (greetBusy) {
      return;
    }
    greetBusy = true;
    const requestId = ++greetingRequest;
    setBusy("[data-greet-submit]", true);
    result.textContent = props.t("common.loading");
    AppService.greet(name)
      .then((message) => {
        if (!disposed && requestId === greetingRequest) {
          greeting = message;
          result.textContent = message;
          props.onToast(props.t("common.success"), "success");
        }
      })
      .catch((error: unknown) => {
        if (!disposed && requestId === greetingRequest) {
          result.textContent = displayError(error);
          props.onToast(displayError(error), "error");
        }
      })
      .finally(() => {
        if (!disposed && requestId === greetingRequest) {
          greetBusy = false;
          setBusy("[data-greet-submit]", false);
        }
      });
  }) : () => undefined;

  const cleanupChildren = on(container, "click", (event) => {
    const target = event.target;
    if (!(target instanceof HTMLElement) || disposed) {
      return;
    }
    const childKind = target.closest<HTMLElement>("[data-child]")?.dataset.child;
    if (childKind !== "confirm" && childKind !== "message" && childKind !== "blank") {
      if (target.closest("[data-child-send]")) {
        const message = qs<HTMLInputElement>(container, "[data-child-message]")?.value.trim() ?? "";
        if (!blankChildId || !message) {
          return;
        }
        ChildWindowService.send(blankChildId, message)
          .catch((error: unknown) => props.onToast(displayError(error), "error"));
        return;
      }
      if (target.closest("[data-child-broadcast]")) {
        const message = qs<HTMLInputElement>(container, "[data-child-message]")?.value.trim() ?? "";
        if (!message) {
          return;
        }
        ChildWindowService.broadcast(message)
          .catch((error: unknown) => props.onToast(displayError(error), "error"));
      }
      return;
    }
    if (childBusy) {
      return;
    }
    childBusy = true;
    setBusy("[data-child]", true);
    const kind = childKind;
    const title = kind === "confirm" ? props.t("child.confirmTitle") : kind === "message" ? props.t("child.messageTitle") : props.t("child.blankTitle");
    const message = kind === "confirm" ? props.t("child.confirmMessage") : kind === "message" ? props.t("child.messageBody") : props.t("child.blankBody");
    let terminalBeforeHandle = false;
    let handle: ChildOpenResult | null = null;
    ChildWindowService.open(kind, { title, message, width: 520, height: 360 }, {
      onResult: (payload) => {
        if (payload.status === "result" || payload.status === "closed") {
          terminalBeforeHandle = true;
        }
        setChildResult(payload);
        if (payload.status === "result") {
          handle?.dispose();
        }
      },
      onClosed: (payload) => {
        terminalBeforeHandle = true;
        setChildResult(payload);
        handle?.dispose();
      },
      onSend: (payload) => setChildResult(payload),
      onBroadcast: (payload) => setChildResult(payload),
    })
      .then((next) => {
        if (disposed) {
          next.dispose();
          return;
        }
        handle = next;
        childHandles.set(next.id, next);
        if (kind === "blank") {
          blankChildId = next.id;
        }
        if (terminalBeforeHandle) {
          next.dispose();
          childHandles.delete(next.id);
        }
        setChildResult({ id: next.id, kind, status: "opened" });
      })
      .catch((error: unknown) => {
        if (!disposed) {
          props.onToast(displayError(error), "error");
        }
      })
      .finally(() => {
        if (!disposed) {
          childBusy = false;
          setBusy("[data-child]", false);
        }
      });
  });

  const cleanupInfo = on(container, "click", (event) => {
    const target = event.target;
    if (!(target instanceof HTMLElement) || !target.closest("[data-load-info]") || !info || disposed) {
      return;
    }
    info.innerHTML = `<div><dt>${props.t("common.loading")}</dt><dd>${props.t("common.loading")}</dd></div>`;
    AppService.getInfo()
      .then((appInfo) => {
        if (!disposed) {
          info.innerHTML = `<div><dt>${props.t("app.name")}</dt><dd>${escapeHtml(appInfo.name)}</dd></div><div><dt>${props.t("common.version")}</dt><dd>${escapeHtml(appInfo.version)}</dd></div><div><dt>${props.t("common.platform")}</dt><dd>${escapeHtml(appInfo.platform)} / ${escapeHtml(appInfo.arch)}</dd></div>`;
        }
      })
      .catch((error: unknown) => {
        if (!disposed) {
          info.innerHTML = `<div><dt>${props.t("common.failed")}</dt><dd>${escapeHtml(displayError(error))}</dd></div>`;
        }
      });
  });

  listenEvent<TimePayload>(AppEvents.time, (payload) => {
    if (disposed) {
      return;
    }
    latestTime = formatLocalTime(payload.unixMs);
    qs<HTMLElement>(container, "[data-time-value]")?.replaceChildren(document.createTextNode(latestTime));
  }).then((unlisten) => {
    if (disposed) {
      unlisten();
    } else {
      timeUnlisten = unlisten;
    }
  }).catch(() => undefined);

  return () => {
    disposed = true;
    greetingRequest += 1;
    cleanupForm();
    cleanupChildren();
    cleanupInfo();
    timeUnlisten();
    childHandles.forEach((handle) => handle.dispose());
    childHandles.clear();
    container.replaceChildren();
  };
};
