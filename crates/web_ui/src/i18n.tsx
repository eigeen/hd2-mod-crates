import { createContext, useCallback, useContext, useEffect, useMemo, useState } from "react";
import type { ReactNode } from "react";
import { resources, SUPPORTED_LANGUAGES, type LanguageCode } from "./locales";
import type { TranslationKey } from "./locales/translationKeys";

const LANGUAGE_STORAGE_KEY = "hd2-migrator-language";

type TranslationValues = Record<string, string | number>;
export type Translate = (key: TranslationKey, values?: TranslationValues) => string;
export type { LanguageCode };

interface I18nContextValue {
  language: LanguageCode;
  languages: readonly LanguageCode[];
  setLanguage: (language: LanguageCode) => void;
  t: Translate;
}

const I18nContext = createContext<I18nContextValue | null>(null);

export function I18nProvider({ children }: { children: ReactNode }) {
  const [language, setLanguageState] = useState<LanguageCode>(detectInitialLanguage);

  const setLanguage = useCallback((nextLanguage: LanguageCode) => {
    setLanguageState(nextLanguage);
    window.localStorage.setItem(LANGUAGE_STORAGE_KEY, nextLanguage);
  }, []);

  useEffect(() => {
    document.documentElement.lang = language;
  }, [language]);

  const t = useCallback<Translate>(
    (key, values) => interpolate(resources[language][key], values),
    [language],
  );

  const value = useMemo(
    () => ({ language, languages: SUPPORTED_LANGUAGES, setLanguage, t }),
    [language, setLanguage, t],
  );

  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export function useI18n() {
  const value = useContext(I18nContext);
  if (!value) {
    throw new Error("useI18n must be used within I18nProvider.");
  }
  return value;
}

function detectInitialLanguage(): LanguageCode {
  const stored = normalizeLanguage(window.localStorage.getItem(LANGUAGE_STORAGE_KEY));
  if (stored) {
    return stored;
  }
  return detectBrowserLanguage(navigator.languages);
}

function detectBrowserLanguage(languages: readonly string[]): LanguageCode {
  for (const language of languages) {
    const normalized = normalizeLanguage(language);
    if (normalized) {
      return normalized;
    }
  }
  return "en";
}

function normalizeLanguage(language: string | null): LanguageCode | null {
  if (!language) {
    return null;
  }
  const lower = language.toLowerCase();
  if (lower === "zh-cn" || lower === "zh-hans" || lower.startsWith("zh")) {
    return "zh-CN";
  }
  return lower.startsWith("en") ? "en" : null;
}

function interpolate(template: string, values?: TranslationValues): string {
  if (!values) {
    return template;
  }
  return template.replace(/\{\{(\w+)\}\}/g, (_, key: string) => String(values[key] ?? ""));
}
