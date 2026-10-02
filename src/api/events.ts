import { listen } from "@tauri-apps/api/event";
import { isTauriRuntime } from "./tauri";

export type Unlisten = () => void;
export type EventHandler<T> = (payload: T) => void;

export async function listenEvent<T>(event: string, handler: EventHandler<T>): Promise<Unlisten> {
  if (!isTauriRuntime()) {
    return () => undefined;
  }
  const unlisten = await listen<T>(event, (eventPayload) => handler(eventPayload.payload));
  return unlisten;
}

export async function safeListen<T>(event: string, handler: EventHandler<T>): Promise<Unlisten> {
  try {
    return await listenEvent(event, handler);
  } catch {
    return () => undefined;
  }
}
