import type { ChildMessagePayload, ChildWindowType, Cleanup } from "../../contracts/types";
import type { Translate } from "../../i18n";
import { safeListen, type Unlisten } from "../../api/events";
import { ChildWindowService, WindowService } from "../../services";
import { AppEvents } from "../../shared/events";
import { escapeHtml, on } from "../../shared/dom";

export interface ChildPageProps {
  t: Translate;
  kind: ChildWindowType;
  id: string;
  message?: string;
  onError: (error: unknown) => void;
}

export const mountChildPage = (container: HTMLElement, props: ChildPageProps): Cleanup => {
  let disposed = false;
  let busy = false;
  let unlistenSend: Unlisten = () => undefined;
  let unlistenBroadcast: Unlisten = () => undefined;
  const title = props.kind === "confirm" ? props.t("child.confirmTitle") : props.kind === "message" ? props.t("child.messageTitle") : props.t("child.blankTitle");
  const body = props.kind === "confirm" ? props.message ?? props.t("child.confirmMessage") : props.kind === "message" ? props.message ?? props.t("child.messageBody") : props.t("child.blankBody");
  container.innerHTML = `<main class="child-window" data-scroll-container="child"><div class="child-window__bar"><span class="title-bar__mark" aria-hidden="true">F</span><strong>${escapeHtml(title)}</strong></div><section class="child-window__content"><span class="eyebrow">${escapeHtml(props.kind.toUpperCase())}</span><h1>${escapeHtml(title)}</h1><p data-child-body>${escapeHtml(body)}</p><div class="button-row">${props.kind === "confirm" ? `<button class="button button--primary" data-result="true">${props.t("child.confirmAction")}</button>` : ""}${props.kind === "message" ? `<button class="button button--primary" data-result="message">${props.t("child.send")}</button>` : ""}<button class="button" data-result="null">${props.t("child.close")}</button></div></section></main>`;

  const setBody = (payload: ChildMessagePayload): void => {
    const bodyElement = container.querySelector<HTMLElement>("[data-child-body]");
    if (!bodyElement || disposed) {
      return;
    }
    const value = typeof payload.data === "string" ? payload.data : JSON.stringify(payload.data);
    if (value) {
      bodyElement.textContent = value;
    }
  };

  const cleanupClick = on(container, "click", (event) => {
    const target = event.target;
    if (!(target instanceof HTMLElement) || disposed || busy) {
      return;
    }
    const result = target.closest<HTMLElement>("[data-result]")?.dataset.result;
    if (!result) {
      return;
    }
    busy = true;
    container.querySelectorAll<HTMLButtonElement>("[data-result]").forEach((button) => { button.disabled = true; });
    const value: boolean | string | null = result === "true" ? true : result === "message" ? props.t("child.send") : null;
    const notifyParent = ChildWindowService.close(props.id, value).catch((error: unknown) => {
      if (!disposed) {
        props.onError(error);
      }
    });
    // Close visually even if IPC is delayed by a capability/runtime race;
    // allow the result command a short window to reach the main process first.
    void Promise.race([
      notifyParent,
      new Promise<void>((resolve) => window.setTimeout(resolve, 300)),
    ]).then(() => WindowService.close().catch(props.onError));
  });

  safeListen<ChildMessagePayload>(AppEvents.childSend(props.id), (payload) => setBody(payload)).then((unlisten) => {
    if (disposed) {
      unlisten();
    } else {
      unlistenSend = unlisten;
    }
  }).catch((error: unknown) => { if (!disposed) props.onError(error); });
  safeListen<ChildMessagePayload>(AppEvents.childBroadcast, (payload) => setBody(payload)).then((unlisten) => {
    if (disposed) {
      unlisten();
    } else {
      unlistenBroadcast = unlisten;
    }
  }).catch((error: unknown) => { if (!disposed) props.onError(error); });

  return () => {
    disposed = true;
    cleanupClick();
    unlistenSend();
    unlistenBroadcast();
    container.replaceChildren();
  };
};
