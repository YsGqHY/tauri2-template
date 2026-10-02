import type { TrayActionPayload } from "../contracts/types";
import { safeListen, type Unlisten } from "../api/events";

export const TrayService = {
  subscribe(handler: (payload: TrayActionPayload) => void): Promise<Unlisten> {
    return safeListen<TrayActionPayload>("tray:action", handler);
  },
};
