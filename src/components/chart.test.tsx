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

  it("draws only bars, no profit line", () => {
    const { container } = render(<CashflowChart months={[month("2026-09", 300000, 50000), month("2026-10", 800000, 200000)]} highlight={() => true} currency="EUR" />);
    expect(container.querySelector('svg.block path[fill="none"]')).toBeNull();
    expect(container.querySelectorAll('svg.block path[fill="url(#bar-income)"]')).toHaveLength(2);
  });

  it("keeps dimmed bars opaque against the grid", () => {
    const { container } = render(<CashflowChart months={[month("2026-09", 300000, 50000), month("2026-10", 800000, 200000)]} highlight={() => false} currency="EUR" />);
    const backings = [...container.querySelectorAll('svg.block path[fill="var(--panel)"]')];
    expect(backings).toHaveLength(4);
    for (const backing of backings) expect(backing.closest("g[opacity]")).toBeNull();
    expect(container.querySelector('path[fill="url(#bar-income)"]')?.closest("g[opacity]")?.getAttribute("opacity")).toBe("0.3");
  });
});
