import { createContext, type ReactNode, useContext, useEffect, useMemo, useState } from "react";
import { en, type Messages } from "./en";
import { it } from "./it";

export const LOCALES = { en: "English", it: "Italiano" } as const;
export type Locale = keyof typeof LOCALES;

const dictionaries: Record<Locale, Messages> = { en, it };
const STORAGE_KEY = "werd.locale";

function isLocale(value: string | null): value is Locale {
  return value !== null && value in LOCALES;
}

/** Saved choice, then the system language, then English. */
export function initialLocale(): Locale {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (isLocale(saved)) return saved;
  } catch {
    // Storage can be unavailable (private mode, tests).
  }
  const system = navigator.language.slice(0, 2);
  return isLocale(system) ? system : "en";
}

interface I18n {
  locale: Locale;
  setLocale: (locale: Locale) => void;
  t: Messages;
}

const I18nContext = createContext<I18n>({ locale: "en", setLocale: () => {}, t: en });

export function I18nProvider({ children, locale: fixed }: { children: ReactNode; locale?: Locale }) {
  const [locale, setLocaleState] = useState<Locale>(() => fixed ?? initialLocale());
  useEffect(() => {
    document.documentElement.lang = locale;
  }, [locale]);
  const value = useMemo<I18n>(
    () => ({
      locale,
      t: dictionaries[locale],
      setLocale: (next) => {
        setLocaleState(next);
        try {
          localStorage.setItem(STORAGE_KEY, next);
        } catch {
          // Keep working without persistence.
        }
      },
    }),
    [locale],
  );
  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export const useI18n = () => useContext(I18nContext);
export const useT = () => useContext(I18nContext).t;
