import { useSyncExternalStore } from "react";

export const MASK = "••••";
const NAME_MASK = "••••••";
const KEY = "privacy-mask";

function read(): boolean {
  try {
    return localStorage.getItem(KEY) === "1";
  } catch {
    return false;
  }
}

let masked = read();
const listeners = new Set<() => void>();
const notify = () => listeners.forEach((l) => l());

if (typeof window !== "undefined") {
  window.addEventListener("storage", (e) => {
    if (e.key !== KEY) return;
    masked = read();
    notify();
  });
}

export const isMasked = () => masked;

export function setMasked(next: boolean) {
  masked = next;
  try {
    localStorage.setItem(KEY, next ? "1" : "0");
  } catch {
  }
  notify();
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function usePrivacy(): [boolean, (next: boolean) => void] {
  return [useSyncExternalStore(subscribe, isMasked), setMasked];
}

export function maskMoney(parts: Intl.NumberFormatPart[]): string {
  let out = "";
  let inNumber = false;
  for (const p of parts) {
    if (p.type === "minusSign" || p.type === "plusSign") continue;
    const numeric = p.type === "integer" || p.type === "group" || p.type === "decimal" || p.type === "fraction";
    if (numeric && !inNumber) out += MASK;
    if (!numeric) out += p.value;
    inNumber = numeric;
  }
  return out;
}

export function customer<T extends string | null | undefined>(name: T): T {
  return (masked && name ? NAME_MASK : name) as T;
}

const NUM = String.raw`[-+−]?\d(?:[\d.,\u00a0\u202f]*\d)?`;
const CUR = String.raw`(?:€|EUR|CHF|USD|GBP|\$|£)`;
const AMOUNT = new RegExp(String.raw`${NUM}[\s\u00a0]?${CUR}|${CUR}[\s\u00a0]?${NUM}`, "g");
const NUMBER = new RegExp(NUM);
const escape = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

let knownNames: string[] = [];

export function setCustomerNames(names: string[]) {
  knownNames = names;
}

export function maskText(text: string, names: string[] = knownNames): string {
  if (!masked) return text;
  let out = text.replace(AMOUNT, (m) => m.replace(NUMBER, MASK));
  const terms = names.map((n) => n.trim()).filter((n) => n.length >= 3);
  if (terms.length) {
    terms.sort((a, b) => b.length - a.length);
    out = out.replace(new RegExp(terms.map(escape).join("|"), "gi"), NAME_MASK);
  }
  return out;
}
