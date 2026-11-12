import { useCallback, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
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
import { AppContext, TIMER_EVENT, UPDATE_EVENT, useMenuBarTimerChanged, type Route, type RouteParams, type Theme } from "./lib/nav";
import { cx } from "./components/ui";
import { CommandPalette } from "./components/CommandPalette";
import { NotificationsButton, NotificationsPanel, useNotifications } from "./components/Notifications";
import { UpdateHint, useUpdate } from "./components/Update";
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
import { keys } from "./lib/platform";
import { isMasked, setCustomerNames, usePrivacy } from "./lib/privacy";
import { LockScreen, PIN_CHANGED } from "./components/LockScreen";

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
    <button type="button" onClick={onOpen} className="raised mx-2 mb-2 flex items-center gap-2 rounded-md border border-line bg-panel px-2.5 py-2 text-left">
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
  const [locked, setLocked] = useState<boolean | null>(null);
  const pinOn = useRef(false);
  const [splashGone, setSplashGone] = useState(false);
  const [masked, setMasked] = usePrivacy();
  const mainRef = useRef<HTMLElement>(null);
  const notifications = useNotifications();
  const update = useUpdate(locked === false);
  const { setOpen: setNotificationsOpen } = notifications;
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
    api
      .pinEnabled()
      .then((on) => {
        pinOn.current = on;
        setLocked(on);
      })
      .catch(() => setLocked(false));
    const changed = (e: Event) => {
      pinOn.current = (e as CustomEvent<boolean>).detail;
    };
    window.addEventListener(PIN_CHANGED, changed);
    const lock = listen("app-lock", () => {
      if (pinOn.current) setLocked(true);
    });
    return () => {
      window.removeEventListener(PIN_CHANGED, changed);
      lock.then((stop) => stop());
    };
  }, []);

  useEffect(() => {
    if (!masked || locked !== false) return;
    api
      .listCustomers(undefined, true)
      .then((list) => setCustomerNames(list.flatMap((c) => [c.name, c.contact ?? "", ...(c.contacts ?? []).map((k) => k.name ?? "")])))
      .catch(() => {});
  }, [masked, locked, visit]);

  useEffect(() => {
    if ((settings && locked !== null) || loadError) dismissSplash().then(() => setSplashGone(true));
  }, [settings, locked, loadError]);

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
      if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.key.toLowerCase() === "h") {
        e.preventDefault();
        setMasked(!isMasked());
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [setMasked]);

  useEffect(() => {
    const open = (e: Event) => (e as CustomEvent<unknown>).detail && setNotificationsOpen(true);
    window.addEventListener(UPDATE_EVENT, open);
    return () => window.removeEventListener(UPDATE_EVENT, open);
  }, [setNotificationsOpen]);

  const go = useCallback((r: Route, p: RouteParams = {}) => {
    setRoute(r);
    setParams(p);
    setVisit((v) => v + 1);
  }, []);

  if (loadError) {
    return (
      <div className="p-8 pt-14 text-neg" data-selectable>
        <div className="fixed inset-x-0 top-0 h-12" data-tauri-drag-region />
        {t("The database could not be loaded:")} {loadError}
      </div>
    );
  }
  if (!settings || locked === null) return null;
  if (locked) return <LockScreen ready={splashGone} onUnlock={() => setLocked(false)} />;
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
      <div className="relative isolate flex h-full flex-col">
        <div className="sky" aria-hidden />
        <header className="grid h-12 shrink-0 grid-cols-[1fr_minmax(0,28rem)_1fr] items-center gap-4" data-tauri-drag-region="deep">
          <button
            type="button"
            onClick={() => setPaletteOpen(true)}
            className="glass col-start-2 flex h-8 w-full items-center gap-2 rounded-md border px-2.5 text-[13px] text-fg/70 transition-colors duration-150 hover:text-fg"
          >
            <Search className="size-3.5" />
            {t("Search or run an action…")}
            <kbd className="ml-auto rounded border border-fg/15 px-1 text-[11px]">{keys("⌘K", "Ctrl+K")}</kbd>
          </button>
          <div className="flex items-center justify-end gap-1 pr-4">
            <NotificationsButton state={notifications} update={update} />
          </div>
        </header>
        <div className="flex min-h-0 flex-1">
          <aside className="rail mb-2 ml-2 flex w-[224px] shrink-0 flex-col overflow-hidden rounded-[10px] border">
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
                      "flex h-8 items-center gap-2.5 rounded-md border px-2.5 text-[13px] whitespace-nowrap transition-colors duration-150",
                      active ? "nav-active font-medium text-fg" : "border-transparent text-fg/70 hover:bg-hover hover:text-fg",
                    )}
                  >
                    {n.icon}
                    {n.label}
                  </button>
                );
              })}
            </nav>
            <RunningTimerChip onOpen={() => go("tracker")} />
            <UpdateHint update={update} onOpen={() => setNotificationsOpen(true)} />
            <div className="border-t border-line p-2">
              <button
                type="button"
                onClick={() => go("settings")}
                aria-current={route === "settings" ? "page" : undefined}
                className={cx(
                  "flex h-8 w-full items-center gap-2.5 rounded-md border px-2.5 text-[13px] transition-colors duration-150",
                  route === "settings" ? "nav-active font-medium" : "border-transparent text-fg/70 hover:bg-hover hover:text-fg",
                )}
              >
                <SettingsIcon className="size-4" />
                {t("Settings")}
              </button>
            </div>
          </aside>

          <main ref={mainRef} onScroll={(e) => e.currentTarget.toggleAttribute("data-scrolled", e.currentTarget.scrollTop > 0)} className="content min-h-0 min-w-0 flex-1 overflow-y-scroll">
            <div key={visit} className="anim-page mx-auto w-full max-w-[1240px] px-6 py-6">
              {page}
            </div>
          </main>
        </div>
      </div>
      <NotificationsPanel state={notifications} update={update} />
      <CommandPalette open={paletteOpen} onClose={() => setPaletteOpen(false)} />
    </AppContext.Provider>
  );
}
