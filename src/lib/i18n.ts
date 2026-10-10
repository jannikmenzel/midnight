import { de } from "./i18n.de";

export type Lang = "de" | "en";

export const LANGUAGES: { value: Lang; label: string }[] = [
  { value: "en", label: "English" },
  { value: "de", label: "Deutsch" },
];

export type MessageKey = keyof typeof de;

const STORAGE_KEY = "language";

function detect(): Lang {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored === "de" || stored === "en") return stored;
  } catch {
  }
  return "en";
}

let current: Lang = detect();

export function lang(): Lang {
  return current;
}

export function locale(): string {
  return current === "de" ? "de-DE" : "en-GB";
}

export function isLang(value: unknown): value is Lang {
  return value === "de" || value === "en";
}

export function setLang(next: Lang) {
  if (next === current) return;
  try {
    localStorage.setItem(STORAGE_KEY, next);
  } catch {
    return;
  }
  current = next;
  window.location.reload();
}

export function t(key: MessageKey, vars?: Record<string, string | number>): string {
  const text: string = current === "de" ? de[key] : key;
  if (!vars) return text;
  return text.replace(/\{(\w+)\}/g, (match, name: string) => (name in vars ? String(vars[name]) : match));
}
