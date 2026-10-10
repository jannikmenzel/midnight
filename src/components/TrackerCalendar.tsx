import { Fragment, useMemo } from "react";
import { AlertTriangle, ChevronLeft, ChevronRight, Pencil, Plus, Trash2 } from "lucide-react";
import type { Entry } from "../lib/api";
import { date, duration, isoDate, todayIso } from "../lib/format";
import { Badge, Button, Card, Empty, IconButton, cx } from "./ui";
import { locale, t } from "../lib/i18n";

const WEEKDAYS = [t("Mo"), t("Tu"), t("We"), t("Th"), t("Fr"), t("Sa"), t("Su")];
const weekdays = (mondayFirst: boolean) => (mondayFirst ? WEEKDAYS : [WEEKDAYS[6], ...WEEKDAYS.slice(0, 6)]);
const hoursMinutes = (seconds: number) => duration(seconds).replace(" h", "");

const time = (ts: string) => new Date(ts).toLocaleTimeString(locale(), { hour: "2-digit", minute: "2-digit" });

export function calendarRange(month: string, mondayFirst = true): { from: string; to: string; days: string[] } {
  const [y, m] = month.split("-").map(Number);
  const offset = (d: Date) => (mondayFirst ? (d.getDay() + 6) % 7 : d.getDay());
  const first = new Date(y, m - 1, 1, 12);
  const start = new Date(first);
  start.setDate(1 - offset(first));
  const last = new Date(y, m, 0, 12);
  const end = new Date(last);
  end.setDate(last.getDate() + (6 - offset(last)));
  const days: string[] = [];
  for (const d = new Date(start); d <= end; d.setDate(d.getDate() + 1)) days.push(isoDate(d));
  return { from: days[0], to: days[days.length - 1], days };
}

function shiftMonth(month: string, by: number): string {
  const [y, m] = month.split("-").map(Number);
  return isoDate(new Date(y, m - 1 + by, 1, 12)).slice(0, 7);
}

