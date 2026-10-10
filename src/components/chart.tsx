import { useLayoutEffect, useRef, useState } from "react";
import { money } from "../lib/format";

export const PAD = { top: 8, right: 8, bottom: 22, left: 64 };

export function niceStep(raw: number) {
  const pow = 10 ** Math.floor(Math.log10(raw));
  const f = raw / pow;
  return (f <= 1 ? 1 : f <= 2 ? 2 : f <= 2.5 ? 2.5 : f <= 5 ? 5 : 10) * pow;
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
