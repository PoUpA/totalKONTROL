// UI translations. The texts live in locales/<lang>.json, shared with the
// Rust core; English is the reference. The language is remembered per
// viewer, starts from the system language, and is sent to the Rust core so
// its messages match.

import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import en from "../../locales/en.json";
import esJson from "../../locales/es.json";
import frJson from "../../locales/fr.json";

export type MessageKey = keyof typeof en;
type Messages = Record<MessageKey, string>;

// Typed against the English keys: a missing translation fails the build.
const fr: Messages = frJson;
const es: Messages = esJson;

export type Lang = "en" | "fr" | "es";

/** Tells the Rust core which language its messages should use. */
export async function sendLangToCore(lang: Lang): Promise<void> {
  try {
    await invoke("set_locale", { lang });
  } catch {
    // Outside Tauri (plain browser preview): nothing to tell.
  }
}

/** Language names, each written in its own language. */
export const LANGUAGES: { code: Lang; label: string }[] = [
  { code: "en", label: "English" },
  { code: "fr", label: "Français" },
  { code: "es", label: "Español" },
];

const MESSAGES: Record<Lang, Messages> = { en, fr, es };
const STORAGE_KEY = "totalkontrol.lang";

export type Vars = Record<string, string | number>;
export type Translate = (key: MessageKey, vars?: Vars) => string;

/** Language of a tag such as "fr-CH"; English when unsupported. */
export function langOfTag(tag: string | undefined): Lang {
  const base = (tag ?? "").toLowerCase().split(/[-_]/)[0];
  return base === "fr" || base === "es" ? base : "en";
}

function initialLang(): Lang {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved === "en" || saved === "fr" || saved === "es") return saved;
  } catch {
    // Storage unavailable: fall back to the system language.
  }
  return langOfTag(navigator.language);
}

function format(template: string, vars?: Vars): string {
  if (!vars) return template;
  return template.replace(/\{(\w+)\}/g, (match, name: string) => (name in vars ? String(vars[name]) : match));
}

type I18n = {
  lang: Lang;
  setLang: (lang: Lang) => void;
  t: Translate;
  /** Plural form: looks up `${key}_one` or `${key}_other`, with {count} set. */
  tn: (key: string, count: number, vars?: Vars) => string;
  /** Locale tag for Intl APIs (dates, numbers). */
  locale: string;
};

const I18nContext = createContext<I18n | null>(null);

export function I18nProvider({ children }: { children: ReactNode }) {
  const [lang, setLangState] = useState<Lang>(initialLang);

  useEffect(() => {
    document.documentElement.lang = lang;
  }, [lang]);

  const setLang = useCallback((next: Lang) => {
    try {
      localStorage.setItem(STORAGE_KEY, next);
    } catch {
      // Remembering the choice is a convenience only.
    }
    setLangState(next);
  }, []);

  const value = useMemo<I18n>(() => {
    const messages = MESSAGES[lang];
    const t: Translate = (key, vars) => format(messages[key] ?? en[key] ?? key, vars);
    const plurals = new Intl.PluralRules(lang);
    const tn = (key: string, count: number, vars?: Vars) => {
      const form = plurals.select(count) === "one" ? "one" : "other";
      return t(`${key}_${form}` as MessageKey, { count, ...vars });
    };
    return { lang, setLang, t, tn, locale: lang };
  }, [lang, setLang]);

  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export function useI18n(): I18n {
  const i18n = useContext(I18nContext);
  if (!i18n) throw new Error("useI18n must be used inside I18nProvider");
  return i18n;
}
