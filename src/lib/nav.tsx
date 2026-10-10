import { createContext, useContext, useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import type { Settings } from "./api";

export type Route =
  | "overview"
  | "ledger"
  | "categories"
  | "invoices"
  | "customers"
  | "tracker"
  | "vault"
  | "reports"
  | "settings";

export type RouteParams = Record<string, string>;

export type Theme = "system" | "light" | "dark";

interface AppContextValue {
  route: Route;
  params: RouteParams;
  go: (route: Route, params?: RouteParams) => void;
  settings: Settings;
  reloadSettings: () => Promise<void>;
  theme: Theme;
  setTheme: (theme: Theme) => void;
}

export const AppContext = createContext<AppContextValue | null>(null);

export function useApp(): AppContextValue {
  const ctx = useContext(AppContext);
  if (!ctx) throw new Error("useApp outside AppContext");
  return ctx;
}

export const TIMER_EVENT = "midnight:timer-changed";
export function notifyTimerChanged() {
  window.dispatchEvent(new Event(TIMER_EVENT));
}

export function useMenuBarTimerChanged(onChange: () => void) {
  const latest = useRef(onChange);
  latest.current = onChange;
  useEffect(() => {
    const unlisten = listen("tracker-changed", () => latest.current());
    return () => {
      unlisten.then((off) => off()).catch(() => {});
    };
  }, []);
}
