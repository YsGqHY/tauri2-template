import { invoke } from "@tauri-apps/api/core";
import type { ErrorPayload } from "../contracts/types";
import { i18n, type Translate } from "../i18n";

export class AppError extends Error {
  readonly payload: ErrorPayload;

  constructor(payload: ErrorPayload) {
    super(payload.message);
    this.name = "AppError";
    this.payload = payload;
  }
}

interface TauriWindow extends Window {
  __TAURI__?: unknown;
  __TAURI_INTERNALS__?: unknown;
}

export const isTauriRuntime = (): boolean => {
  if (typeof window === "undefined") {
    return false;
  }
  const candidate = window as TauriWindow;
  return candidate.__TAURI__ !== undefined || candidate.__TAURI_INTERNALS__ !== undefined;
};

const readRecord = (error: unknown): Record<string, unknown> | null => {
  if (typeof error !== "object" || error === null) {
    return null;
  }
  return error as Record<string, unknown>;
};

const toErrorPayload = (error: unknown, command?: string): ErrorPayload => {
  const parsedString = typeof error === "string" && error.trim().startsWith("{") ? (() => {
    try {
      return JSON.parse(error) as unknown;
    } catch {
      return null;
    }
  })() : null;
  const record = readRecord(parsedString ?? error);
  if (record) {
    const code = typeof record.code === "string" ? record.code : undefined;
    const message = typeof record.message === "string" ? record.message : undefined;
    const detail = typeof record.detail === "string" ? record.detail : undefined;
    const nextStep = typeof record.nextStep === "string" ? record.nextStep : undefined;
    if (message || code || detail) {
      return { code, message: message ?? "", detail, nextStep };
    }
  }
  if (typeof error === "string") {
    return { message: error };
  }
  return {
    code: command,
    message: "",
    detail: error === undefined ? undefined : String(error),
  };
};

export async function invokeCommand<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauriRuntime()) {
    throw new AppError({ code: "TAURI_RUNTIME_REQUIRED", message: "" });
  }
  try {
    return await invoke<T>(command, args);
  } catch (error: unknown) {
    throw new AppError(toErrorPayload(error, command));
  }
}

export const toDisplayError = (error: unknown, translate: Translate = i18n.t): ErrorPayload => {
  const payload = error instanceof AppError ? error.payload : toErrorPayload(error);
  const code = payload.code ?? "UNKNOWN";
  const message = payload.message || translate(`errors.${code}`) || translate("errors.unknown");
  const nextStep = payload.nextStep || (code === "TAURI_RUNTIME_REQUIRED" ? translate("errors.runtimeNext") : translate("errors.next"));
  return { ...payload, message, nextStep };
};
