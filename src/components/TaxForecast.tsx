import { useEffect, useState, type ReactNode } from "react";
import { AlertTriangle, CalendarClock, Info } from "lucide-react";
import { api, errorMessage, type Forecast, type ForecastItem } from "../lib/api";
import { date, money } from "../lib/format";
import { useApp } from "../lib/nav";
import { Button, Card, Empty, InfoDialog, Kpi, Skeleton, cx, useToast } from "./ui";
import { locale, t } from "../lib/i18n";

const hrs = (h: number) => `${h.toLocaleString(locale(), { maximumFractionDigits: 1 })} ${t("hrs")}`;

const KIND_LABELS: Record<ForecastItem["kind"], string> = {
  open_invoice: t("Open invoice"),
  draft: t("Draft"),
  unbilled: t("Unbilled time"),
  project: t("Project budget"),
  recurring_expense: t("Recurring expense"),
};

function InfoSection({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section>
      <h3 className="mb-0.5 text-[13px] font-medium text-fg">{title}</h3>
      <p>{children}</p>
    </section>
  );
}


export function TaxForecast({ year }: { year: number }) {
  const { go } = useApp();
  const toast = useToast();
  const [data, setData] = useState<Forecast | null>(null);

  useEffect(() => {
    setData(null);
    api
      .taxForecast(year)
      .then(setData)
      .catch((e) => toast.error(errorMessage(e)));
  }, [year]);

  if (!data) return <Skeleton rows={6} />;
  const f = data;
  const o = f.outlook;
  const hasWarning = f.warnings.some((w) => w.level === "warn");

  return (
    <div className="flex flex-col gap-4">
      <Card>
        <div className="flex flex-col gap-4 p-5">
          {f.active ? (
            <div>
              <div className="text-xs text-muted">{o.balance_cents >= 0 ? t("Expected additional payment") : t("Expected refund")}</div>
              <div className={cx("tabular text-3xl font-semibold", o.balance_cents > 0 ? "text-neg" : o.balance_cents < 0 ? "text-pos" : "")}>{money(Math.abs(o.balance_cents))}</div>
            </div>
          ) : (
            <div>
              <h2 className="text-[13px] font-medium">{t("Year-end forecast {year}", { year })}</h2>
              <p className="text-[13px] text-muted">{t("This year is over. All figures are in the tax return.")}</p>
            </div>
          )}
          {f.warnings.length > 0 && (
            <div className={cx("divide-y divide-line rounded-md text-xs leading-relaxed", hasWarning ? "bg-warn/10" : "bg-subtle")}>
              {f.warnings.map((w, i) => (
                <p key={i} className="flex items-start gap-2 px-3 py-2">
                  {w.level === "warn" ? <AlertTriangle className="mt-0.5 size-3.5 shrink-0 text-warn" /> : <Info className="mt-0.5 size-3.5 shrink-0 text-muted" />}
                  {w.text}
                </p>
              ))}
            </div>
          )}
          {f.projects_without_end.length > 0 && (
            <div className="flex items-center gap-3 rounded-md bg-subtle px-3 py-2 text-xs leading-relaxed">
              <CalendarClock className="size-3.5 shrink-0 text-muted" />
              <span className="flex-1">{t("No expected end for {names}. The forecast assumes 31 Dec.", { names: f.projects_without_end.join(", ") })}</span>
              <Button size="sm" onClick={() => go("tracker")}>
                {t("Add duration")}
              </Button>
            </div>
          )}
        </div>
      </Card>

      {f.active && (
        <div className="grid grid-cols-2 gap-4 lg:grid-cols-3">
          <Kpi label={t("Booked revenue")} value={money(o.booked_income_cents)} />
          <Kpi label={t("Still expected")} value={`+ ${money(o.expected_income_cents)}`} />
          <Kpi label={t("Expected expenses")} value={`− ${money(o.expected_expense_cents)}`} />
          <Kpi label={t("Profit")} value={money(o.profit_cents)} />
          <Kpi label={t("Taxable")} value={money(o.taxable_income_cents)} />
          <Kpi label={t("Marginal rate")} value={`${Math.round(o.marginal_rate * 100)} %`} />
        </div>
      )}

      {f.active && f.plans.length > 0 && (
        <Card
          title={t("Project plan")}
          actions={
            <InfoDialog title={t("Calculation basis")}>
              <InfoSection title={t("Remaining effort")}>{t("The estimated hour budget of the project less the working time recorded so far.")}</InfoSection>
              <InfoSection title={t("Distribution")}>
                {t("The remaining effort is distributed evenly across the weeks up to the planned project end. Time already recorded in the current week counts towards that week's target, so time recorded as planned does not change the forecast for other months. Working ahead reduces the effort in the following weeks, a backlog increases it.")}
              </InfoSection>
              <InfoSection title={t("Capacity")}>
                {f.weekly_hours_limit > 0
                  ? t("All active projects, including non-billable ones, share the configured weekly working time of at most {limit}. If demand exceeds this capacity, it is reduced proportionally and moved to later weeks. Projects that are therefore completed after their planned end are highlighted.", { limit: hrs(f.weekly_hours_limit) })
                  : t("No weekly working time is configured. The forecast therefore assumes unlimited capacity.")}
              </InfoSection>
              <InfoSection title={t("Invoicing")}>
                {t("Hourly projects are invoiced monthly at the end of the month, with the final invoice issued on completion. Recorded time that has not yet been billed is included in the current month's invoice. Fixed-price projects are invoiced on completion for the outstanding amount.")}
              </InfoSection>
              <InfoSection title={t("Payment")}>
                {t("If a customer has paid at least two invoices, the median payment period of their most recent invoices is applied. Otherwise, the customer's payment terms or the default payment terms from the settings apply.")}
              </InfoSection>
              <InfoSection title={t("Limitations")}>{t("Public holidays and vacation periods are currently not taken into account.")}</InfoSection>
            </InfoDialog>
          }
        >
          <table className="w-full text-[13px]">
            <thead>
              <tr className="border-b border-line text-left text-xs whitespace-nowrap text-muted">
                <th className="px-4 py-2 font-normal">{t("Project")}</th>
                <th className="px-4 py-2 text-right font-normal">{t("Remaining")}</th>
                <th className="px-4 py-2 text-right font-normal">{t("Per week")}</th>
                <th className="px-4 py-2 font-normal">{t("Planned end")}</th>
                <th className="px-4 py-2 font-normal">{t("Expected finish")}</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-line">
              {f.plans.map((p, i) => (
                <tr key={i}>
                  <td className="w-full max-w-0 px-4 py-2">
                    <div className="truncate">{p.name}</div>
                    <div className="truncate text-xs text-muted">
                      {p.customer} · {p.fixed_price ? t("Fixed price") : t("Hourly")}
                      {!p.billable && ` · ${t("not billable")}`}
                    </div>
                  </td>
                  <td className="tabular px-4 py-2 text-right whitespace-nowrap">{hrs(p.remaining_hours)}</td>
                  <td className="tabular px-4 py-2 text-right whitespace-nowrap">{p.weekly_hours > 0 ? hrs(p.weekly_hours) : "–"}</td>
                  <td className="tabular-nums px-4 py-2 whitespace-nowrap text-muted">{p.end ? date(p.end) : "–"}</td>
                  <td className={cx("tabular-nums px-4 py-2 whitespace-nowrap", p.delayed && "text-warn")}>{p.finish ? date(p.finish) : "–"}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </Card>
      )}

      {f.active && (
        <Card title={t("Expected payments in {year}", { year })}>
          {f.items.length === 0 ? (
            <Empty title={t("No expected payments")} text={t("Open invoices, unbilled time and project budgets appear here.")} />
          ) : (
            <table className="w-full table-fixed text-[13px]">
              <thead>
                <tr className="border-b border-line text-left text-xs text-muted">
                  <th className="w-28 px-4 py-2 font-normal">{t("Expected on")}</th>
                  <th className="px-4 py-2 font-normal">{t("Item")}</th>
                  <th className="w-48 px-4 py-2 font-normal">{t("Source")}</th>
                  <th className="w-32 px-4 py-2 text-right font-normal">{t("Amount")}</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-line">
                {f.items.map((it, i) => (
                  <tr key={i}>
                    <td className="tabular-nums px-4 py-2 whitespace-nowrap">{date(it.date)}</td>
                    <td className="px-4 py-2">
                      <div className="truncate">{it.label}</div>
                      <div className="truncate text-xs text-muted">{it.customer}</div>
                      {it.detail && (
                        <div className="truncate text-xs text-muted" title={it.detail}>
                          {it.detail}
                        </div>
                      )}
                    </td>
                    <td className="px-4 py-2 text-xs whitespace-nowrap text-muted">{KIND_LABELS[it.kind]}</td>
                    <td className="tabular px-4 py-2 text-right">{money(it.amount_cents)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </Card>
      )}

      {f.active && f.expenses.length > 0 && (
        <Card title={t("Expected recurring expenses in {year}", { year })}>
          <table className="w-full table-fixed text-[13px]">
            <thead>
              <tr className="border-b border-line text-left text-xs text-muted">
                <th className="w-28 px-4 py-2 font-normal">{t("Expected on")}</th>
                <th className="px-4 py-2 font-normal">{t("Item")}</th>
                <th className="w-32 px-4 py-2 text-right font-normal">{t("Amount")}</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-line">
              {f.expenses.map((it, i) => (
                <tr key={i}>
                  <td className="tabular-nums px-4 py-2 whitespace-nowrap">{date(it.date)}</td>
                  <td className="px-4 py-2">
                    <div className="truncate">{it.label}</div>
                    <div className="truncate text-xs text-muted">{it.customer}</div>
                  </td>
                  <td className="tabular px-4 py-2 text-right">− {money(it.amount_cents)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </Card>
      )}
    </div>
  );
}
