import { render } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { MonthRow } from "../lib/api";
import { CashflowChart } from "./Cashflow";
import { smoothPath } from "./chart";

const x = (i: number) => i * 10;
const y = (v: number) => 100 - v;

describe("smoothPath", () => {
  it("draws nothing without values", () => {
    expect(smoothPath([], x, y)).toBe("");
  });

  it("joins two values with a straight line and skips gaps", () => {
    expect(smoothPath([1, null, 3], x, y)).toBe("M0,99 L20,97");
  });

  it("curves through every value from three values on", () => {
    const d = smoothPath([1, 5, 2, 8], x, y);
    expect(d.startsWith("M0,99")).toBe(true);
    expect(d.match(/C/g)).toHaveLength(3);
    expect(d.endsWith("30,92")).toBe(true);
  });
});

describe("CashflowChart", () => {
  const month = (key: string, revenue: number, expenses: number): MonthRow => ({
    month: key,
    revenue_cents: revenue,
    expenses_cents: expenses,
    profit_cents: revenue - expenses,
  });

  const profitPath = (months: MonthRow[]) => {
    const { container } = render(<CashflowChart months={months} highlight={() => true} currency="EUR" />);
    const path = container.querySelector('svg.block path[stroke="var(--profit)"]');
    return { d: path?.getAttribute("d") ?? "", circles: container.querySelectorAll("circle").length };
  };

  beforeEach(() => {
    vi.stubGlobal(
      "ResizeObserver",
      class {
        observe() {}
        disconnect() {}
      },
    );
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({ width: 800 } as DOMRect);
  });

  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it("shows the profit as a visible line when there is only one month", () => {
    const { d } = profitPath([month("2026-10", 500000, 100000)]);
    const [, x1, y1, x2, y2] = d.match(/^M([\d.]+),([\d.]+) L([\d.]+),([\d.]+)$/) ?? [];
    expect(Number(x2)).toBeGreaterThan(Number(x1));
    expect(y2).toBe(y1);
  });

  it("draws the bars in front of the profit line", () => {
    const { container } = render(<CashflowChart months={[month("2026-09", 300000, 50000), month("2026-10", 800000, 200000)]} highlight={() => true} currency="EUR" />);
    const line = container.querySelector('svg.block path[stroke="var(--profit)"]');
    const bar = container.querySelector('svg.block path[fill="url(#bar-income)"]');
    expect(line && bar && line.compareDocumentPosition(bar) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("keeps dimmed bars opaque so the profit line stays hidden behind them", () => {
    const { container } = render(<CashflowChart months={[month("2026-09", 300000, 50000), month("2026-10", 800000, 200000)]} highlight={() => false} currency="EUR" />);
    const line = container.querySelector('svg.block path[stroke="var(--profit)"]');
    const backings = [...container.querySelectorAll('svg.block path[fill="var(--panel)"]')];
    expect(backings).toHaveLength(4);
    for (const backing of backings) {
      expect(backing.closest("g[opacity]")).toBeNull();
      expect(line && line.compareDocumentPosition(backing) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    }
    expect(container.querySelector('path[fill="url(#bar-income)"]')?.closest("g[opacity]")?.getAttribute("opacity")).toBe("0.3");
  });

  it("keeps the single month's profit line in front of its bar", () => {
    const { container } = render(<CashflowChart months={[month("2026-10", 800000, 200000)]} highlight={() => true} currency="EUR" />);
    const line = container.querySelector('svg.block path[stroke="var(--profit)"]');
    const bar = container.querySelector('svg.block path[fill="url(#bar-income)"]');
    expect(line && bar && line.compareDocumentPosition(bar) & Node.DOCUMENT_POSITION_PRECEDING).toBeTruthy();
  });

  it("draws a curve without markers for several months", () => {
    const { d, circles } = profitPath([month("2026-08", 500000, 100000), month("2026-09", 300000, 50000), month("2026-10", 800000, 200000)]);
    expect(d).toContain("C");
    expect(circles).toBe(0);
  });
});
