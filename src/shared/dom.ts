import type { Cleanup } from "../contracts/types";

export const escapeHtml = (value: string): string =>
  value.replace(/[&<>'"]/g, (character) => {
    const entities: Record<string, string> = {
      "&": "&amp;",
      "<": "&lt;",
      ">": "&gt;",
      "'": "&#39;",
      '"': "&quot;",
    };
    return entities[character] ?? character;
  });

export const on = <K extends keyof HTMLElementEventMap>(
  element: HTMLElement | Document,
  event: K,
  handler: (event: HTMLElementEventMap[K]) => void,
): Cleanup => {
  element.addEventListener(event, handler as EventListener);
  return () => element.removeEventListener(event, handler as EventListener);
};

export const qs = <T extends Element>(container: ParentNode, selector: string): T | null =>
  container.querySelector<T>(selector);

export const formatBytes = (value: number): string => {
  if (!Number.isFinite(value) || value <= 0) {
    return "0 B";
  }
  const units = ["B", "KB", "MB", "GB"];
  const index = Math.min(Math.floor(Math.log(value) / Math.log(1024)), units.length - 1);
  return `${(value / 1024 ** index).toFixed(index === 0 ? 0 : 1)} ${units[index]}`;
};
