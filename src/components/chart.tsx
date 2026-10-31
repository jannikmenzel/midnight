import { useLayoutEffect, useRef, useState } from "react";
import { money } from "../lib/format";

export const PAD = { top: 8, right: 8, bottom: 22, left: 64 };

export function niceStep(raw: number) {
  const pow = 10 ** Math.floor(Math.log10(raw));
  const f = raw / pow;
  return (f <= 1 ? 1 : f <= 2 ? 2 : f <= 2.5 ? 2.5 : f <= 5 ? 5 : 10) * pow;
}

export function smoothPath(values: (number | null)[], x: (i: number) => number, y: (v: number) => number) {
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

export function useWidth<T extends HTMLElement>() {
  const ref = useRef<T>(null);
  const [width, setWidth] = useState(0);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const measure = (w: number) => w > 0 && setWidth(Math.floor(w));
    measure(el.getBoundingClientRect().width);
    const ro = new ResizeObserver(([entry]) => measure(entry.contentRect.width));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  return [ref, width] as const;
}

export function YGrid({ ticks, y, width, currency }: { ticks: number[]; y: (v: number) => number; width: number; currency: string }) {
  return (
    <>
      {ticks.map((t) => (
        <g key={t}>
          <line x1={PAD.left} x2={width - PAD.right} y1={y(t)} y2={y(t)} stroke="var(--line)" strokeDasharray={t === 0 ? undefined : "2 3"} />
          <text x={PAD.left - 10} y={y(t)} dy="0.32em" textAnchor="end" fontSize={11} fill="var(--muted)" className="tabular">
            {money(t, currency, { compact: true })}
          </text>
        </g>
      ))}
    </>
  );
}
