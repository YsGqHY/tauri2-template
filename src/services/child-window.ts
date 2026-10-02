import type { ChildClosedPayload, ChildEventPayload, ChildMessagePayload, ChildWindow, ChildWindowOptions, ChildWindowType, Cleanup } from "../contracts/types";
import { listenEvent, type Unlisten } from "../api/events";
import { invokeCommand } from "../api/tauri";

export interface ChildOpenHandlers {
  onResult?: (payload: ChildEventPayload) => void;
  onClosed?: (payload: ChildClosedPayload) => void;
  onSend?: (payload: ChildMessagePayload) => void;
  onBroadcast?: (payload: ChildMessagePayload) => void;
}

export interface ChildOpenResult {
  id: string;
  window: ChildWindow;
  dispose: Cleanup;
}

export interface ChildOpenInput {
  title: string;
  message?: string;
  width?: number;
  height?: number;
}

const makeId = (): string => {
  const random = typeof crypto !== "undefined" && typeof crypto.randomUUID === "function" ? crypto.randomUUID() : `${Date.now()}-${Math.random().toString(36).slice(2)}`;
  return random.replace(/[^a-zA-Z0-9_-]/g, "").slice(0, 64);
};

const eventName = (prefix: string, id: string): string => `${prefix}:${id}`;

export const ChildWindowService = {
  async open(kind: ChildWindowType, input: ChildOpenInput, handlers: ChildOpenHandlers = {}): Promise<ChildOpenResult> {
    const id = makeId();
    let disposed = false;
    const subscriptions: Unlisten[] = [];
    const subscribe = async <T>(event: string, handler: ((payload: T) => void) | undefined): Promise<void> => {
      const unlisten = await listenEvent<T>(event, (payload) => {
        if (!disposed) {
          handler?.(payload);
        }
      });
      if (disposed) {
        unlisten();
      } else {
        subscriptions.push(unlisten);
      }
    };

    try {
      await Promise.all([
        subscribe<ChildEventPayload>(eventName("child:result", id), handlers.onResult),
        subscribe<ChildClosedPayload>(eventName("child:closed", id), handlers.onClosed),
        subscribe<ChildMessagePayload>(eventName("child:send", id), handlers.onSend),
        subscribe<ChildMessagePayload>("child:broadcast", handlers.onBroadcast),
      ]);
      const options: ChildWindowOptions = { id, kind, title: input.title, message: input.message, width: input.width, height: input.height };
      const window = await invokeCommand<ChildWindow>("open_child_window", { options });
      return {
        id,
        window,
        dispose: () => {
          if (disposed) {
            return;
          }
          disposed = true;
          subscriptions.splice(0).forEach((unlisten) => unlisten());
        },
      };
    } catch (error: unknown) {
      disposed = true;
      subscriptions.splice(0).forEach((unlisten) => unlisten());
      throw error;
    }
  },
  close(id: string, result?: unknown): Promise<void> {
    return invokeCommand<void>("close_child_window", { id, result });
  },
  list(): Promise<ChildWindow[]> {
    return invokeCommand<ChildWindow[]>("list_child_windows");
  },
  send(targetId: string, data: unknown): Promise<void> {
    return invokeCommand<void>("send_child_message", { targetId, data });
  },
  broadcast(data: unknown): Promise<void> {
    return invokeCommand<void>("broadcast_child_message", { data });
  },
};
