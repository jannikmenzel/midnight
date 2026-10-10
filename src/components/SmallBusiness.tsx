import type { SmallBusiness } from "../lib/api";
import { money } from "../lib/format";
import { cx } from "./ui";
import { t } from "../lib/i18n";

export function SmallBusinessCard({ data, currency, className, projectedCents, bare }: { data: SmallBusiness; currency: string; className?: string; projectedCents?: number | null; bare?: boolean }) {
  const pct = data.limit_cents > 0 ? (data.revenue_cents / data.limit_cents) * 100 : 0;
  const over = data.revenue_cents > data.limit_cents;
  const overUpper = data.revenue_cents > data.upper_limit_cents;
  const previousOver = data.previous_year_revenue_cents > data.limit_cents;
  const bar = overUpper || over || previousOver ? "bg-neg" : pct >= 80 ? "bg-warn" : "bg-pos";
  const projected = projectedCents && projectedCents > data.revenue_cents ? projectedCents : null;
  const projectedPct = projected && data.limit_cents > 0 ? Math.min(100, (projected / data.limit_cents) * 100) : 0;
  const projectedOver = projected !== null && projected > data.limit_cents && !over;
  const note = overUpper
    ? t("Over {limit}: the small business scheme no longer applies, effective immediately.", { limit: money(data.upper_limit_cents, currency) })
    : previousOver
      ? t("Previous year over {limit}: the small business scheme does not apply in {year}.", { limit: money(data.limit_cents, currency), year: data.year })
      : over
      ? t("Over {limit}: the small business scheme no longer applies from {year}.", { limit: money(data.limit_cents, currency), year: data.year + 1 })
      : t("{amount} left until the limit for {year}.", { amount: money(data.limit_cents - data.revenue_cents, currency), year: data.year + 1 });

  return (
    <div className={cx("flex flex-col gap-2", !bare && "rounded-lg border border-line bg-panel p-4", className)}>
      <div className="flex items-baseline justify-between gap-2">
        <span className="text-xs text-muted">{t("Small business limit {year}", { year: data.year })}</span>
        <span className="tabular text-xs text-muted">{Math.round(pct)} %</span>
      </div>
      <div className="tabular text-xl font-semibold tracking-tight">
        {money(data.revenue_cents, currency)} <span className="text-[13px] font-normal text-muted">{t("of")} {money(data.limit_cents, currency)}</span>
      </div>
      <div className="relative h-1.5 w-full rounded-full bg-line" role="progressbar" aria-valuenow={Math.round(pct)} aria-valuemin={0} aria-valuemax={100}>
        {projected !== null && (
          <div className={cx("absolute inset-y-0 left-0 rounded-full opacity-30", projectedOver ? "bg-warn" : bar)} style={{ width: `${projectedPct}%` }} />
        )}
        <div className={cx("relative h-1.5 rounded-full", bar)} style={{ width: `${Math.min(100, pct)}%` }} />
      </div>
      {over || previousOver || overUpper ? (
        <span className="text-xs text-neg">{note}</span>
      ) : projectedOver ? (
        <span className="text-xs text-warn">{t("At the current pace about {amount} by year end – above the limit.", { amount: money(projected, currency, { compact: true }) })}</span>
      ) : (
        <span className="text-xs text-muted">{note}</span>
      )}
    </div>
  );
}
