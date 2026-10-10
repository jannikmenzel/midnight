import { useState } from "react";
import { money, MONTHS_SHORT } from "../lib/format";
import { t } from "../lib/i18n";
import { niceStep, PAD, useWidth, YGrid } from "./chart";

const CURRENT = "var(--pos)";
const PREVIOUS = "var(--muted)";
const RIGHT = 40;

export type YearSeries = { year: number; values: (number | null)[] };

function path(values: (number | null)[], x: (i: number) => number, y: (v: number) => number) {
  const pts = values.flatMap((v, i) => (v === null ? [] : [[x(i), y(v)] as const]));
  if (pts.length < 3) return pts.map(([px, py], i) => `${i === 0 ? "M" : "L"}${px},${py}`).join(" ");
  const slope = (k: number) => (pts[k + 1][1] - pts[k][1]) / (pts[k + 1][0] - pts[k][0]);
  const n = pts.length - 1;
  const tangents = pts.map((_, k) => {
    if (k === 0 || k === n) return 0;
    const [a, b] = [slope(k - 1), slope(k)];
    if (a * b <= 0) return 0;
    return Math.sign(a) * Math.min(Math.abs(a), Math.abs(b), Math.abs((a + b) / 2));
  });
  tangents[0] = (3 * slope(0) - tangents[1]) / 2;
  tangents[n] = (3 * slope(n - 1) - tangents[n - 1]) / 2;
  if (slope(0) * tangents[0] <= 0) tangents[0] = 0;
  if (slope(n - 1) * tangents[n] <= 0) tangents[n] = 0;
  let d = `M${pts[0][0]},${pts[0][1]}`;
  for (let k = 0; k < n; k++) {
    const h = (pts[k + 1][0] - pts[k][0]) / 3;
    d += ` C${pts[k][0] + h},${pts[k][1] + tangents[k] * h} ${pts[k + 1][0] - h},${pts[k + 1][1] - tangents[k + 1] * h} ${pts[k + 1][0]},${pts[k + 1][1]}`;
  }
  return d;
}

