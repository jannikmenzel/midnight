import { useEffect, useMemo, useState } from "react";
import { ChevronRight } from "lucide-react";
import { api, type Analysis, type MonthRow } from "../lib/api";
import { duration, money, MONTHS_SHORT, PERIODS, pctChange, periodRange, type PeriodKey } from "../lib/format";
import { useAsync } from "../lib/hooks";
import { useApp } from "../lib/nav";
import { YearCompareChart, type YearSeries } from "../components/YearCompare";
import { TaxReturn } from "../components/TaxReturn";
import { TaxForecast } from "../components/TaxForecast";
import { Card, CategoryDot, DateInput, Empty, Kpi, PageHeader, Segmented, Select, Skeleton, Tabs, cx } from "../components/ui";
import { t } from "../lib/i18n";

const CUSTOMER_LIMIT = 8;

const percent = (fraction: number) => `${Math.round(fraction * 100)} %`;

const hourly = (cents: number, secs: number) => (secs >= 1800 && cents > 0 ? Math.round((cents * 3600) / secs) : null);

function cumulative(rows: MonthRow[], year: number, metric: "income" | "profit", untilMonth: number): YearSeries {
  const byMonth = new Map(rows.filter((r) => r.month.startsWith(`${year}-`)).map((r) => [Number(r.month.slice(5, 7)) - 1, r]));
  if (byMonth.size === 0) return { year, values: Array(12).fill(null) };
  let sum = 0;
  const values = MONTHS_SHORT.map((_, i) => {
    if (i > untilMonth) return null;
    const r = byMonth.get(i);
    sum += r ? (metric === "income" ? r.revenue_cents : r.profit_cents) : 0;
    return sum;
  });
  return { year, values };
}

function Change({ current, previous, invert = false }: { current: number; previous: number; invert?: boolean }) {
  if (previous === 0 && current === 0) return null;
  if (previous === 0) return <span className="text-muted">{t("new")}</span>;
  const p = pctChange(current, previous)!;
  const good = invert ? p <= 0 : p >= 0;
  return (
    <span className={cx("tabular", Math.abs(p) < 1 ? "text-muted" : good ? "text-pos" : "text-neg")}>
      {p >= 0 ? "+" : ""}
      {p.toFixed(0)} %
    </span>
  );
}

function YearComparison({ year, currency }: { year: number; currency: string }) {
  const [metric, setMetric] = useState<"income" | "profit">("income");
  const monthly = useAsync(() => api.reportMonthly(`${year - 1}-01-01`, `${year}-12-31`), [year]);
  const now = new Date();
  const untilMonth = year === now.getFullYear() ? now.getMonth() : 11;
  const current = monthly.data ? cumulative(monthly.data, year, metric, untilMonth) : null;
  const previous = monthly.data ? cumulative(monthly.data, year - 1, metric, 11) : null;
  const curValue = current?.values[untilMonth] ?? null;
  const prevValue = previous?.values[untilMonth] ?? null;
  const span = untilMonth === 0 ? MONTHS_SHORT[0] : `${MONTHS_SHORT[0]}–${MONTHS_SHORT[untilMonth]}`;

  return (
    <Card
      title={t("Year comparison")}
      actions={
        <Segmented
          items={[
            { value: "income", label: t("Income") },
            { value: "profit", label: t("Profit") },
          ]}
          value={metric}
          onChange={setMetric}
        />
      }
    >
      <div className="flex flex-col gap-3 p-4">
        {!monthly.data || !current || !previous ? (
          <Skeleton height={290} />
        ) : curValue === null && prevValue === null ? (
          <Empty title={t("No data for {a} and {b}", { a: year - 1, b: year })} />
        ) : (
          <>
            <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
              <span className={cx("tabular text-xl font-semibold tracking-tight", (curValue ?? 0) < 0 && "text-neg")}>{money(curValue ?? 0, currency)}</span>
              <span className="text-[13px] text-muted">
                {span} {year}
                {prevValue !== null && prevValue !== 0 && (
                  <>
                    {" | "}
                    <Change current={curValue ?? 0} previous={prevValue} />{" "}
                    {t("vs. {span} {year}", { span, year: year - 1 })} <span className="tabular text-fg">{money(prevValue, currency)}</span>
                  </>
                )}
                {previous.values.every((v) => v === null) && (
                  <>
                    {" | "}
                    {t("No data for {year} yet", { year: year - 1 })}
                  </>
                )}
              </span>
            </div>
            <YearCompareChart current={current} previous={previous} currency={currency} />
          </>
        )}
      </div>
    </Card>
  );
}

