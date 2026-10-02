import type { LocaleChoice } from "../contracts/types";
import { enUS, type LocaleCode, type LocaleDictionary, zhCN } from "./locales";

const clone = (value: LocaleDictionary): LocaleDictionary => JSON.parse(JSON.stringify(value)) as LocaleDictionary;

const deepMerge = (base: LocaleDictionary, patch: LocaleDictionary): LocaleDictionary => {
  const output = clone(base);
  Object.entries(patch).forEach(([key, value]) => {
    if (typeof value === "object" && value !== null && !Array.isArray(value)) {
      const current = output[key];
      output[key] = deepMerge(
        typeof current === "object" && current !== null && !Array.isArray(current) ? (current as LocaleDictionary) : {},
        value as LocaleDictionary,
      );
    } else {
      output[key] = value;
    }
  });
  return output;
};

const getValue = (dictionary: LocaleDictionary, key: string): string | undefined => {
  const value = key.split(".").reduce<unknown>((current, segment) => {
    if (typeof current !== "object" || current === null) {
      return undefined;
    }
    return (current as Record<string, unknown>)[segment];
  }, dictionary);
  return typeof value === "string" ? value : undefined;
};

const resolveLocale = (choice: LocaleChoice): LocaleCode => {
  if (choice !== "auto") {
    return choice;
  }
  const languages = typeof navigator === "undefined" ? [] : navigator.languages;
  return languages.some((language) => language.toLowerCase().startsWith("zh")) ? "zh-CN" : "en-US";
};

export type Translate = (key: string, vars?: Record<string, string | number>) => string;

export class I18n {
  private localeChoice: LocaleChoice = "auto";
  private locale: LocaleCode = resolveLocale(this.localeChoice);
  private dictionaries: Record<LocaleCode, LocaleDictionary> = {
    "zh-CN": clone(zhCN),
    "en-US": clone(enUS),
  };
  private listeners = new Set<() => void>();

  register(locale: LocaleCode, dictionary: LocaleDictionary): void {
    this.dictionaries[locale] = deepMerge(this.dictionaries[locale], dictionary);
  }

  setChoice(choice: LocaleChoice): void {
    const nextLocale = resolveLocale(choice);
    if (this.localeChoice === choice && this.locale === nextLocale) {
      return;
    }
    this.localeChoice = choice;
    this.locale = nextLocale;
    this.notify();
  }

  getChoice(): LocaleChoice {
    return this.localeChoice;
  }

  getLocale(): LocaleCode {
    return this.locale;
  }

  t: Translate = (key, vars) => {
    const current = getValue(this.dictionaries[this.locale], key);
    const fallback = getValue(this.dictionaries["en-US"], key);
    const template = current ?? fallback ?? key;
    return template.replace(/\{\{?([\w.-]+)\}?\}/g, (_match, variable: string) => {
      const value = vars?.[variable];
      return value === undefined ? `{{${variable}}}` : String(value);
    });
  };

  subscribe(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  private notify(): void {
    this.listeners.forEach((listener) => listener());
  }
}

export const i18n = new I18n();
export { deepMerge };