export function YearCompareChart({ current, previous, currency, height = 240 }: { current: YearSeries; previous: YearSeries; currency: string; height?: number }) {
  const [ref, width] = useWidth<HTMLDivElement>();
  const [hover, setHover] = useState<number | null>(null);

  const all = [...current.values, ...previous.values].filter((v): v is number => v !== null);
  const top = Math.max(0, ...all);
  const bottom = Math.min(0, ...all);
  const step = niceStep(Math.max(10000, top - bottom) / 4);
  const max = Math.max(step, Math.ceil(top / step) * step);
  const min = Math.floor(bottom / step) * step;
  const ticks: number[] = [];
  for (let v = min; v <= max + step / 2; v += step) ticks.push(v || 0);

  const plotW = Math.max(1, width - PAD.left - RIGHT);
  const plotH = height - PAD.top - PAD.bottom;
  const x = (i: number) => PAD.left + (i / 11) * plotW;
  const y = (v: number) => PAD.top + ((max - v) / (max - min || 1)) * plotH;
  const colW = plotW / 11;

  const lastIndex = (values: (number | null)[]) => values.reduce<number>((last, v, i) => (v === null ? last : i), -1);
  const ends = [
    { series: current, color: CURRENT, i: lastIndex(current.values) },
    { series: previous, color: PREVIOUS, i: lastIndex(previous.values) },
  ].filter((e) => e.i >= 0);
  const endY = ends.map((e) => y(e.series.values[e.i]!));
  if (ends.length === 2 && ends[0].i === ends[1].i && Math.abs(endY[0] - endY[1]) < 12) {
    const shift = (12 - Math.abs(endY[0] - endY[1])) / 2;
    const up = endY[0] <= endY[1] ? 0 : 1;
    endY[up] -= shift;
    endY[1 - up] += shift;
  }

  const hasPrevious = previous.values.some((v) => v !== null);
  const tipLeft = hover !== null && x(hover) > width / 2;
  const cur = hover !== null ? current.values[hover] : null;
  const prev = hover !== null ? previous.values[hover] : null;

  return (
    <div>
      <div ref={ref} className="relative" style={{ height }}>
        {width > 0 && (
          <svg
            width={width}
            height={height}
            className="block"
            role="img"
            aria-label={t("Cumulative comparison of {current} with {previous}", { current: current.year, previous: previous.year })}
            onMouseLeave={() => setHover(null)}
          >
            <YGrid ticks={ticks} y={y} width={width - RIGHT + PAD.right} currency={currency} />
            {MONTHS_SHORT.map((label, i) => (
              <g key={label} onMouseEnter={() => setHover(i)}>
                <rect x={x(i) - colW / 2} y={PAD.top} width={colW} height={plotH} fill="transparent" />
                <text x={x(i)} y={height - 5} textAnchor="middle" fontSize={11} fill={hover === i ? "var(--fg)" : "var(--muted)"}>
                  {label}
                </text>
              </g>
            ))}
            {min < 0 && <line x1={PAD.left} x2={width - RIGHT} y1={y(0)} y2={y(0)} stroke="var(--muted)" strokeOpacity={0.5} />}
            {hover !== null && <line x1={x(hover)} x2={x(hover)} y1={PAD.top} y2={PAD.top + plotH} stroke="var(--line)" strokeWidth={1} pointerEvents="none" />}
            <path d={path(previous.values, x, y)} fill="none" stroke={PREVIOUS} strokeWidth={2} strokeDasharray="4 4" strokeLinejoin="round" strokeLinecap="round" pointerEvents="none" />
            <path d={path(current.values, x, y)} fill="none" stroke={CURRENT} strokeWidth={2} strokeLinejoin="round" strokeLinecap="round" pointerEvents="none" />
            {ends.map((e, k) => (
              <g key={e.series.year} pointerEvents="none">
                <circle cx={x(e.i)} cy={y(e.series.values[e.i]!)} r={4} fill={e.color} stroke="var(--panel)" strokeWidth={2} />
                <text x={x(e.i) + 8} y={endY[k]} dy="0.32em" fontSize={11} fill="var(--muted)" className="tabular">
                  {e.series.year}
                </text>
              </g>
            ))}
            {hover !== null &&
              [
                { v: prev, color: PREVIOUS },
                { v: cur, color: CURRENT },
              ].map(
                (p, k) =>
                  p.v !== null && <circle key={k} cx={x(hover)} cy={y(p.v)} r={4} fill={p.color} stroke="var(--panel)" strokeWidth={2} pointerEvents="none" />,
              )}
          </svg>
        )}
        {hover !== null && (cur !== null || prev !== null) && (
          <div
            className="pointer-events-none absolute top-1 z-10 min-w-48 rounded-md border border-line bg-panel px-2.5 py-2 text-xs shadow-sm"
            style={tipLeft ? { right: width - x(hover) + 10 } : { left: x(hover) + 10 }}
          >
            <div className="mb-1 font-medium">{t("Jan – {month}", { month: MONTHS_SHORT[hover] })}</div>
            <TipRow label={String(current.year)} value={cur !== null ? money(cur, currency) : "–"} marker={<LineMark color={CURRENT} />} />
            {hasPrevious && <TipRow label={String(previous.year)} value={prev !== null ? money(prev, currency) : "–"} marker={<LineMark color={PREVIOUS} dashed />} />}
            {cur !== null && prev !== null && (
              <div className="mt-1 border-t border-line pt-1 font-medium">
                <TipRow label={t("Difference")} value={money(cur - prev, currency, { sign: true })} negative={cur < prev} />
              </div>
            )}
          </div>
        )}
      </div>
      <div className="mt-2 flex items-center gap-4 text-xs text-muted" style={{ paddingLeft: PAD.left }}>
        <span className="inline-flex items-center gap-1.5">
          <LineMark color={CURRENT} />
          {current.year}
        </span>
        {hasPrevious && (
          <span className="inline-flex items-center gap-1.5">
            <LineMark color={PREVIOUS} dashed />
            {previous.year}
          </span>
        )}
      </div>
    </div>
  );
}

function TipRow({ label, value, marker, negative }: { label: string; value: string; marker?: React.ReactNode; negative?: boolean }) {
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

function LineMark({ color, dashed }: { color: string; dashed?: boolean }) {
  return (
    <svg width="14" height="6" aria-hidden className="shrink-0">
      <line x1="1" x2="13" y1="3" y2="3" stroke={color} strokeWidth={2} strokeLinecap="round" strokeDasharray={dashed ? "3 3" : undefined} />
    </svg>
  );
}
