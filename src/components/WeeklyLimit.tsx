import { AlertTriangle } from "lucide-react";
import type { Entry, ExceededWeek, WeeklyHours } from "../lib/api";
import { date, duration } from "../lib/format";
import { lang, locale, t } from "../lib/i18n";
import { cx } from "./ui";

const hoursMinutes = (seconds: number) => `${Math.floor(seconds / 3600)}:${String(Math.floor((seconds % 3600) / 60)).padStart(2, "0")}`;

function weekSeconds(week: WeeklyHours, running: Entry | null, now: number): number {
  const live = running && running.date >= week.week_start ? Math.max(0, (now - new Date(running.start).getTime()) / 1000) : 0;
  return week.tracked_seconds + live;
}

export function WeeklyLimitBar({ week, running, now, className }: { week: WeeklyHours; running: Entry | null; now: number; className?: string }) {
  if (week.limit_hours <= 0) return null;
  const used = weekSeconds(week, running, now);
  const limit = week.limit_hours * 3600;
  const pct = (used / limit) * 100;
  const over = used > limit;
  return (
    <div className={cx("flex flex-col gap-1.5", className)}>
      <div className="flex items-baseline justify-between gap-2 text-xs">
        <span className="font-medium">{t("This week")}</span>
        <span className="tabular-nums text-muted">
          {t("{used} of {limit} h", { used: hoursMinutes(used), limit: week.limit_hours.toLocaleString(locale(), { maximumFractionDigits: 2 }) })}
        </span>
      </div>
      <div
        className="h-1.5 overflow-hidden rounded-full bg-line"
        role="progressbar"
        aria-label={t("This week")}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={Math.round(Math.min(100, pct))}
      >
        <div className={cx("h-full rounded-full transition-[width] duration-500", over ? "bg-neg" : pct >= 80 ? "bg-warn" : "bg-pos")} style={{ width: `${Math.min(100, pct)}%` }} />
      </div>
      <span className={cx("text-[11px]", over ? "font-medium text-neg" : "text-muted")}>
        {over ? t("{time} over the weekly limit", { time: duration(used - limit) }) : t("{time} left", { time: duration(limit - used) })}
      </span>
    </div>
  );
}

function weekRange(w: ExceededWeek): string {
  const [start, end] = [date(w.week_start), date(w.week_end)];
  return w.week_start.slice(0, 4) === w.week_end.slice(0, 4) ? `${start.slice(0, lang() === "de" ? -4 : -5)}–${end}` : `${start}–${end}`;
}

export function PastWeeksBanner({ week, onOpenWeek, className }: { week: WeeklyHours; onOpenWeek?: (weekStart: string) => void; className?: string }) {
  const open = week.limit_hours > 0 ? week.past_exceeded : [];
  if (open.length === 0) return null;
  const limit = week.limit_hours * 3600;
  const limitLabel = week.limit_hours.toLocaleString(locale(), { maximumFractionDigits: 2 });
  const [latest] = open;
  const title =
    open.length === 1
      ? t("Weekly limit exceeded in the week of {range}", { range: weekRange(latest) })
      : t("Weekly limit exceeded in {n} past weeks", { n: open.length });
  const detail =
    open.length === 1
      ? t("{used} of {limit} h tracked, {time} too much", { used: hoursMinutes(latest.tracked_seconds), limit: limitLabel, time: duration(latest.tracked_seconds - limit) })
      : t("Most recently {range} with {time} too much", { range: weekRange(latest), time: duration(latest.tracked_seconds - limit) });
  const body = (
    <>
      <AlertTriangle className="size-4 shrink-0" />
      <span className="min-w-0 flex-1 text-xs">
        <span className="block font-medium">{title}</span>
        <span className="tabular-nums mt-0.5 block text-[11px] opacity-90">{detail}</span>
      </span>
      {onOpenWeek && (
        <span className="shrink-0 text-[11px] font-medium">{t("Show in calendar")}</span>
      )}
    </>
  );
  return (
    <div role="alert" className={cx("flex flex-col gap-2 bg-neg/8 text-neg", className)}>
      <div className="flex items-center gap-2.5">
        {onOpenWeek ? (
          <button type="button" onClick={() => onOpenWeek(latest.week_start)} className="-mx-2.5 -my-1 flex min-w-0 flex-1 items-center gap-2.5 rounded-md px-2.5 py-1 text-left hover:bg-neg/8">
            {body}
          </button>
        ) : (
          <div className="flex min-w-0 flex-1 items-center gap-2.5">{body}</div>
        )}
      </div>
      {onOpenWeek && open.length > 1 && (
        <div className="tabular-nums flex flex-wrap gap-1.5 pl-6.5 text-[11px]">
          {open.slice(0, 6).map((w) => (
            <button key={w.week_start} type="button" onClick={() => onOpenWeek(w.week_start)} className="rounded-full border border-neg/25 px-2 py-0.5 hover:bg-neg/10">
              {weekRange(w)} · +{hoursMinutes(w.tracked_seconds - limit)}
            </button>
          ))}
          {open.length > 6 && <span className="self-center px-1">{t("+{n} more", { n: open.length - 6 })}</span>}
        </div>
      )}
    </div>
  );
}