function CustomerTable({ data, currency, onOpen }: { data: Analysis; currency: string; onOpen: (id: string) => void }) {
  const [all, setAll] = useState(false);
  const rows = data.customers;
  const total = rows.reduce((s, r) => s + r.revenue_cents, 0);
  const shown = all ? rows : rows.slice(0, CUSTOMER_LIMIT);
  const top = Math.max(1, ...rows.map((r) => r.revenue_cents));

  return (
    <Card title={t("Customers")} actions={<span className="text-xs text-muted">{t("Income, time and payment behaviour")}</span>}>
      {rows.length === 0 ? (
        <Empty title={t("No customer activity in this period")} text={t("Paid invoices, tracked time and open invoices show up here.")} />
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full min-w-[720px] text-[13px]">
            <thead>
              <tr className="border-b border-line text-left text-xs text-muted">
                <th className="px-4 py-2 font-normal">{t("Customer")}</th>
                <th className="w-[26%] px-4 py-2 font-normal">{t("Income")}</th>
                <th className="px-4 py-2 text-right font-normal">{t("Hours")}</th>
                <th className="px-4 py-2 text-right font-normal">{t("Per hour")}</th>
                <th className="px-4 py-2 text-right font-normal">{t("Paid after")}</th>
                <th className="px-4 py-2 text-right font-normal">{t("Outstanding")}</th>
                <th className="w-8" />
              </tr>
            </thead>
            <tbody>
              {shown.map((r) => {
                const rate = hourly(r.revenue_cents, r.tracked_secs);
                const clickable = !!r.customer_id;
                return (
                  <tr
                    key={r.key}
                    onClick={clickable ? () => onOpen(r.customer_id!) : undefined}
                    className={cx("group border-b border-line last:border-0", clickable && "cursor-pointer hover:bg-subtle")}
                  >
                    <td className="max-w-56 truncate px-4 py-2.5">{r.name}</td>
                    <td className="px-4 py-2.5">
                      <div className="flex items-center gap-3">
                        <div className="h-1.5 flex-1 rounded-full bg-line">
                          {r.revenue_cents > 0 && <div className="h-1.5 rounded-full bg-pos" style={{ width: `${(r.revenue_cents / top) * 100}%`, minWidth: 4 }} />}
                        </div>
                        <span className="tabular w-24 text-right">{money(r.revenue_cents, currency)}</span>
                        <span className="tabular w-10 text-right text-xs text-muted">{total > 0 ? percent(r.revenue_cents / total) : "–"}</span>
                      </div>
                    </td>
                    <td className="tabular px-4 py-2.5 text-right">{r.tracked_secs > 0 ? duration(r.tracked_secs) : <span className="text-muted">–</span>}</td>
                    <td className="tabular px-4 py-2.5 text-right">{rate !== null ? money(rate, currency) : <span className="text-muted">–</span>}</td>
                    <td className="tabular px-4 py-2.5 text-right">
                      {r.avg_payment_days !== null ? t("{n} days", { n: Math.round(r.avg_payment_days) }) : <span className="text-muted">–</span>}
                    </td>
                    <td className="tabular px-4 py-2.5 text-right">{r.open_cents > 0 ? money(r.open_cents, currency) : <span className="text-muted">–</span>}</td>
                    <td className="pr-3">{clickable && <ChevronRight className="size-3.5 text-muted opacity-0 transition-opacity group-hover:opacity-100" />}</td>
                  </tr>
                );
              })}
            </tbody>
          </table>
          {rows.length > CUSTOMER_LIMIT && (
            <button type="button" onClick={() => setAll(!all)} className="w-full border-t border-line px-4 py-2 text-left text-xs text-muted hover:bg-subtle hover:text-fg">
              {all ? t("Show fewer") : t("Show all {n} customers", { n: rows.length })}
            </button>
          )}
        </div>
      )}
    </Card>
  );
}

