import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Settings } from "../lib/api";
import { AppContext } from "../lib/nav";
import { setMasked } from "../lib/privacy";
import Overview from "./Overview";

const mocks = vi.hoisted(() => ({
  reportSummary: () =>
    Promise.resolve({
      revenue_cents: 1234500,
      expenses_cents: 234500,
      profit_cents: 1000000,
      prev_revenue_cents: 0,
      prev_expenses_cents: 0,
      open_invoices_cents: 0,
      open_invoices_count: 0,
      overdue_cents: 0,
      overdue_count: 0,
    }),
  listLedger: () =>
    Promise.resolve({
      items: [{ kind: "income", id: "i1", date: "2026-10-05", name: "Acme GmbH", description: "Invoice RE-1", amount_cents: 119000, currency: "EUR" }],
      total: 1,
    }),
  getTaxReport: () => Promise.resolve(null),
  taxForecast: () => Promise.resolve(null),
}));

vi.mock("@tauri-apps/api/event", () => ({ listen: () => Promise.resolve(() => {}) }));
vi.mock("../lib/api", async (original) => ({
  ...(await original<typeof import("../lib/api")>()),
  api: new Proxy(mocks, { get: (target, key: string) => target[key as keyof typeof mocks] ?? (() => Promise.resolve([])) }),
}));

const settings = { base_currency: "EUR", small_business: false, default_vat_rate: 19, company_name: "", limits: {} } as unknown as Settings;
const context = { route: "overview" as const, params: {}, go: () => {}, settings, reloadSettings: async () => {}, theme: "light" as const, setTheme: () => {} };

describe("Overview privacy toggle", () => {
  afterEach(() => act(() => setMasked(false)));

  it("hides amounts and customer names and shows them again", async () => {
    vi.stubGlobal("ResizeObserver", class { observe() {} disconnect() {} unobserve() {} });
    render(
      <AppContext.Provider value={context}>
        <Overview />
      </AppContext.Provider>,
    );
    expect(await screen.findByText("Acme GmbH")).toBeTruthy();
    expect(document.body.textContent).toMatch(/12.345,00|12,345.00/);

    fireEvent.click(screen.getByLabelText("Hide amounts and customer names"));
    const text = document.body.textContent ?? "";
    expect(text).not.toContain("Acme");
    expect(text).not.toMatch(/12.345|1.190|10.000/);
    expect(screen.getByLabelText("Show amounts and customer names").getAttribute("aria-pressed")).toBe("true");

    fireEvent.click(screen.getByLabelText("Show amounts and customer names"));
    expect(await screen.findByText("Acme GmbH")).toBeTruthy();
    vi.unstubAllGlobals();
  });
});
