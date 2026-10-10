import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { emit, listen } from "@tauri-apps/api/event";
import { AppWindow, Play, Power, Search, Square } from "lucide-react";
import { api, errorMessage, type Entry, type Project, type WeeklyHours } from "../lib/api";
import { clock, duration, todayIso } from "../lib/format";
import { useTick } from "../lib/hooks";
import { t } from "../lib/i18n";
import { cx } from "./ui";
import { PastWeeksBanner, WeeklyLimitBar } from "./WeeklyLimit";

const CHANGED = "tracker-changed";

function readDark(): boolean {
  let theme = "system";
  try {
    theme = localStorage.getItem("theme") || "system";
  } catch {
  }
  return theme === "dark" || (theme === "system" && window.matchMedia("(prefers-color-scheme: dark)").matches);
}

function useTheme() {
  useEffect(() => {
    const apply = () => document.documentElement.classList.toggle("dark", readDark());
    const onStorage = (e: StorageEvent) => (e.key === "language" ? window.location.reload() : apply());
    apply();
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    media.addEventListener("change", apply);
    window.addEventListener("storage", onStorage);
    window.addEventListener("focus", apply);
    return () => {
      media.removeEventListener("change", apply);
      window.removeEventListener("storage", onStorage);
      window.removeEventListener("focus", apply);
    };
  }, []);
}

const elapsed = (entry: Entry, now: number) => Math.max(0, (now - new Date(entry.start).getTime()) / 1000);

