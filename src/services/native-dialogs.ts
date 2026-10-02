import { confirm, message, open, save } from "@tauri-apps/plugin-dialog";
import { AppError, isTauriRuntime } from "../api/tauri";

export interface DialogFilter {
  name: string;
  extensions: string[];
}

export interface ConfirmOptions {
  title: string;
  message: string;
  okLabel: string;
  cancelLabel: string;
}

const requireRuntime = (): void => {
  if (!isTauriRuntime()) {
    throw new AppError({ code: "DIALOG_RUNTIME_REQUIRED", message: "" });
  }
};

export const NativeDialogs = {
  async openFile(filters?: DialogFilter[]): Promise<string | null> {
    requireRuntime();
    const selected = await open({ multiple: false, directory: false, filters });
    return typeof selected === "string" ? selected : null;
  },
  async openFiles(filters?: DialogFilter[]): Promise<string[]> {
    requireRuntime();
    const selected = await open({ multiple: true, directory: false, filters });
    if (Array.isArray(selected)) {
      return selected.filter((path): path is string => typeof path === "string");
    }
    return typeof selected === "string" ? [selected] : [];
  },
  async openDirectory(): Promise<string | null> {
    requireRuntime();
    const selected = await open({ multiple: false, directory: true });
    return typeof selected === "string" ? selected : null;
  },
  async saveFile(filters?: DialogFilter[], defaultPath?: string): Promise<string | null> {
    requireRuntime();
    const selected = await save({ filters, defaultPath });
    return typeof selected === "string" ? selected : null;
  },
  async confirm(options: ConfirmOptions): Promise<boolean> {
    requireRuntime();
    return confirm(options.message, {
      title: options.title,
      kind: "warning",
      okLabel: options.okLabel,
      cancelLabel: options.cancelLabel,
    });
  },
  async info(title: string, body: string): Promise<void> {
    requireRuntime();
    await message(body, { title, kind: "info" });
  },
  async warning(title: string, body: string): Promise<void> {
    requireRuntime();
    await message(body, { title, kind: "warning" });
  },
  async error(title: string, body: string): Promise<void> {
    requireRuntime();
    await message(body, { title, kind: "error" });
  },
};