function CategoryList({ data, currency, onOpen }: { data: Analysis; currency: string; onOpen: (key: string) => void }) {
  const rows = data.categories;
  const top = Math.max(1, ...rows.map((r) => Math.max(r.total_cents, r.prev_cents)));
  const total = rows.reduce((s, r) => s + r.total_cents, 0);
  const prevTotal = rows.reduce((s, r) => s + r.prev_cents, 0);
  const compare = prevTotal > 0;

  return (
    <Card
      title={t("Expenses by category")}
      actions={
        compare && (
          <span className="text-xs text-muted">
            <Change current={total} previous={prevTotal} invert /> {t("vs. previous period")}
          </span>
        )
      }
    >
      {rows.length === 0 ? (
        <Empty title={t("No expenses in this period")} />
      ) : (
        <ul className="flex flex-col py-1">
          {rows.map((r) => (
            <li key={r.key}>
              <button type="button" onClick={() => onOpen(r.key)} className="flex w-full flex-col gap-1.5 px-4 py-2 text-left hover:bg-subtle">
                <div className="flex w-full items-baseline gap-2 text-[13px]">
                  <CategoryDot color={r.color} className="self-center" />
                  <span className="min-w-0 flex-1 truncate">{r.name}</span>
                  <span className="tabular">{money(r.total_cents, currency)}</span>
                  {compare && (
                    <span className="w-12 text-right text-xs">
                      <Change current={r.total_cents} previous={r.prev_cents} invert />
                    </span>
                  )}
                </div>
                <div className="relative h-1.5 w-full rounded-full bg-line">
                  {r.total_cents > 0 && <div className="h-1.5 rounded-full" style={{ width: `${(r.total_cents / top) * 100}%`, minWidth: 4, background: r.color }} />}
                  {r.prev_cents > 0 && (
                    <div className="absolute -top-0.5 h-2.5 w-0.5 rounded-full bg-fg" style={{ left: `calc(${(r.prev_cents / top) * 100}% - 1px)` }} />
                  )}
                </div>
              </button>
            </li>
          ))}
        </ul>
      )}
      {compare && (
        <p className="flex items-center gap-1.5 border-t border-line px-4 py-2 text-xs text-muted">
          <span className="inline-block h-2.5 w-0.5 rounded-full bg-fg" />
          {t("Marker shows the previous period")}
        </p>
      )}
    </Card>
  );
}

function ProjectList({ data, onOpen }: { data: Analysis; onOpen: (id: string) => void }) {
  const rows = data.projects;
  const totalSecs = rows.reduce((s, r) => s + r.tracked_secs, 0);
  const billableSecs = rows.filter((r) => r.billable).reduce((s, r) => s + r.tracked_secs, 0);
  const top = Math.max(1, ...rows.map((r) => r.tracked_secs));

  return (
    <Card title={t("Time by project")} actions={totalSecs > 0 && <span className="tabular text-xs text-muted">{duration(totalSecs)}</span>}>
      {rows.length === 0 ? (
        <Empty title={t("No time tracked in this period")} />
      ) : (
        <>
          <ul className="flex flex-col py-1">
            {rows.map((r) => (
              <li key={r.id}>
                <button type="button" onClick={() => onOpen(r.id)} className="flex w-full flex-col gap-1.5 px-4 py-2 text-left hover:bg-subtle">
                  <div className="flex w-full items-baseline gap-2 text-[13px]">
                    <span className="min-w-0 flex-1 truncate">
                      {r.name}
                      {r.customer_name && <span className="ml-1.5 text-xs text-muted">{r.customer_name}</span>}
                      {!r.billable && <span className="ml-1.5 text-xs text-muted">· {t("not billable")}</span>}
                    </span>
                    <span className="tabular">{duration(r.tracked_secs)}</span>
                    <span className="tabular w-10 text-right text-xs text-muted">{percent(r.tracked_secs / totalSecs)}</span>
                  </div>
                  <div className="h-1.5 w-full rounded-full bg-line">
                    <div className="h-1.5 rounded-full" style={{ width: `${(r.tracked_secs / top) * 100}%`, minWidth: 4, background: r.billable ? "var(--fg)" : "var(--muted)" }} />
                  </div>
                </button>
              </li>
            ))}
          </ul>
          <p className="border-t border-line px-4 py-2 text-xs text-muted">
            {t("{pct} billable", { pct: percent(billableSecs / totalSecs) })} <span className="tabular text-fg">{duration(billableSecs)}</span>
            {billableSecs < totalSecs && (
              <>
                {" | "}
                {t("Not billable")} <span className="tabular text-fg">{duration(totalSecs - billableSecs)}</span>
              </>
            )}
          </p>
        </>
      )}
    </Card>
  );
}

