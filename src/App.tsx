import { useCallback, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import {
  ArrowLeftRight,
  BarChart3,
  FileText,
  FolderLock,
  LayoutDashboard,
  Search,
  Settings as SettingsIcon,
  Timer,
  Users,
} from "lucide-react";
import { api, type Entry, type Settings } from "./lib/api";
import { clock } from "./lib/format";
import { useTick } from "./lib/hooks";
import { AppContext, TIMER_EVENT, useMenuBarTimerChanged, type Route, type RouteParams, type Theme } from "./lib/nav";
import { cx } from "./components/ui";
import { CommandPalette } from "./components/CommandPalette";
import { NotificationsButton, NotificationsPanel, useNotifications } from "./components/Notifications";
import Overview from "./pages/Overview";
import Ledger from "./pages/Ledger";
import Categories from "./pages/Categories";
import Invoices from "./pages/Invoices";
import Customers from "./pages/Customers";
import Tracker from "./pages/Tracker";
import Vault from "./pages/Vault";
import Reports from "./pages/Reports";
import SettingsPage from "./pages/Settings";
import Onboarding from "./pages/Onboarding";
import { isLang, lang, setLang, t } from "./lib/i18n";
import { dismissSplash } from "./lib/splash";

const NAV: { route: Route; label: string; icon: ReactNode }[] = [
  { route: "overview", label: t("Overview"), icon: <LayoutDashboard className="size-4" /> },
  { route: "invoices", label: t("Invoices"), icon: <FileText className="size-4" /> },
  { route: "ledger", label: t("Income & expenses"), icon: <ArrowLeftRight className="size-4" /> },
  { route: "customers", label: t("Customers"), icon: <Users className="size-4" /> },
  { route: "tracker", label: t("Time tracking"), icon: <Timer className="size-4" /> },
  { route: "vault", label: "Vault", icon: <FolderLock className="size-4" /> },
  { route: "reports", label: t("Reports"), icon: <BarChart3 className="size-4" /> },
];

function readTheme(): Theme {
  try {
    return (localStorage.getItem("theme") as Theme) || "system";
  } catch {
    return "system";
  }
}

const DARK_QUERY = "(prefers-color-scheme: dark)";

function SunMark() {
  return (
    <svg viewBox="0 0 24 24" className="size-5" aria-hidden>
      <path d="M5 15a7 7 0 0 1 14 0z" fill="currentColor" />
      <rect x="3" y="16.5" width="18" height="1.6" rx="0.8" fill="currentColor" />
    </svg>
  );
}

function RunningTimerChip({ onOpen }: { onOpen: () => void }) {
  const [entry, setEntry] = useState<Entry | null>(null);
  const load = useCallback(() => {
    api.runningTimer().then(setEntry).catch(() => setEntry(null));
  }, []);
  useEffect(() => {
    load();
    const t = setInterval(load, 15000);
    window.addEventListener(TIMER_EVENT, load);
    return () => {
      clearInterval(t);
      window.removeEventListener(TIMER_EVENT, load);
    };
  }, [load]);
  useMenuBarTimerChanged(load);
  const now = useTick(1000, entry !== null);
  if (!entry) return null;
  const secs = Math.max(0, (now - new Date(entry.start).getTime()) / 1000);
  return (
    <button type="button" onClick={onOpen} className="mx-2 mb-2 flex items-center gap-2 rounded-md border border-line bg-subtle px-2.5 py-2 text-left">
      <span className="size-2 animate-pulse rounded-full bg-neg" />
      <span className="min-w-0 flex-1">
        <span className="block truncate text-xs">{entry.project_name}</span>
        <span className="tabular block text-[11px] text-muted">{clock(secs)}</span>
      </span>
    </button>
  );
}

export default function App() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [route, setRoute] = useState<Route>("overview");
  const [params, setParams] = useState<RouteParams>({});
  const [visit, setVisit] = useState(0);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [theme, setTheme] = useState<Theme>(readTheme);
  const [systemDark, setSystemDark] = useState(() => window.matchMedia(DARK_QUERY).matches);
  const mainRef = useRef<HTMLElement>(null);
  const notifications = useNotifications();
  const dark = theme === "dark" || (theme === "system" && systemDark);

  const reloadSettings = useCallback(async () => {
    try {
      const loaded = await api.getSettings();
      if (loaded.onboarded && isLang(loaded.language) && loaded.language !== lang()) return setLang(loaded.language);
      setSettings(loaded);
    } catch (e) {
      setLoadError(String(e));
    }
  }, []);

  useEffect(() => {
    reloadSettings();
  }, [reloadSettings]);

  useEffect(() => {
    if (settings || loadError) dismissSplash();
  }, [settings, loadError]);

  useEffect(() => {
    try {
      localStorage.setItem("theme", theme);
    } catch {
    }
  }, [theme]);

  useEffect(() => {
    document.documentElement.lang = lang();
  }, []);

  useLayoutEffect(() => {
    const root = document.documentElement;
    if (root.classList.contains("dark") === dark) return;
    root.classList.add("theme-switching");
    root.classList.toggle("dark", dark);
    const frame = requestAnimationFrame(() => requestAnimationFrame(() => root.classList.remove("theme-switching")));
    return () => {
      cancelAnimationFrame(frame);
      root.classList.remove("theme-switching");
    };
  }, [dark]);

  useLayoutEffect(() => {
    mainRef.current?.scrollTo(0, 0);
  }, [visit]);

  useEffect(() => {
    const mq = window.matchMedia(DARK_QUERY);
    const onChange = () => setSystemDark(mq.matches);
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setPaletteOpen((o) => !o);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const go = useCallback((r: Route, p: RouteParams = {}) => {
    setRoute(r);
    setParams(p);
    setVisit((v) => v + 1);
  }, []);

  if (loadError) {
    return <div className="p-8 text-neg" data-selectable>{t("The database could not be loaded:")} {loadError}</div>;
  }
  if (!settings) return null;
  if (!settings.onboarded) return <Onboarding settings={settings} onDone={reloadSettings} />;

  const page = (() => {
    switch (route) {
      case "overview":
        return <Overview />;
      case "ledger":
        return <Ledger />;
      case "categories":
        return <Categories />;
      case "invoices":
        return <Invoices />;
      case "customers":
        return <Customers />;
      case "tracker":
        return <Tracker />;
      case "vault":
        return <Vault />;
      case "reports":
        return <Reports />;
      case "settings":
        return <SettingsPage />;
    }
  })();

  return (
    <AppContext.Provider value={{ route, params, go, settings, reloadSettings, theme, setTheme }}>
      <div className="flex h-full">
        <aside className="flex w-[216px] shrink-0 flex-col border-r border-line bg-panel">
          <div className="flex h-12 items-center gap-2 px-4" data-tauri-drag-region>
            <SunMark />
            <span className="text-[14px] font-semibold tracking-tight">Midnight</span>
          </div>
          <nav className="flex flex-1 flex-col gap-0.5 px-2 pt-2" aria-label={t("Main navigation")}>
            {NAV.map((n) => {
              const active = route === n.route || (n.route === "ledger" && route === "categories");
              return (
                <button
                  key={n.route}
                  type="button"
                  onClick={() => go(n.route)}
                  aria-current={active ? "page" : undefined}
                  className={cx(
                    "flex h-8 items-center gap-2.5 rounded-md px-2.5 text-[13px]",
                    active ? "bg-subtle font-medium text-fg" : "text-muted hover:bg-subtle hover:text-fg",
                  )}
                >
                  {n.icon}
                  {n.label}
                </button>
              );
            })}
          </nav>
          <RunningTimerChip onOpen={() => go("tracker")} />
          <div className="border-t border-line p-2">
            <button
              type="button"
              onClick={() => go("settings")}
              className={cx(
                "flex h-8 w-full items-center gap-2.5 rounded-md px-2.5 text-[13px]",
                route === "settings" ? "bg-subtle font-medium" : "text-muted hover:bg-subtle hover:text-fg",
              )}
            >
              <SettingsIcon className="size-4" />
              {t("Settings")}
            </button>
          </div>
        </aside>

        <div className="flex min-w-0 flex-1 flex-col">
          <header className="flex h-12 shrink-0 items-center gap-2 border-b border-line bg-panel px-4" data-tauri-drag-region>
            <button
              type="button"
              onClick={() => setPaletteOpen(true)}
              className="flex h-8 w-full max-w-md items-center gap-2 rounded-md border border-line bg-bg px-2.5 text-[13px] text-muted hover:bg-subtle"
            >
              <Search className="size-3.5" />
              {t("Search or run an action…")}
              <kbd className="ml-auto rounded border border-line px-1 text-[11px]">⌘K</kbd>
            </button>
            <div className="ml-auto flex items-center gap-1">
              <NotificationsButton state={notifications} />
            </div>
          </header>
          <main ref={mainRef} className="min-h-0 flex-1 overflow-y-scroll">
            <div key={visit} className="anim-page mx-auto w-full max-w-[1240px] px-6 py-6">
              {page}
            </div>
          </main>
        </div>
      </div>
      <NotificationsPanel state={notifications} />
      <CommandPalette open={paletteOpen} onClose={() => setPaletteOpen(false)} />
    </AppContext.Provider>
  );
}