export default function MenuBarPanel() {
  useTheme();
  const [projects, setProjects] = useState<Project[]>([]);
  const [running, setRunning] = useState<Entry | null>(null);
  const [today, setToday] = useState<Entry[]>([]);
  const [week, setWeek] = useState<WeeklyHours | null>(null);
  const [note, setNote] = useState("");
  const [query, setQuery] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const searchRef = useRef<HTMLInputElement>(null);
  const now = useTick(1000, running !== null);

  const load = useCallback(async () => {
    try {
      const day = todayIso();
      const [p, r, e, w] = await Promise.all([api.listProjects(), api.runningTimer(), api.listEntries(null, day, day), api.weeklyHours()]);
      setProjects(p);
      setRunning(r);
      setToday(e);
      setWeek(w);
      setError(null);
    } catch (e) {
      setError(errorMessage(e));
    }
  }, []);

  useEffect(() => {
    load();
    const offs = [
      listen("menubar-shown", () => {
        setQuery("");
        load();
        setTimeout(() => searchRef.current?.focus(), 50);
      }),
      listen(CHANGED, load),
    ];
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") api.hideMenuBarPanel();
    };
    window.addEventListener("keydown", onKey);
    return () => {
      offs.forEach((off) => off.then((f) => f()).catch(() => {}));
      window.removeEventListener("keydown", onKey);
    };
  }, [load]);

  const changed = async () => {
    await load();
    await api.refreshMenuBar().catch(() => {});
    await emit(CHANGED).catch(() => {});
  };

  const run = async (action: () => Promise<unknown>) => {
    if (busy) return;
    setBusy(true);
    try {
      await action();
      await changed();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const start = (project: Project) =>
    run(async () => {
      await api.startTimer(project.id, note.trim() || null);
      setNote("");
      setQuery("");
    });

  const stop = () => run(() => api.stopTimer());

  const visible = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return projects;
    return projects.filter((p) => `${p.name} ${p.customer_name ?? ""}`.toLowerCase().includes(q));
  }, [projects, query]);

  const todaySeconds = today.reduce((sum, e) => sum + (e.stop ? e.duration : elapsed(e, now)), 0);

  return (
    <div className="flex h-full flex-col overflow-hidden rounded-xl border border-line bg-panel text-fg">
      {running ? (
        <div className="flex items-center gap-3 border-b border-line px-4 py-3.5">
          <span className="size-2 shrink-0 animate-pulse rounded-full bg-neg" />
          <div className="min-w-0 flex-1">
            <div className="truncate text-[13px] font-medium">{running.project_name}</div>
            <div className="truncate text-xs text-muted">{running.description ?? t("No description")}</div>
            <div className="tabular mt-1 text-xl">{clock(elapsed(running, now))}</div>
          </div>
          <button
            type="button"
            onClick={stop}
            disabled={busy}
            className="inline-flex h-8 items-center gap-1.5 rounded-md bg-neg px-3 text-[13px] font-medium text-white hover:opacity-90 disabled:opacity-50"
          >
            <Square className="size-3 fill-current" />
            {t("Stop")}
          </button>
        </div>
      ) : (
        <div className="border-b border-line px-4 py-3.5">
          <div className="font-serif text-lg">{t("No timer running")}</div>
          <div className="text-xs text-muted">{t("Pick a project to start tracking.")}</div>
        </div>
      )}

      <div className="flex flex-col gap-2 border-b border-line p-3">
        <input
          value={note}
          onChange={(e) => setNote(e.target.value)}
          placeholder={t("What are you working on?")}
          className="h-8 rounded-md border border-line bg-bg px-2.5 text-[13px] outline-none placeholder:text-muted focus:border-muted"
        />
        <label className="flex h-8 items-center gap-2 rounded-md border border-line bg-bg px-2.5 focus-within:border-muted">
          <Search className="size-3.5 text-muted" />
          <input
            ref={searchRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && visible[0]) start(visible[0]);
            }}
            placeholder={t("Search project…")}
            className="min-w-0 flex-1 bg-transparent text-[13px] outline-none placeholder:text-muted"
          />
        </label>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto py-1">
        {error && <p className="px-4 py-2 text-xs text-neg">{error}</p>}
        {visible.length === 0 ? (
          <p className="px-4 py-6 text-center text-xs text-muted">{projects.length === 0 ? t("No active projects") : t("No results")}</p>
        ) : (
          visible.map((p) => {
            const active = running?.project_id === p.id;
            return (
              <button
                key={p.id}
                type="button"
                disabled={busy || active}
                onClick={() => start(p)}
                className={cx("group flex w-full items-center gap-3 px-4 py-2 text-left", active ? "bg-subtle" : "hover:bg-subtle")}
              >
                <div className="min-w-0 flex-1">
                  <div className="truncate text-[13px]">{p.name}</div>
                  {p.customer_name && <div className="truncate text-xs text-muted">{p.customer_name}</div>}
                </div>
                {active ? (
                  <span className="text-xs text-neg">{t("Running")}</span>
                ) : (
                  <span className="inline-flex size-7 items-center justify-center rounded-full bg-subtle text-muted group-hover:bg-line group-hover:text-fg">
                    <Play className="size-3 fill-current" />
                  </span>
                )}
              </button>
            );
          })
        )}
      </div>

      {week && <PastWeeksBanner week={week} className="border-t border-line px-4 py-2.5" />}
      {week && week.limit_hours > 0 && <WeeklyLimitBar week={week} running={running} now={now} className="border-t border-line px-4 py-2.5" />}

      <div className="flex items-center gap-1 border-t border-line px-3 py-2 text-xs">
        <span className="flex-1 text-muted">{t("Today {time}", { time: duration(todaySeconds) })}</span>
        <button type="button" onClick={() => api.openMainWindow()} className="inline-flex h-7 items-center gap-1.5 rounded-md px-2 hover:bg-subtle">
          <AppWindow className="size-3.5" />
          {t("Open Midnight")}
        </button>
        <button
          type="button"
          onClick={() => api.quitApp()}
          title={t("Quit Midnight")}
          aria-label={t("Quit Midnight")}
          className="inline-flex size-7 items-center justify-center rounded-md text-muted hover:bg-subtle hover:text-fg"
        >
          <Power className="size-3.5" />
        </button>
      </div>
    </div>
  );
}