export function TrackerCalendar({
  month,
  onMonth,
  selected,
  onSelect,
  entries,
  onOpenProject,
  onNewEntry,
  onEditEntry,
  onDeleteEntry,
  canAdd,
  mondayFirst,
  weeklyLimitHours,
}: {
  month: string;
  onMonth: (month: string) => void;
  selected: string;
  onSelect: (day: string) => void;
  entries: Entry[];
  onOpenProject: (projectId: string) => void;
  onNewEntry: (day: string) => void;
  onEditEntry: (e: Entry) => void;
  onDeleteEntry: (e: Entry) => void;
  canAdd: boolean;
  mondayFirst: boolean;
  weeklyLimitHours: number;
}) {
  const { days } = calendarRange(month, mondayFirst);
  const weeks = Array.from({ length: days.length / 7 }, (_, i) => days.slice(i * 7, i * 7 + 7));
  const showWeeks = weeklyLimitHours > 0;
  const columns = showWeeks ? "grid-cols-[repeat(7,minmax(0,1fr))_4.5rem]" : "grid-cols-7";
  const today = todayIso();

  const byDay = useMemo(() => {
    const map = new Map<string, Entry[]>();
    for (const e of entries.filter((x) => x.stop)) map.set(e.date, [...(map.get(e.date) ?? []), e]);
    for (const list of map.values()) list.sort((a, b) => a.start.localeCompare(b.start));
    return map;
  }, [entries]);

  const monthTotal = days.filter((d) => d.startsWith(month)).reduce((s, d) => s + (byDay.get(d) ?? []).reduce((t, e) => t + e.duration, 0), 0);
  const title = new Date(`${month}-01T12:00:00`).toLocaleDateString(locale(), { month: "long", year: "numeric" });
  const dayEntries = byDay.get(selected) ?? [];

  return (
    <Card
      title={t("Calendar")}
      actions={
        <div className="flex items-center gap-1">
          <span className="mr-2 text-xs text-muted">
            <span className="text-fg">{title}</span> · <span className="tabular">{duration(monthTotal)}</span>
          </span>
          <IconButton label={t("Previous month")} onClick={() => onMonth(shiftMonth(month, -1))}>
            <ChevronLeft className="size-3.5" />
          </IconButton>
          <Button
            size="sm"
            variant="ghost"
            onClick={() => {
              onMonth(today.slice(0, 7));
              onSelect(today);
            }}
          >
            {t("Today")}
          </Button>
          <IconButton label={t("Next month")} onClick={() => onMonth(shiftMonth(month, 1))}>
            <ChevronRight className="size-3.5" />
          </IconButton>
        </div>
      }
    >
      <div className={cx("grid border-b border-line text-[11px] text-muted", columns)}>
        {weekdays(mondayFirst).map((w) => (
          <div key={w} className="px-2 py-1.5">
            {w}
          </div>
        ))}
        {showWeeks && <div className="border-l border-line px-2 py-1.5 text-right">{t("Week")}</div>}
      </div>

      <div className={cx("grid", columns)}>
        {weeks.map((week, row) => {
          const weekTotal = week.reduce((s, d) => s + (byDay.get(d) ?? []).reduce((t, e) => t + e.duration, 0), 0);
          const over = weekTotal > weeklyLimitHours * 3600;
          const lastRow = row === weeks.length - 1;
          return (
            <Fragment key={week[0]}>
        {week.map((day, col) => {
          const list = byDay.get(day) ?? [];
          const total = list.reduce((s, e) => s + e.duration, 0);
          const projects = [...new Map(list.map((e) => [e.project_id, e.project_name])).values()];
          const inMonth = day.startsWith(month);
          const isSel = day === selected;
          return (
            <button
              key={day}
              type="button"
              onClick={() => onSelect(day)}
              onDoubleClick={() => canAdd && onNewEntry(day)}
              aria-label={`${date(day)}${total ? `, ${duration(total)}` : ""}`}
              aria-pressed={isSel}
              className={cx(
                "flex min-h-24 min-w-0 flex-col gap-1 border-line p-2 text-left outline-none hover:bg-subtle focus-visible:bg-subtle",
                (col !== 6 || showWeeks) && "border-r",
                !lastRow && "border-b",
                !inMonth && "bg-subtle/50",
                isSel && "bg-subtle shadow-[inset_0_0_0_1px_var(--muted)]",
              )}
            >
              <div className="flex items-center justify-between gap-1">
                <span
                  className={cx(
                    "tabular-nums inline-flex size-5 items-center justify-center rounded-full text-[11px]",
                    day === today ? "bg-accent font-medium text-accent-fg" : inMonth ? "text-fg" : "text-muted",
                  )}
                >
                  {Number(day.slice(8))}
                </span>
                {total > 0 && <span className="tabular text-[11px] text-muted">{duration(total).replace(" h", "")}</span>}
              </div>
              {projects.slice(0, 2).map((name) => (
                <span key={name} className="truncate rounded bg-info/12 px-1 py-px text-[10px] font-medium text-info">
                  {name}
                </span>
              ))}
              {projects.length > 2 && <span className="px-1 text-[10px] text-muted">{t("+{n} more", { n: projects.length - 2 })}</span>}
            </button>
          );
        })}
              {showWeeks && (
                <div
                  className={cx("flex flex-col items-end gap-1 border-line p-2 text-right", !lastRow && "border-b", over && "bg-neg/8")}
                  title={over ? t("{time} over the weekly limit", { time: duration(weekTotal - weeklyLimitHours * 3600) }) : undefined}
                >
                  <span className={cx("tabular text-[11px]", over ? "font-medium text-neg" : weekTotal > 0 ? "text-fg" : "text-muted")}>
                    {weekTotal > 0 ? hoursMinutes(weekTotal) : "–"}
                  </span>
                  {over && <AlertTriangle className="size-3.5 text-neg" aria-label={t("{time} over the weekly limit", { time: duration(weekTotal - weeklyLimitHours * 3600) })} />}
                </div>
              )}
            </Fragment>
          );
        })}
      </div>

      <div className="border-t border-line">
        <div className="flex items-center justify-between bg-subtle px-4 py-1.5 text-xs text-muted">
          <span>
            {selected === today ? t("Today") : date(selected)}
            {dayEntries.length > 0 && <span className="tabular"> · {duration(dayEntries.reduce((s, e) => s + e.duration, 0))}</span>}
          </span>
          <Button size="sm" variant="ghost" icon={<Plus className="size-3" />} onClick={() => onNewEntry(selected)} disabled={!canAdd}>
            {t("Add time")}
          </Button>
        </div>
        {dayEntries.length === 0 ? (
          <Empty title={t("No entries on this day")} />
        ) : (
          <div className="divide-y divide-line">
            {dayEntries.map((e) => (
              <div key={e.id} className="flex items-center gap-3 px-4 py-2.5 text-[13px]">
                <button type="button" className="w-40 truncate text-left font-medium hover:underline" onClick={() => onOpenProject(e.project_id)}>
                  {e.project_name}
                </button>
                <span className="min-w-0 flex-1 truncate text-muted">{e.description}</span>
                {e.billed && <Badge tone="pos">{e.invoice_number ?? "abgerechnet"}</Badge>}
                <span className="tabular-nums text-xs text-muted">
                  {time(e.start)}–{e.stop && time(e.stop)}
                </span>
                <span className="tabular w-16 text-right">{duration(e.duration)}</span>
                <IconButton label={t("Edit")} onClick={() => onEditEntry(e)}>
                  <Pencil className="size-3.5" />
                </IconButton>
                <IconButton label={t("Delete")} onClick={() => onDeleteEntry(e)}>
                  <Trash2 className="size-3.5" />
                </IconButton>
              </div>
            ))}
          </div>
        )}
      </div>
    </Card>
  );
}
