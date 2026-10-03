import type { CustomTheme, CustomThemeSnapshot, ThemeChoice, ThemeMode, ThemeToken } from "../contracts/types";

export type ThemePalette = Record<ThemeToken, string>;

export const themeTokens: ThemeToken[] = [
  "bg", "surface", "surfaceRaised", "surfaceMuted", "text", "textMuted", "textSubtle", "border",
  "accent", "accentStrong", "accentSoft", "success", "warning", "danger", "info", "focus", "sidebar",
];

export const lightPalette: ThemePalette = {
  bg: "#ffffff",
  surface: "#ffffff",
  surfaceRaised: "#eef1f6",
  surfaceMuted: "rgba(15, 23, 42, 0.08)",
  text: "#0b1220",
  textMuted: "#3f4a5a",
  textSubtle: "#8a93a4",
  border: "rgba(15, 23, 42, 0.10)",
  accent: "#2563eb",
  accentStrong: "#1d4ed8",
  accentSoft: "rgba(37, 99, 235, 0.14)",
  success: "#16a34a",
  warning: "#d97706",
  danger: "#dc2626",
  info: "#2563eb",
  focus: "#1d4ed8",
  sidebar: "#f6f7fa",
};

export const darkPalette: ThemePalette = {
  bg: "#0b0d10",
  surface: "#22262d",
  surfaceRaised: "#2a2f37",
  surfaceMuted: "rgba(255, 255, 255, 0.06)",
  text: "#f8fafc",
  textMuted: "#cbd5e1",
  textSubtle: "#64748b",
  border: "rgba(255, 255, 255, 0.08)",
  accent: "#60a5fa",
  accentStrong: "#93c5fd",
  accentSoft: "rgba(96, 165, 250, 0.16)",
  success: "#4ade80",
  warning: "#fbbf24",
  danger: "#f87171",
  info: "#60a5fa",
  focus: "#93c5fd",
  sidebar: "#15181d",
};

export const obsidianPalette: ThemePalette = {
  bg: "#000000",
  surface: "#16161b",
  surfaceRaised: "#1c1c22",
  surfaceMuted: "rgba(167, 139, 250, 0.08)",
  text: "#f5f5f7",
  textMuted: "#c4c4cc",
  textSubtle: "#6b6b78",
  border: "rgba(255, 255, 255, 0.06)",
  accent: "#a78bfa",
  accentStrong: "#c4b5fd",
  accentSoft: "rgba(167, 139, 250, 0.18)",
  success: "#4ade80",
  warning: "#fbbf24",
  danger: "#f87171",
  info: "#a78bfa",
  focus: "#c4b5fd",
  sidebar: "#0a0a0c",
};

let mediaCleanup: (() => void) | null = null;

const themeChoiceAliases: Record<string, ThemeChoice> = {
  system: "system",
  light: "light",
  dark: "dark",
  obsidian: "obsidian",
  custom: "custom",
  "foundation-light": "light",
  "foundation-dark": "dark",
  "foundation-obsidian": "obsidian",
  "foundation-custom": "custom",
};

const cssColorFunctionNames = ["rgb", "rgba", "hsl", "hsla", "hwb", "lab", "lch", "oklab", "oklch", "color"];

export const isValidHexColor = (value: string): boolean => /^(#(?:[\da-f]{3}|[\da-f]{4}|[\da-f]{6}|[\da-f]{8}))$/i.test(value.trim());

export const isValidCssColor = (value: string): boolean => {
  const normalized = value.trim();
  if (isValidHexColor(normalized) || normalized.toLowerCase() === "transparent") {
    return true;
  }
  const match = normalized.match(/^([a-z]+)\((.*)\)$/i);
  if (!match || !cssColorFunctionNames.includes(match[1].toLowerCase())) {
    return false;
  }
  return match[2].length > 0 && !/[;{}<>"'`]/.test(match[2]) && /^[\w\s.,%+\-\/()]+$/.test(match[2]);
};

export const normalizeThemeChoice = (choice: string): ThemeChoice | null => themeChoiceAliases[choice.trim()] ?? null;

export const validateCustomTheme = (theme: CustomTheme | null | undefined): theme is CustomThemeSnapshot => {
  if (!theme || typeof theme !== "object" || typeof theme.palette !== "object" || theme.palette === null) {
    return false;
  }
  const mode = theme.mode ?? "light";
  const name = theme.name?.trim() || "Foundation Custom";
  const palette = mergePalette(mode, theme.palette);
  return name.length <= 64 && Object.keys(theme.palette).every((key) => themeTokens.includes(key as ThemeToken)) && themeTokens.every((key) => isValidCssColor(palette[key]));
};

export const normalizeCustomTheme = (theme: CustomTheme | null | undefined): CustomThemeSnapshot | null => {
  if (!validateCustomTheme(theme)) {
    return null;
  }
  return {
    name: theme.name?.trim() || "Foundation Custom",
    mode: theme.mode ?? "light",
    palette: mergePalette(theme.mode ?? "light", theme.palette),
  };
};

const paletteForMode = (mode: ThemeMode): ThemePalette => mode === "dark" ? darkPalette : lightPalette;

export const mergePalette = (mode: ThemeMode, palette: Partial<Record<ThemeToken, string>> | undefined): ThemePalette => ({
  ...paletteForMode(mode),
  ...(palette ?? {}),
});

export const seedCustomTheme = (choice: ThemeChoice, existing?: CustomTheme | null): CustomThemeSnapshot => {
  const normalized = normalizeCustomTheme(existing);
  if (normalized) {
    return normalized;
  }
  const canonical = normalizeThemeChoice(choice) ?? "light";
  const sourcePalette = resolvePalette(canonical, existing);
  const mode: ThemeMode = canonical === "dark" || canonical === "obsidian" ? "dark" : sourcePalette === darkPalette ? "dark" : "light";
  return {
    name: "Foundation Custom",
    mode,
    palette: { ...sourcePalette },
  };
};


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
