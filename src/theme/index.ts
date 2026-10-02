import type { CustomTheme, ThemeChoice, ThemeMode, ThemeToken } from "../contracts/types";

export type ThemePalette = Record<ThemeToken, string>;

export const themeTokens: ThemeToken[] = [
  "bg", "surface", "surfaceRaised", "surfaceMuted", "text", "textMuted", "textSubtle", "border",
  "accent", "accentStrong", "accentSoft", "success", "warning", "danger", "info", "focus", "sidebar",
];

export const lightPalette: ThemePalette = {
  bg: "#f5f7fb", surface: "#ffffff", surfaceRaised: "#ffffff", surfaceMuted: "#eef2f8", text: "#172033", textMuted: "#5d6a7d", textSubtle: "#8490a3", border: "#dbe2ec", accent: "#2f6fed", accentStrong: "#1d4ed8", accentSoft: "#e5edff", success: "#1e9b6b", warning: "#c78318", danger: "#d64a58", info: "#3d83c6", focus: "#83a9ff", sidebar: "#ffffff",
};

export const darkPalette: ThemePalette = {
  bg: "#141822", surface: "#1d2330", surfaceRaised: "#252d3c", surfaceMuted: "#242b38", text: "#eff4ff", textMuted: "#a5b0c3", textSubtle: "#738097", border: "#374156", accent: "#77a5ff", accentStrong: "#9bbaff", accentSoft: "#253b69", success: "#5bd5a2", warning: "#f1be62", danger: "#ff7b87", info: "#75b8ef", focus: "#9bbaff", sidebar: "#1a202c",
};

export const obsidianPalette: ThemePalette = {
  bg: "#0a0a10", surface: "#11111b", surfaceRaised: "#191925", surfaceMuted: "#1b1b2a", text: "#f4f3ff", textMuted: "#b4b0ca", textSubtle: "#7d789a", border: "#302c4a", accent: "#9b8cff", accentStrong: "#b7adff", accentSoft: "#292445", success: "#68d5ac", warning: "#efbc67", danger: "#ff7e93", info: "#80bfff", focus: "#b7adff", sidebar: "#0f0f18",
};

let mediaCleanup: (() => void) | null = null;

export const isValidHexColor = (value: string): boolean => /^#(?:[\da-f]{3}|[\da-f]{6}|[\da-f]{8})$/i.test(value.trim());

export const validateCustomTheme = (theme: CustomTheme | null | undefined): theme is CustomTheme => {
  if (!theme || typeof theme !== "object" || typeof theme.palette !== "object" || theme.palette === null) {
    return false;
  }
  return Object.values(theme.palette).every((value) => typeof value === "string" && isValidHexColor(value));
};

const paletteForMode = (mode: ThemeMode): ThemePalette => mode === "dark" ? darkPalette : lightPalette;

export const resolvePalette = (choice: ThemeChoice, customTheme?: CustomTheme | null): ThemePalette => {
  if (choice === "dark") {
    return darkPalette;
  }
  if (choice === "obsidian") {
    return obsidianPalette;
  }
  if (choice === "custom" && validateCustomTheme(customTheme)) {
    return { ...paletteForMode(customTheme.mode ?? "light"), ...customTheme.palette };
  }
  if (choice === "system") {
    const dark = typeof window !== "undefined" && window.matchMedia?.("(prefers-color-scheme: dark)").matches;
    return dark ? darkPalette : lightPalette;
  }
  return lightPalette;
};

export const applyTheme = (choice: ThemeChoice, customTheme?: CustomTheme | null): ThemePalette => {
  mediaCleanup?.();
  mediaCleanup = null;
  const palette = resolvePalette(choice, customTheme);
  const root = document.documentElement;
  root.dataset.theme = choice;
  themeTokens.forEach((token) => root.style.setProperty(`--color-${token}`, palette[token]));
  const isDark = choice === "dark" || choice === "obsidian" || (choice === "custom" && customTheme?.mode === "dark") || (choice === "system" && palette === darkPalette);
  root.style.colorScheme = isDark ? "dark" : "light";
  if (choice === "system" && typeof window !== "undefined") {
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const handler = (): void => { applyTheme(choice, customTheme); };
    media.addEventListener?.("change", handler);
    mediaCleanup = () => media.removeEventListener?.("change", handler);
  }
  return palette;
};

export const disposeTheme = (): void => {
  mediaCleanup?.();
  mediaCleanup = null;
};