function Highlights({ data, currency, onFixedCosts }: { data: Analysis; currency: string; onFixedCosts: () => void }) {
  const withTime = data.customers.filter((c) => c.tracked_secs > 0);
  const timeSecs = withTime.reduce((s, c) => s + c.tracked_secs, 0);
  const rate = hourly(
    withTime.reduce((s, c) => s + c.revenue_cents, 0),
    timeSecs,
  );
  const income = data.customers.reduce((s, c) => s + c.revenue_cents, 0);
  const largest = data.customers[0];
  const share = largest && income > 0 ? largest.revenue_cents / income : null;

  return (
    <div className="grid grid-cols-2 gap-4 lg:grid-cols-4">
      <Kpi
        label={t("Income per hour")}
        value={
          rate !== null ? (
            <>
              {money(rate, currency)}
              {data.tax_share !== null && (
                <span
                  className="ml-2 text-base font-normal text-muted"
                  title={t("{pct} of revenue goes to income tax, solidarity surcharge and church tax (forecast {year})", { pct: percent(data.tax_share), year: data.tax_year })}
                >
                  / {money(Math.round(rate * (1 - data.tax_share)), currency)}
                </span>
              )}
            </>
          ) : (
            "–"
          )
        }
        sub={timeSecs > 0 ? t("{time} tracked for customers", { time: duration(timeSecs) }) : t("No time tracked for customers")}
      />
      <Kpi
        label={t("Avg. time to payment")}
        value={data.avg_payment_days !== null ? t("{n} days", { n: Math.round(data.avg_payment_days) }) : "–"}
        sub={data.paid_count > 0 ? t("{pct} paid on time", { pct: percent(data.on_time_count / data.paid_count) }) : t("No paid invoices")}
        tone={data.paid_count > 0 && data.on_time_count / data.paid_count < 0.7 ? "warn" : undefined}
      />
      <Kpi
        label={t("Largest customer")}
        value={share !== null ? percent(share) : "–"}
        sub={<span className="block truncate">{share !== null ? (share >= 0.5 ? t("{name} – high dependency", { name: largest.name }) : largest.name) : t("No income in this period")}</span>}
        tone={share !== null && share >= 0.5 ? "warn" : undefined}
      />
      <Kpi
        label={t("Fixed costs per month")}
        value={money(data.fixed_costs_monthly_cents, currency)}
        sub={data.fixed_costs_count > 0 ? t("{n} recurring expenses", { n: data.fixed_costs_count }) : t("No recurring expenses")}
        onClick={onFixedCosts}
      />
    </div>
  );
}

