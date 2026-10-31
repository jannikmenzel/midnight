import { useState } from "react";
import type { MonthRow } from "../lib/api";
import { money, monthLabel } from "../lib/format";
import { t } from "../lib/i18n";
import { niceStep, PAD, smoothPath, useWidth, YGrid } from "./chart";

const INCOME = "var(--pos)";
const EXPENSES = "var(--neg)";

function GrainBar({ d, fill }: { d: string; fill: string }) {
  return (
    <>
      <path d={d} fill={fill} />
      <path d={d} fill="url(#bar-grain)" opacity={0.24} pointerEvents="none" />
    </>
  );
}

function bar(x: number, w: number, base: number, end: number) {
  const h = Math.max(2, Math.abs(end - base));
  const r = Math.min(6, w / 2, h);
  const up = end < base;
  const tip = up ? base - h : base + h;
  const inner = up ? tip + r : tip - r;
  return [
    `M${x},${base}`,
    `L${x},${inner}`,
    `Q${x},${tip} ${x + r},${tip}`,
    `L${x + w - r},${tip}`,
    `Q${x + w},${tip} ${x + w},${inner}`,
    `L${x + w},${base}`,
    "Z",
  ].join(" ");
}

export function CashflowChart({
  months,
  highlight,
  currency,
  height = 240,
}: {
  months: MonthRow[];
  highlight: (month: string) => boolean;
  currency: string;
  height?: number;
}) {
  const [ref, width] = useWidth<HTMLDivElement>();
  const [hover, setHover] = useState<number | null>(null);

  const top = Math.max(0, ...months.map((m) => m.revenue_cents));
  const bottom = Math.max(0, ...months.map((m) => m.expenses_cents));
  const step = niceStep(Math.max(10000, top, bottom) / 3);
  const max = Math.max(step, Math.ceil(top / step) * step);
  const min = bottom > 0 ? -Math.max(bottom * 1.15, max * 0.12) : 0;
  const ticks: number[] = [];
  for (let v = Math.ceil(min / step) * step; v <= max + step / 2; v += step) ticks.push(v || 0);

  const n = Math.max(months.length, 1);
  const plotW = Math.max(1, width - PAD.left - PAD.right);
  const plotH = height - PAD.top - PAD.bottom;
  const groupW = plotW / n;
  const barW = Math.max(3, Math.min(20, groupW * 0.5));
  const y = (v: number) => PAD.top + ((max - v) / (max - min || 1)) * plotH;
  const zero = y(0);
  const groupX = (i: number) => PAD.left + i * groupW;
  const labelEvery = Math.ceil(n / Math.max(1, Math.floor(plotW / 48)));
  const tipLeft = hover !== null && groupX(hover) + groupW / 2 > width / 2;
  const active = hover !== null ? months[hover] : null;
  const center = (i: number) => groupX(i) + groupW / 2;
  const profitLine =
    months.length === 1
      ? `M${center(0) - barW / 2 - 4},${y(months[0].profit_cents)} L${center(0) + barW / 2 + 4},${y(months[0].profit_cents)}`
      : smoothPath(months.map((m) => m.profit_cents), center, y);
  const profitPath = <path d={profitLine} fill="none" stroke="var(--profit)" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round" pointerEvents="none" />;

  return (
    <div>
      <div className="mb-2 flex items-center justify-end gap-4 pr-2 text-xs text-muted">
        <span className="inline-flex items-center gap-1.5">
          <Swatch color={INCOME} />
          {t("Income")}
        </span>
        <span className="inline-flex items-center gap-1.5">
          <Swatch color={EXPENSES} />
          {t("Expenses")}
        </span>
        <span className="inline-flex items-center gap-1.5">
          <ProfitMark />
          {t("Profit")}
        </span>
      </div>
      <div ref={ref} className="relative" style={{ height }}>
        {width > 0 && (
          <svg
            width={width}
            height={height}
            className="block"
            role="img"
            aria-label={t("Cash flow chart: income above, expenses below the line")}
            onMouseLeave={() => setHover(null)}
          >
            <defs>
              <pattern id="bar-grain" patternUnits="userSpaceOnUse" width={256} height={256}>
                <image href="/grain.png" width={256} height={256} />
              </pattern>
              <linearGradient id="bar-income" x1="0" y1="0" x2="0" y2="1">
                <stop offset="0" stopColor={INCOME} />
                <stop offset="1" stopColor="var(--pos-soft)" />
              </linearGradient>
              <linearGradient id="bar-expenses" x1="0" y1="0" x2="0" y2="1">
                <stop offset="0" stopColor="var(--neg-soft)" />
                <stop offset="1" stopColor={EXPENSES} />
              </linearGradient>
            </defs>
            <YGrid ticks={ticks} y={y} width={width} currency={currency} />
            {hover !== null && <rect x={groupX(hover)} y={PAD.top} width={groupW} height={plotH} rx={8} fill="var(--subtle)" pointerEvents="none" />}
            {months.length > 1 && profitPath}
            {months.map((m, i) => {
              const x = groupX(i) + (groupW - barW) / 2;
              const on = highlight(m.month);
              const income = m.revenue_cents > 0 ? bar(x, barW, zero - 1, y(m.revenue_cents)) : null;
              const expenses = m.expenses_cents > 0 ? bar(x, barW, zero + 1, y(-m.expenses_cents)) : null;
              return (
                <g key={m.month} onMouseEnter={() => setHover(i)}>
                  <rect x={groupX(i)} y={PAD.top} width={groupW} height={plotH} fill="transparent" />
                  {income && <path d={income} fill="var(--panel)" />}
                  {expenses && <path d={expenses} fill="var(--panel)" />}
                  <g opacity={hover === i || (on && hover === null) ? 1 : on ? 0.55 : 0.3}>
                    {income && <GrainBar d={income} fill="url(#bar-income)" />}
                    {expenses && <GrainBar d={expenses} fill="url(#bar-expenses)" />}
                    {(i % labelEvery === 0 || hover === i) && (
                      <text x={groupX(i) + groupW / 2} y={height - 5} textAnchor="middle" fontSize={11} fill={hover === i ? "var(--fg)" : "var(--muted)"}>
                        {monthLabel(m.month)}
                      </text>
                    )}
                  </g>
                </g>
              );
            })}
            <line x1={PAD.left} x2={width - PAD.right} y1={zero} y2={zero} stroke="var(--muted)" strokeOpacity={0.5} />
            {months.length === 1 && profitPath}
          </svg>
        )}
        {active && hover !== null && (
          <div
            className="floating pointer-events-none absolute top-1 z-10 min-w-44 rounded-md border border-line bg-panel px-2.5 py-2 text-xs"
            style={tipLeft ? { right: width - groupX(hover) + 6 } : { left: groupX(hover) + groupW + 6 }}
          >
            <div className="mb-1 font-medium">{monthLabel(active.month)}</div>
            <TipRow label={t("Income")} value={money(active.revenue_cents, currency)} marker={<Swatch color={INCOME} />} />
            <TipRow label={t("Expenses")} value={money(active.expenses_cents, currency)} marker={<Swatch color={EXPENSES} />} />
            <div className="mt-1 border-t border-line pt-1 font-medium">
              <TipRow label={t("Profit")} value={money(active.profit_cents, currency)} marker={<ProfitMark />} negative={active.profit_cents < 0} />
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

function TipRow({ label, value, marker, negative }: { label: string; value: string; marker: React.ReactNode; negative?: boolean }) {
  return (
    <div className="flex items-center justify-between gap-4">
      <span className="inline-flex items-center gap-1.5 text-muted">
        {marker}
        {label}
      </span>
      <span className={negative ? "tabular text-neg" : "tabular"}>{value}</span>
    </div>
  );
}

function Swatch({ color }: { color: string }) {
  return <span className="inline-block size-2 rounded-sm" style={{ background: color }} />;
}

function ProfitMark() {
  return (
    <svg width="12" height="6" aria-hidden className="shrink-0">
      <line x1="1" x2="11" y1="3" y2="3" stroke="var(--profit)" strokeWidth={2} strokeLinecap="round" />
    </svg>
  );
}
