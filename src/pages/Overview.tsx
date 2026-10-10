import { useMemo, useState, type ReactNode } from "react";
import { ArrowRight } from "lucide-react";
import { api, type ForecastOutlook, type Summary, type TaxReport } from "../lib/api";
import { date, duration, money, PERIODS, pctChange, periodRange, todayIso, type PeriodKey } from "../lib/format";
import { useAsync } from "../lib/hooks";
import { useApp } from "../lib/nav";
import { CashflowChart } from "../components/Cashflow";
import { SmallBusinessCard } from "../components/SmallBusiness";
import { Amount, Badge, Button, Card, CategoryDot, Empty, Kpi, PageHeader, Select, Skeleton, cx } from "../components/ui";
import { locale, t } from "../lib/i18n";

function Trend({ current, previous, invert = false }: { current: number; previous: number; invert?: boolean }) {
  const p = pctChange(current, previous);
  if (p === null) return <span>{t("no comparison")}</span>;
  const good = invert ? p <= 0 : p >= 0;
  return (
    <span className={good ? "text-pos" : "text-neg"}>
      {t("{pct} % vs. previous period", { pct: `${p >= 0 ? "+" : ""}${p.toFixed(0)}` })}
    </span>
  );
}

const clockTime = (ts: string) => new Date(ts).toLocaleTimeString(locale(), { hour: "2-digit", minute: "2-digit" });

const PERIOD_KEY = "overview-period";

function readPeriod(): PeriodKey {
  try {
    const saved = localStorage.getItem(PERIOD_KEY);
    if (saved && PERIODS.some((p) => p.key === saved)) return saved as PeriodKey;
  } catch {
  }
  return "this_year";
}