export default function Reports() {
  const { settings, go, params } = useApp();
  const [period, setPeriod] = useState<PeriodKey | "custom">("this_year");
  const [custom, setCustom] = useState(periodRange("this_year"));
  const [view, setView] = useState<"analysis" | "tax" | "forecast">(params.view === "tax" || params.view === "forecast" ? params.view : "analysis");
  const range = period === "custom" ? custom : periodRange(period);
  const cur = settings.base_currency;

  const thisYear = new Date().getFullYear();
  const [taxYear, setTaxYear] = useState(thisYear);
  const [forecastYear, setForecastYear] = useState(thisYear);
  const knownTaxYears = useAsync(() => api.taxYears(), []);
  const taxYears = (knownTaxYears.data ?? [thisYear]).filter((y) => y <= thisYear);
  const forecastYears = [thisYear + 1, thisYear, thisYear - 1, thisYear - 2];

  useEffect(() => {
    if (taxYears.length > 0 && !taxYears.includes(taxYear)) setTaxYear(taxYears[0]);
  }, [knownTaxYears.data]);

  const analysis = useAsync(() => api.reportAnalysis(range.from, range.to), [range.from, range.to]);
  const compareYear = useMemo(() => Math.min(new Date().getFullYear(), Number(range.to.slice(0, 4))), [range.to]);
  const a = analysis.data;

  return (
    <div className="flex flex-col gap-4">
      <PageHeader
        title={t("Reports")}
        subtitle={
          view === "tax"
            ? t("Your income tax at a glance.")
            : view === "forecast"
              ? t("Expected tax at year end.")
              : t("Where your money comes from, where it goes and what your time earns.")
        }
        actions={
          view !== "analysis" ? (
            <Select
              value={view === "tax" ? taxYear : forecastYear}
              onChange={(e) => (view === "tax" ? setTaxYear : setForecastYear)(Number(e.target.value))}
              className="w-28"
              aria-label={t("Tax year")}
            >
              {(view === "tax" ? taxYears : forecastYears).map((y) => (
                <option key={y} value={y}>
                  {y}
                </option>
              ))}
            </Select>
          ) : (
            <div className="flex items-center gap-2">
              {period === "custom" && (
                <>
                  <DateInput value={custom.from} onChange={(v) => setCustom({ ...custom, from: v })} className="w-36" aria-label={t("From")} />
                  <DateInput value={custom.to} onChange={(v) => setCustom({ ...custom, to: v })} className="w-36" aria-label={t("To")} />
                </>
              )}
              <Select value={period} onChange={(e) => setPeriod(e.target.value as PeriodKey | "custom")} className="w-44" aria-label={t("Period")}>
                {PERIODS.map((p) => (
                  <option key={p.key} value={p.key}>
                    {p.label}
                  </option>
                ))}
                <option value="custom">{t("Custom period")}</option>
              </Select>
            </div>
          )
        }
      />

      <Tabs
        items={[
          { value: "analysis", label: t("Analysis") },
          { value: "tax", label: t("Tax return") },
          { value: "forecast", label: t("Forecast") },
        ]}
        value={view}
        onChange={setView}
      />

      <div hidden={view !== "tax"}>
        <TaxReturn year={taxYear} />
      </div>
      <div hidden={view !== "forecast"}>
        <TaxForecast year={forecastYear} />
      </div>
      <div hidden={view !== "analysis"} className="flex flex-col gap-4">
        {a ? (
          <Highlights data={a} currency={cur} onFixedCosts={() => go("ledger", { tab: "recurring" })} />
        ) : (
          <div className="grid grid-cols-2 gap-4 lg:grid-cols-4">
            {Array.from({ length: 4 }).map((_, i) => (
              <Skeleton key={i} height={86} />
            ))}
          </div>
        )}

        {a ? <CustomerTable data={a} currency={cur} onOpen={(id) => go("customers", { open: id })} /> : <Skeleton height={220} />}

        <YearComparison year={compareYear} currency={cur} />

        <div className="grid gap-4 lg:grid-cols-2">
          {a ? (
            <>
              <CategoryList data={a} currency={cur} onOpen={(key) => go("ledger", { kind: "expense", category: key === "uncategorized" ? "" : key, from: range.from, to: range.to })} />
              <ProjectList data={a} onOpen={(id) => go("tracker", { open: id })} />
            </>
          ) : (
            <>
              <Skeleton height={240} />
              <Skeleton height={240} />
            </>
          )}
        </div>
      </div>
    </div>
  );
}
