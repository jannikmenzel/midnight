import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { centsToInput, money } from "../lib/format";
import { AppContext } from "../lib/nav";
import type { Invoice, InvoiceInput, Settings } from "../lib/api";
import { MomentProvider } from "../components/ActionMoment";
import { ToastProvider } from "../components/ui";
import { InvoiceEditor } from "./InvoiceEditor";

const mocks = vi.hoisted(() => ({
  getInvoice: vi.fn(),
  saveInvoice: vi.fn(),
  setInvoiceStatus: vi.fn(),
}));

vi.mock("../lib/api", async (original) => ({
  ...(await original<typeof import("../lib/api")>()),
  api: new Proxy(mocks, { get: (target, key: string) => target[key as keyof typeof mocks] ?? (() => Promise.resolve([])) }),
}));

const draft: Invoice = {
  id: "a",
  invoice_number: "RE-1",
  customer_id: null,
  customer_name: "Acme GmbH",
  customer_details: null,
  status: "draft",
  issue_date: "2026-10-20",
  due_date: "2026-11-03",
  currency: "EUR",
  line_items: [{ name: "Beratung", quantity: 1, price_cents: 10000, unit: null }],
  vat_rate: 0,
  discount_cents: 0,
  subtotal_cents: 10000,
  vat_cents: 0,
  amount_cents: 10000,
  note: null,
  internal_note: null,
  paid_at: null,
  file_path: null,
  created_at: "2026-10-20T10:00:00Z",
  order_number: null,
  customer_number: null,
  tracked_entries: 0,
};

const settings = { base_currency: "EUR", default_vat_rate: 0, default_payment_terms: 14, small_business: true, invoice_note: "" } as Settings;
const context = { route: "invoices" as const, params: {}, go: () => {}, settings, reloadSettings: async () => {}, theme: "light" as const, setTheme: () => {} };

async function openDraft() {
  render(
    <ToastProvider>
      <MomentProvider>
        <AppContext.Provider value={context}>
          <InvoiceEditor invoiceId="a" isNew={false} onClose={() => {}} onChanged={() => {}} />
        </AppContext.Provider>
      </MomentProvider>
    </ToastProvider>,
  );
  return (await screen.findByDisplayValue(centsToInput(10000))) as HTMLInputElement;
}

async function markSent() {
  fireEvent.click(screen.getByRole("button", { name: "More" }));
  fireEvent.click(await screen.findByText("Mark as sent"));
}

describe("marking an invoice as sent", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.getInvoice.mockResolvedValue(draft);
    mocks.saveInvoice.mockImplementation(async (input: InvoiceInput) => {
      const cents = input.line_items[0].price_cents;
      return { ...draft, line_items: input.line_items, subtotal_cents: cents, amount_cents: cents };
    });
    mocks.setInvoiceStatus.mockImplementation(async (_id: string, status: string) => {
      const saved = await mocks.saveInvoice.mock.results.slice(-1)[0]?.value;
      return { ...(saved ?? draft), status };
    });
  });

  it("saves pending edits first and confirms the saved amount", async () => {
    const price = await openDraft();
    fireEvent.change(price, { target: { value: centsToInput(12000) } });
    await markSent();

    await waitFor(() => expect(mocks.setInvoiceStatus).toHaveBeenCalledWith("a", "unpaid"));
    expect(mocks.saveInvoice).toHaveBeenCalledOnce();
    expect(mocks.saveInvoice.mock.calls[0][0].line_items[0].price_cents).toBe(12000);
    expect(mocks.saveInvoice.mock.invocationCallOrder[0]).toBeLessThan(mocks.setInvoiceStatus.mock.invocationCallOrder[0]);
    await waitFor(() => expect(document.querySelector(".moment .sr-only")?.textContent).toBe(money(12000)));
  });

  it("does not issue the invoice when saving fails", async () => {
    mocks.saveInvoice.mockRejectedValue(new Error("Disk full"));
    await openDraft();
    await markSent();

    expect(await screen.findByText("Disk full")).toBeTruthy();
    expect(mocks.setInvoiceStatus).not.toHaveBeenCalled();
    expect(document.querySelector(".moment")).toBeNull();
  });

  it("shows an error and no moment when the status change fails", async () => {
    mocks.setInvoiceStatus.mockRejectedValue(new Error("Locked"));
    await openDraft();
    await markSent();

    expect(await screen.findByText("Locked")).toBeTruthy();
    await act(async () => {});
    expect(document.querySelector(".moment")).toBeNull();
  });
});