const monthStart = (d: Date) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-01`;

const percent = (fraction: number) => `${Math.round(fraction * 100)} %`;

function yearProgress(year: number) {
  const now = new Date();
  if (now.getFullYear() !== year) return null;
  const start = new Date(year, 0, 1).getTime();
  const end = new Date(year + 1, 0, 1).getTime();
  return (now.getTime() - start) / (end - start);
}

function Cell({ onClick, children }: { onClick: () => void; children: ReactNode }) {
  return (
    <button type="button" onClick={onClick} className="flex flex-col justify-start p-5 text-left transition-colors hover:bg-subtle">
      {children}
    </button>
  );
}

function Meter({ solid, faded, color, label }: { solid: number; faded: number; color: string; label: string }) {
  return (
    <div className="relative h-1.5 w-full rounded-full bg-line" role="progressbar" aria-label={label} aria-valuenow={Math.round(solid)} aria-valuemin={0} aria-valuemax={100}>
      {faded > 0 && <div className={cx("absolute inset-y-0 left-0 rounded-full opacity-30", color)} style={{ width: `${Math.min(100, faded)}%` }} />}
      {solid > 0 && <div className={cx("relative h-1.5 rounded-full", color)} style={{ width: `${Math.min(100, solid)}%`, minWidth: 6 }} />}
    </div>
  );
}

function TaxSummary({ year, forecast, report, currency }: { year: number; forecast?: ForecastOutlook; report?: TaxReport; currency: string }) {
  const expected = forecast?.balance_cents;
  const soFar = report?.balance_cents ?? 0;
  const refund = (expected ?? 0) < 0;
  const monthsLeft = new Date().getFullYear() === year ? 12 - new Date().getMonth() : 0;
  const share = expected && expected > 0 ? Math.max(0, soFar / expected) * 100 : 0;
  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-baseline justify-between gap-2">
        <span className="text-xs text-muted">{t("Tax forecast {year}", { year })}</span>
        {forecast && forecast.marginal_rate > 0 && (
          <span className="text-xs text-muted">
            {t("Marginal rate")} <span className="tabular">{percent(forecast.marginal_rate)}</span>
          </span>
        )}
      </div>
      <div className="tabular text-xl font-semibold tracking-tight">
        <span className={cx(refund && "text-pos")}>{expected !== undefined ? money(Math.abs(expected), currency) : "…"}</span>{" "}
        <span className="text-[13px] font-normal text-muted">{refund ? t("refund") : t("additional payment")}</span>
      </div>
      <Meter solid={share} faded={expected && expected > 0 ? 100 : 0} color="bg-warn" label={t("Booked so far")} />
      <span className="text-xs text-muted">
        {expected !== undefined && expected > 0 && monthsLeft > 0
          ? t("Set aside {amount} per month until December.", { amount: money(Math.ceil(expected / monthsLeft / 100) * 100, currency) })
          : t("{amount} of it from bookings so far.", { amount: money(Math.max(0, soFar), currency) })}
      </span>
    </div>
  );
}

function OutstandingSummary({ s, unbilledCents, currency }: { s?: Summary; unbilledCents?: number; currency: string }) {
  const invoiced = s?.open_invoices_cents ?? 0;
  const overdue = Math.min(s?.overdue_cents ?? 0, invoiced);
  const unbilled = unbilledCents ?? 0;
  const total = invoiced + unbilled;
  const width = (cents: number) => (total > 0 ? `${(cents / total) * 100}%` : "0%");
  const ready = s !== undefined && unbilledCents !== undefined;
  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-baseline justify-between gap-2">
        <span className="text-xs text-muted">{t("Money outstanding")}</span>
        {s && s.open_invoices_count > 0 && <span className="tabular text-xs text-muted">{t("{n} open", { n: s.open_invoices_count })}</span>}
      </div>
      <div className="tabular text-xl font-semibold tracking-tight">{ready ? money(total, currency) : "…"}</div>
      <div
        className="flex h-1.5 w-full overflow-hidden rounded-full bg-line"
        role="img"
        aria-label={t("{invoiced} invoiced, {unbilled} not yet billed", { invoiced: money(invoiced, currency), unbilled: money(unbilled, currency) })}
      >
        {overdue > 0 && <div className="h-full bg-neg" style={{ width: width(overdue) }} />}
        {invoiced - overdue > 0 && <div className="h-full bg-pos" style={{ width: width(invoiced - overdue) }} />}
        {unbilled > 0 && <div className="h-full bg-pos opacity-30" style={{ width: width(unbilled) }} />}
      </div>
      <span className={cx("text-xs", s && s.overdue_count > 0 ? "text-neg" : "text-muted")}>
        {!ready
          ? "…"
          : s.overdue_count > 0
            ? t("{n} overdue · {amount}", { n: s.overdue_count, amount: money(s.overdue_cents, currency) })
            : unbilled > 0
              ? t("Of which {amount} not yet billed.", { amount: money(unbilled, currency) })
              : total > 0
                ? t("Nothing overdue.")
                : t("Nothing outstanding.")}
      </span>
    </div>
  );
}

export default function Overview() {
  const { go, settings } = useApp();
  const [period, setPeriodState] = useState<PeriodKey>(readPeriod);
  const setPeriod = (p: PeriodKey) => {
    setPeriodState(p);
    try {
      localStorage.setItem(PERIOD_KEY, p);
    } catch {
    }
  };
  const range = periodRange(period);
  const chartRange = useMemo(() => {
    const currentEnd = periodRange("this_month").to;
    const to = range.to < currentEnd ? range.to : currentEnd;
    const context = monthStart(new Date(Number(to.slice(0, 4)), Number(to.slice(5, 7)) - 6, 1));
    return { from: range.from < context ? range.from : context, to };
  }, [range.from, range.to]);
  const inPeriod = (month: string) => month >= range.from.slice(0, 7) && month <= range.to.slice(0, 7);

  const summary = useAsync(() => api.reportSummary(range.from, range.to), [range.from, range.to]);
  const monthly = useAsync(() => api.reportMonthly(chartRange.from, chartRange.to), [chartRange.from, chartRange.to]);
  const recent = useAsync(() => api.listLedger({ limit: 8 }), []);
  const entries = useAsync(() => api.listEntries(null, null, null), []);
  const recentEntries = (entries.data ?? []).filter((e) => e.stop).slice(0, 8);

  const thisYear = new Date().getFullYear();
  const taxReport = useAsync(() => api.getTaxReport(thisYear), [thisYear]);
  const taxForecast = useAsync(() => api.taxForecast(thisYear), [thisYear]);
  const projects = useAsync(() => api.listProjects(false), []);
  const vatFactor = settings.small_business ? 1 : 1 + settings.default_vat_rate / 100;
  const unbilledCents = projects.data ? Math.round(projects.data.filter((p) => p.billable).reduce((sum, p) => sum + p.unbilled_cents, 0) * vatFactor) : undefined;
  const firstBooking = useAsync(
    () => (period === "all" ? api.reportMonthly(range.from, range.to).then((rows) => rows[0]?.month ?? null) : Promise.resolve(null)),
    [period],
  );
  const monthsElapsed = useMemo(() => {
    const today = todayIso();
    const from = firstBooking.data ? `${firstBooking.data}-01` : range.from;
    const end = range.to < today ? range.to : today;
    const months = (Number(end.slice(0, 4)) - Number(from.slice(0, 4))) * 12 + Number(end.slice(5, 7)) - Number(from.slice(5, 7)) + 1;
    return Math.max(1, months);
  }, [range.from, range.to, firstBooking.data]);
  const averageReady = period !== "all" || !firstBooking.loading;

  const cur = settings.base_currency;
  const s = summary.data;
  const sb = s?.small_business;
  const progress = sb ? yearProgress(sb.year) : null;
  const projected = sb && progress !== null && progress >= 0.15 ? Math.round(sb.revenue_cents / progress / 10000) * 10000 : null;

  return (
    <div className="flex flex-col gap-6">
      <PageHeader
        title={settings.company_name ? t("Hello {name}", { name: settings.company_owner ? settings.company_owner.split(" ")[0] : settings.company_name }) : t("Overview")}
        subtitle={t("Income, expenses and open items in the selected period.")}
        actions={
          <Select value={period} onChange={(e) => setPeriod(e.target.value as PeriodKey)} className="w-44" aria-label={t("Period")}>
            {PERIODS.map((p) => (
              <option key={p.key} value={p.key}>
                {p.label}
              </option>
            ))}
          </Select>
        }
      />

      <div className="grid grid-cols-1 gap-4 md:grid-cols-3">
        <Kpi
          label={t("Income")}
          value={s ? money(s.revenue_cents, cur) : "…"}
          sub={s ? <Trend current={s.revenue_cents} previous={s.prev_revenue_cents} /> : "…"}
          onClick={() => go("ledger", { kind: "income", from: range.from, to: range.to })}
        />
        <Kpi
          label={t("Expenses")}
          value={s ? money(s.expenses_cents, cur) : "…"}
          sub={s ? <Trend current={s.expenses_cents} previous={s.prev_expenses_cents} invert /> : "…"}
          onClick={() => go("ledger", { kind: "expense", from: range.from, to: range.to })}
        />
        <Kpi
          label={t("Profit")}
          value={s ? money(s.profit_cents, cur) : "…"}
          tone={s && s.profit_cents < 0 ? "neg" : undefined}
          sub={
            s && averageReady
              ? [
                  s.revenue_cents > 0 && `${t("Margin")} ${percent(s.profit_cents / s.revenue_cents)}`,
                  monthsElapsed > 1 && t("Ø {amount} per month", { amount: money(Math.round(s.profit_cents / monthsElapsed), cur) }),
                ]
                  .filter(Boolean)
                  .join(" · ")
              : "…"
          }
        />
      </div>

      <section className="overflow-hidden rounded-lg border border-line bg-panel" aria-label={t("Cash flow")}>
        <div className="px-4 pt-5 pb-3">
          {monthly.data ? (
            monthly.data.length === 0 ? (
              <Empty title={t("No data in this period")} />
            ) : (
              <CashflowChart months={monthly.data} highlight={inPeriod} currency={cur} />
            )
          ) : (
            <Skeleton height={264} />
          )}
        </div>
      </section>

      <section className={cx("grid divide-line overflow-hidden rounded-lg border border-line bg-panel max-lg:divide-y lg:divide-x", settings.small_business ? "lg:grid-cols-3" : "lg:grid-cols-2")}>
        {settings.small_business &&
          (sb ? (
            <Cell onClick={() => go("reports", { view: "forecast" })}>
              <SmallBusinessCard data={sb} currency={cur} projectedCents={projected} bare />
            </Cell>
          ) : (
            <div className="m-5 animate-pulse rounded-lg bg-subtle" />
          ))}
        <Cell onClick={() => go("reports", { view: "forecast" })}>
          <TaxSummary year={thisYear} forecast={taxForecast.data?.outlook} report={taxReport.data ?? undefined} currency={cur} />
        </Cell>
        <Cell onClick={() => go("invoices")}>
          <OutstandingSummary s={s ?? undefined} unbilledCents={unbilledCents} currency={cur} />
        </Cell>
      </section>

      <div className="grid gap-4 lg:grid-cols-2">
        <Card
          title={t("Recent transactions")}
          actions={
            <Button size="sm" variant="ghost" onClick={() => go("ledger")}>
              {t("All")} <ArrowRight className="size-3" />
            </Button>
          }
        >
          {!recent.data ? (
            <Skeleton rows={4} />
          ) : recent.data.items.length ? (
            <ul>
              {recent.data.items.map((e) => (
                <li key={`${e.kind}-${e.id}`}>
                  <button
                    type="button"
                    onClick={() => (e.kind === "income" ? go("invoices", { open: e.id }) : go("ledger", { open: e.id }))}
                    className="flex w-full items-center gap-3 border-b border-line px-4 py-2.5 text-left last:border-0 hover:bg-subtle"
                  >
                    <CategoryDot color={e.kind === "income" ? "var(--pos)" : e.category_color} />
                    <span className="min-w-0 flex-1">
                      <span className="block truncate text-[13px]">{e.name}</span>
                      <span className="block truncate text-xs text-muted">
                        {date(e.date)} · {e.kind === "income" ? e.description : (e.category_name ?? t("Uncategorised"))}
                      </span>
                    </span>
                    <Amount cents={e.amount_cents} currency={e.currency} />
                  </button>
                </li>
              ))}
            </ul>
          ) : (
            <Empty title={t("No transactions yet")} text={t("Paid invoices and recorded expenses show up here.")} />
          )}
        </Card>

        <Card
          title={t("Recent time entries")}
          actions={
            <Button size="sm" variant="ghost" onClick={() => go("tracker")}>
              {t("All")} <ArrowRight className="size-3" />
            </Button>
          }
        >
          {!entries.data ? (
            <Skeleton rows={4} />
          ) : recentEntries.length ? (
            <ul>
              {recentEntries.map((e) => (
                <li key={e.id}>
                  <button
                    type="button"
                    onClick={() => go("tracker")}
                    className="flex w-full items-center gap-3 border-b border-line px-4 py-2.5 text-left last:border-0 hover:bg-subtle"
                  >
                    <span className="min-w-0 flex-1">
                      <span className="block truncate text-[13px]">{e.project_name}</span>
                      <span className="block truncate text-xs text-muted">
                        {date(e.date)} · {clockTime(e.start)}–{clockTime(e.stop!)}
                        {e.description ? ` · ${e.description}` : ""}
                      </span>
                    </span>
                    {e.billed && <Badge tone="pos">{t("billed")}</Badge>}
                    <span className="tabular w-16 text-right text-[13px]">{duration(e.duration)}</span>
                  </button>
                </li>
              ))}
            </ul>
          ) : (
            <Empty title={t("No time yet")} text={t("Tracked working time shows up here.")} />
          )}
        </Card>
      </div>
    </div>
  );
}
