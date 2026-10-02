import { getCurrentWindow } from "@tauri-apps/api/window";
import { invokeCommand, isTauriRuntime } from "../api/tauri";

export const WindowService = {
  async minimize(): Promise<void> {
    if (isTauriRuntime()) {
      await getCurrentWindow().minimize();
      return;
    }
    await invokeCommand<void>("window_minimize");
  },
  async toggleMaximize(): Promise<void> {
    if (isTauriRuntime()) {
      await getCurrentWindow().toggleMaximize();
      return;
    }
    await invokeCommand<void>("window_toggle_maximize");
  },
  async close(): Promise<void> {
    if (isTauriRuntime()) {
      await getCurrentWindow().close();
      return;
    }
    await invokeCommand<void>("window_close");
  },
};
