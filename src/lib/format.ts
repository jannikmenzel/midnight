import { lang, locale, t } from "./i18n";

const moneyFormatters = new Map<string, Intl.NumberFormat>();

export function money(cents: number | null | undefined, currency = "EUR", opts: { sign?: boolean; compact?: boolean } = {}): string {
  if (cents === null || cents === undefined) return "–";
  const key = `${locale()}-${currency}-${opts.compact ? "c" : "f"}`;
  let fmt = moneyFormatters.get(key);
  if (!fmt) {
    fmt = new Intl.NumberFormat(locale(), {
      style: "currency",
      currency,
      maximumFractionDigits: opts.compact ? 0 : 2,
      minimumFractionDigits: opts.compact ? 0 : 2,
    });
    moneyFormatters.set(key, fmt);
  }
  const text = fmt.format(cents / 100);
  return opts.sign && cents > 0 ? `+${text}` : text;
}

export function parseMoney(input: string): number | null {
  const s = input.replace(/[^\d.,-]/g, "");
  if (!/\d/.test(s)) return null;
  const lastComma = s.lastIndexOf(",");
  const lastDot = s.lastIndexOf(".");
  let normalized = s;
  if (lastComma >= 0 && lastDot >= 0) {
    normalized = lastComma > lastDot ? s.replace(/\./g, "").replace(",", ".") : s.replace(/,/g, "");
  } else if (lastComma >= 0 || lastDot >= 0) {
    const sep = lastComma >= 0 ? "," : ".";
    const parts = s.split(sep);
    const group = lang() === "de" ? "." : ",";
    const thousands = parts.length > 2 || (sep === group && parts[1].length === 3);
    normalized = parts.join(thousands ? "" : ".");
  }
  const value = Number(normalized);
  return Number.isFinite(value) ? Math.round(value * 100) : null;
}

export function centsToInput(cents: number | null | undefined): string {
  if (cents === null || cents === undefined) return "";
  const fixed = (cents / 100).toFixed(2);
  return lang() === "de" ? fixed.replace(".", ",") : fixed;
}

function decimalInput(n: number): string {
  const s = String(n);
  return lang() === "de" ? s.replace(".", ",") : s;
}

export function date(iso: string | null | undefined): string {
  if (!iso) return "–";
  const [y, m, d] = iso.slice(0, 10).split("-");
  return lang() === "de" ? `${d}.${m}.${y}` : `${d}/${m}/${y}`;
}

function dateTime(ts: string): string {
  const d = new Date(ts);
  return d.toLocaleString(locale(), { day: "2-digit", month: "2-digit", hour: "2-digit", minute: "2-digit" });
}

export function relativeTime(ts: string): string {
  const diff = (Date.now() - new Date(ts).getTime()) / 1000;
  if (diff < 60) return t("just now");
  if (diff < 3600) return t("{n} min ago", { n: Math.floor(diff / 60) });
  if (diff < 86400) return t("{n} h ago", { n: Math.floor(diff / 3600) });
  if (diff < 86400 * 7) return t("{n} days ago", { n: Math.floor(diff / 86400) });
  return dateTime(ts);
}

export function duration(seconds: number): string {
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  return `${h}:${String(m).padStart(2, "0")} h`;
}

export function clock(seconds: number): string {
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = Math.floor(seconds % 60);
  return [h, m, s].map((n) => String(n).padStart(2, "0")).join(":");
}

export function hours(seconds: number): string {
  return `${Math.floor(seconds / 3600)}:${String(Math.floor((seconds % 3600) / 60)).padStart(2, "0")}`;
}

export const HOURS = "HUR";

export function quantityInput(n: number, unit: string | null | undefined): string {
  if (unit !== HOURS) return decimalInput(n);
  const minutes = Math.round(Math.abs(n) * 60);
  return `${n < 0 ? "-" : ""}${Math.floor(minutes / 60)}:${String(minutes % 60).padStart(2, "0")}`;
}

export function parseQuantity(s: string): number {
  const clock = s.trim().match(/^(-?)(\d+):(\d{1,2})$/);
  if (clock) return (clock[1] ? -1 : 1) * (Number(clock[2]) + Number(clock[3]) / 60);
  return Number(s.replace(",", ".")) || 0;
}

export function fileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

export function isoDate(d: Date): string {
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${y}-${m}-${day}`;
}

export function todayIso(): string {
  return isoDate(new Date());
}

export function addDays(iso: string, days: number): string {
  const d = new Date(`${iso}T12:00:00`);
  d.setDate(d.getDate() + days);
  return isoDate(d);
}

export type PeriodKey = "this_month" | "last_month" | "last_3_months" | "this_year" | "last_year" | "last_12_months" | "all";

export const PERIODS: { key: PeriodKey; label: string }[] = [
  { key: "this_month", label: t("This month") },
  { key: "last_month", label: t("Last month") },
  { key: "last_3_months", label: t("Last 3 months") },
  { key: "this_year", label: t("This year") },
  { key: "last_12_months", label: t("Last 12 months") },
  { key: "last_year", label: t("Last year") },
  { key: "all", label: t("All time") },
];

export function periodRange(key: PeriodKey): { from: string; to: string } {
  const now = new Date();
  const y = now.getFullYear();
  const m = now.getMonth();
  const first = (yy: number, mm: number) => isoDate(new Date(yy, mm, 1));
  const last = (yy: number, mm: number) => isoDate(new Date(yy, mm + 1, 0));
  switch (key) {
    case "this_month":
      return { from: first(y, m), to: last(y, m) };
    case "last_month":
      return { from: first(y, m - 1), to: last(y, m - 1) };
    case "last_3_months":
      return { from: first(y, m - 2), to: last(y, m) };
    case "this_year":
      return { from: `${y}-01-01`, to: `${y}-12-31` };
    case "last_year":
      return { from: `${y - 1}-01-01`, to: `${y - 1}-12-31` };
    case "last_12_months":
      return { from: first(y, m - 11), to: last(y, m) };
    case "all":
      return { from: "2000-01-01", to: last(y, m) };
  }
}

export const MONTHS_SHORT = [t("Jan"), t("Feb"), t("Mar"), t("Apr"), t("May"), t("Jun"), t("Jul"), t("Aug"), t("Sep"), t("Oct"), t("Nov"), t("Dec")];

export function monthLabel(yyyyMm: string): string {
  const [y, m] = yyyyMm.split("-");
  return `${MONTHS_SHORT[Number(m) - 1]} ${y.slice(2)}`;
}

export function pctChange(current: number, previous: number): number | null {
  if (previous === 0) return null;
  return ((current - previous) / Math.abs(previous)) * 100;
}

export const STATUS_LABELS: Record<string, string> = {
  draft: t("Draft"),
  unpaid: t("Outstanding"),
  overdue: t("Overdue"),
  paid: t("Paid"),
  canceled: t("Canceled"),
  scheduled: t("Scheduled"),
  refunded: t("Refunded"),
};

export const FREQUENCY_LABELS: Record<string, string> = {
  weekly: t("Weekly"),
  biweekly: t("Every 2 weeks"),
  monthly_date: t("Monthly"),
  monthly_last_day: t("Monthly on the last day"),
  quarterly: t("Quarterly"),
  semi_annual: t("Every 6 months"),
  annual: t("Yearly"),
  custom: t("Every X days"),
};
