import { useEffect, useMemo, useState } from "react";
import { ask, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { Ban, CheckCircle2, Copy, Download, ExternalLink, FileCode, Plus, Send, Timer, Trash2, X } from "lucide-react";
import { api, errorMessage, type Customer, type Invoice, type InvoiceStatus, type Product, type Project, type TrackerInvoiceDraft } from "../lib/api";
import { addDays, centsToInput, date, hours, money, parseMoney, parseQuantity, quantityInput, todayIso } from "../lib/format";
import { useAsync } from "../lib/hooks";
import { useApp } from "../lib/nav";
import { ActionMenu, Button, CurrencySelect, DateInput, Field, IconButton, Input, Section, Select, Sheet, StatusBadge, Textarea, useToast } from "../components/ui";
import { PaidDialog } from "../components/PaidDialog";
import { UnitSelect } from "../components/fields";
import { t } from "../lib/i18n";

interface LineDraft {
  name: string;
  quantity: string;
  price: string;
  unit: string;
}

const emptyLine = (): LineDraft => ({ name: "", quantity: "1", price: "", unit: "" });
const qty = parseQuantity;

function totals(lines: LineDraft[], vatRate: number, discountCents: number) {
  const subtotal = lines.reduce((s, l) => s + Math.round(qty(l.quantity) * (parseMoney(l.price) ?? 0)), 0);
  const net = subtotal - discountCents;
  const vat = Math.round((net * vatRate) / 100);
  return { subtotal, vat, total: net + vat };
}

export function InvoiceEditor({
  invoiceId,
  isNew,
  prefill,
  onClose,
  onChanged,
}: {
  invoiceId: string | null;
  isNew: boolean;
  prefill?: TrackerInvoiceDraft | null;
  onClose: () => void;
  onChanged: () => void;
}) {
  const toast = useToast();
  const { settings } = useApp();
  const open = isNew || invoiceId !== null;
  const customers = useAsync(() => (open ? api.listCustomers() : Promise.resolve([] as Customer[])), [open]);
  const products = useAsync(() => (open ? api.listProducts() : Promise.resolve([] as Product[])), [open]);
  const projects = useAsync(() => (open ? api.listProjects() : Promise.resolve([] as Project[])), [open]);

  const [invoice, setInvoice] = useState<Invoice | null>(null);
  const [number, setNumber] = useState("");
  const [autoNumber, setAutoNumber] = useState("");
  const [customerId, setCustomerId] = useState("");
  const [issueDate, setIssueDate] = useState(todayIso());
  const [dueDate, setDueDate] = useState(addDays(todayIso(), settings.default_payment_terms));
  const [currency, setCurrency] = useState(settings.base_currency);
  const [lines, setLines] = useState<LineDraft[]>([emptyLine()]);
  const [vatRate, setVatRate] = useState(String(settings.small_business ? 0 : settings.default_vat_rate));
  const [discount, setDiscount] = useState("");
  const [note, setNote] = useState(settings.invoice_note);
  const [internalNote, setInternalNote] = useState("");
  const [orderNumber, setOrderNumber] = useState("");
  const [entryIds, setEntryIds] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [paidOpen, setPaidOpen] = useState(false);

  const applyInvoice = (inv: Invoice) => {
    setInvoice(inv);
    setNumber(inv.invoice_number);
    setCustomerId(inv.customer_id ?? "");
    setIssueDate(inv.issue_date);
    setDueDate(inv.due_date);
    setCurrency(inv.currency);
    setLines(inv.line_items.map((l) => ({ name: l.name, quantity: quantityInput(l.quantity, l.unit), price: centsToInput(l.price_cents), unit: l.unit ?? "" })));
    setVatRate(String(inv.vat_rate));
    setDiscount(inv.discount_cents ? centsToInput(inv.discount_cents) : "");
    setNote(inv.note ?? "");
    setInternalNote(inv.internal_note ?? "");
    setOrderNumber(inv.order_number ?? "");
    setEntryIds([]);
  };

  const suggestNumber = (issue: string) =>
    api
      .nextInvoiceNumber(issue)
      .then((n) => {
        setNumber(n);
        setAutoNumber(n);
      })
      .catch(() => setNumber(""));

  useEffect(() => {
    if (invoiceId) {
      api.getInvoice(invoiceId).then(applyInvoice).catch((e) => toast.error(errorMessage(e)));
    } else if (isNew) {
      setInvoice(null);
      const today = todayIso();
      setIssueDate(today);
      setDueDate(addDays(today, settings.default_payment_terms));
      setCurrency(prefill?.currency || settings.base_currency);
      setCustomerId(prefill?.customer_id ?? "");
      setLines(
        prefill?.lines.length
          ? prefill.lines.map((l) => ({ name: l.name, quantity: quantityInput(l.quantity, l.unit), price: centsToInput(l.price_cents), unit: l.unit ?? "" }))
          : [emptyLine()],
      );
      setEntryIds(prefill?.entry_ids ?? []);
      setVatRate(String(settings.small_business ? 0 : settings.default_vat_rate));
      setDiscount("");
      setNote(settings.invoice_note);
      setInternalNote("");
      setOrderNumber("");
      suggestNumber(today);
    }
  }, [invoiceId, isNew, prefill]);

  useEffect(() => {
    const c = isNew && !invoiceId && prefill?.customer_id ? customers.data?.find((x) => x.id === prefill.customer_id) : undefined;
    if (!c) return;
    if (c.default_payment_terms) setDueDate(addDays(todayIso(), c.default_payment_terms));
    if (!prefill?.currency && c.preferred_currency) setCurrency(c.preferred_currency);
  }, [isNew, invoiceId, prefill, customers.data]);

  const changeIssueDate = (value: string) => {
    if (!invoice && value && number === autoNumber && value.slice(0, 4) !== issueDate.slice(0, 4)) suggestNumber(value);
    setIssueDate(value);
  };

  const sum = useMemo(() => totals(lines, Number(vatRate.replace(",", ".")) || 0, parseMoney(discount) ?? 0), [lines, vatRate, discount]);

  const onCustomer = (id: string) => {
    setCustomerId(id);
    const c = customers.data?.find((x) => x.id === id);
    if (c?.default_payment_terms) setDueDate(addDays(issueDate, c.default_payment_terms));
    if (c?.preferred_currency) setCurrency(c.preferred_currency);
  };

  const setLine = (i: number, patch: Partial<LineDraft>) => setLines(lines.map((l, idx) => (idx === i ? { ...l, ...patch } : l)));

  const addProduct = (id: string) => {
    const p = products.data?.find((x) => x.id === id);
    if (!p) return;
    const line = { name: p.description ? `${p.name} – ${p.description}` : p.name, quantity: "1", price: centsToInput(p.price_cents), unit: p.unit ?? "" };
    setLines(lines.length === 1 && !lines[0].name && !lines[0].price ? [line] : [...lines, line]);
    api.touchProduct(id).catch(() => {});
  };

  const addTracker = async (projectId: string) => {
    try {
      const draft = await api.invoiceFromTracker(projectId);
      const newLines = draft.lines.map((l) => ({ name: l.name, quantity: quantityInput(l.quantity, l.unit), price: centsToInput(l.price_cents), unit: l.unit ?? "" }));
      setLines(lines.length === 1 && !lines[0].name ? newLines : [...lines, ...newLines]);
      setEntryIds([...new Set([...entryIds, ...draft.entry_ids])]);
      if (!customerId && draft.customer_id) onCustomer(draft.customer_id);
    } catch (e) {
      toast.error(errorMessage(e));
    }
  };

  const persist = async (status?: InvoiceStatus): Promise<Invoice | null> => {
    const parsed = lines
      .filter((l) => l.name.trim() || l.price.trim())
      .map((l) => ({ name: l.name.trim(), quantity: qty(l.quantity), price_cents: parseMoney(l.price) ?? 0, unit: l.unit || null }));
    if (!parsed.length) {
      toast.error(t("Add at least one line item"));
      return null;
    }
    setBusy(true);
    try {
      const saved = await api.saveInvoice({
        id: invoice?.id,
        invoice_number: number,
        customer_id: customerId || null,
        issue_date: issueDate,
        due_date: dueDate,
        currency,
        line_items: parsed,
        vat_rate: Number(vatRate.replace(",", ".")) || 0,
        discount_cents: parseMoney(discount) ?? 0,
        note: note || null,
        internal_note: internalNote || null,
        order_number: orderNumber.trim() || null,
        status: status ?? invoice?.status ?? "draft",
        tracker_entry_ids: entryIds,
      });
      applyInvoice(saved);
      onChanged();
      return saved;
    } catch (e) {
      toast.error(errorMessage(e));
      return null;
    } finally {
      setBusy(false);
    }
  };

  const withSaved = async (fn: (inv: Invoice) => Promise<void>) => {
    const inv = await persist();
    if (inv) {
      try {
        await fn(inv);
      } catch (e) {
        toast.error(errorMessage(e));
      }
    }
  };

  const setStatus = async (status: InvoiceStatus) => {
    if (!invoice) return;
    try {
      applyInvoice(await api.setInvoiceStatus(invoice.id, status));
      onChanged();
    } catch (e) {
      toast.error(errorMessage(e));
    }
  };

  const remove = async () => {
    if (!invoice) return;
    if (!(await ask(t("Delete invoice {number}? Linked time entries will be marked as unbilled again.", { number: invoice.invoice_number }), { title: t("Delete"), kind: "warning" }))) return;
    await api.deleteInvoice(invoice.id);
    onChanged();
    onClose();
  };

  const locked = invoice !== null && ["paid", "canceled"].includes(invoice.status);

  return (
    <>
      <Sheet
        open={open}
        onClose={onClose}
        extraWide
        title={
          <span className="flex items-center gap-2">
            {invoice ? t("Invoice {number}", { number: invoice.invoice_number }) : t("New invoice")}
            {invoice && <StatusBadge status={invoice.status} />}
          </span>
        }
        footer={
          <>
            {invoice && (
              <Button variant="danger" icon={<Trash2 className="size-3.5" />} onClick={remove} className="mr-auto">
                {t("Delete")}
              </Button>
            )}
            <Button onClick={onClose}>{t("Close")}</Button>
            <Button variant="primary" onClick={() => persist()} loading={busy}>
              {t("Save")}
            </Button>
          </>
        }
      >
        <div className="flex flex-col gap-6">
          {invoice && (
            <div className="flex flex-wrap items-center gap-2 rounded-lg border border-line bg-subtle/60 px-3 py-2">
              <div className="min-w-0 flex-1 text-xs text-muted">
                {invoice.paid_at ? (
                  <span className="text-pos">{t("Paid on {date}", { date: date(invoice.paid_at) })}</span>
                ) : invoice.status === "draft" ? (
                  t("Draft")
                ) : invoice.status === "canceled" ? (
                  t("Canceled")
                ) : (
                  <span className={invoice.status === "overdue" ? "text-neg" : undefined}>{t("Due on {date}", { date: date(invoice.due_date) })}</span>
                )}
              </div>
              <Button
                size="sm"
                icon={<ExternalLink className="size-3" />}
                onClick={() =>
                  withSaved(async (inv) => {
                    const pdf = await api.renderInvoicePdf(inv.id);
                    if (pdf.e_invoice_warning) toast.warn(pdf.e_invoice_warning);
                    await api.openPath(pdf.path);
                  })
                }
              >
                PDF
              </Button>
              {invoice.tracked_entries > 0 && (
                <Button
                  size="sm"
                  icon={<Timer className="size-3" />}
                  onClick={() =>
                    withSaved(async (inv) => {
                      const path = await api.renderInvoiceTimesheet(inv.id);
                      if (path) await api.openPath(path);
                    })
                  }
                >
                  {t("Time tracking")}
                </Button>
              )}
              {invoice.status !== "paid" && invoice.status !== "canceled" && (
                <Button size="sm" icon={<CheckCircle2 className="size-3" />} onClick={() => withSaved(async () => setPaidOpen(true))}>
                  {t("Paid")}
                </Button>
              )}
              <ActionMenu
                items={[
                  {
                    label: t("Save PDF"),
                    icon: <Download className="size-3.5" />,
                    onClick: () =>
                      withSaved(async (inv) => {
                        const target = await saveDialog({ defaultPath: `${t("Invoice")}_${inv.invoice_number}.pdf`, filters: [{ name: "PDF", extensions: ["pdf"] }] });
                        if (target) {
                          const warning = await api.exportInvoicePdf(inv.id, target);
                          toast.success(t("PDF saved"));
                          if (warning) toast.warn(warning);
                        }
                      }),
                  },
                  {
                    label: t("Save XRechnung"),
                    icon: <FileCode className="size-3.5" />,
                    onClick: () =>
                      withSaved(async (inv) => {
                        const target = await saveDialog({ defaultPath: `XRechnung_${inv.invoice_number}.xml`, filters: [{ name: "XRechnung", extensions: ["xml"] }] });
                        if (target) {
                          await api.exportInvoiceXRechnung(inv.id, target);
                          toast.success(t("XRechnung saved"));
                        }
                      }),
                  },
                  invoice.status === "draft" && { label: t("Mark as sent"), icon: <Send className="size-3.5" />, onClick: () => setStatus("unpaid") },
                  invoice.status === "paid" && { label: t("Change payment date"), icon: <CheckCircle2 className="size-3.5" />, onClick: () => setPaidOpen(true) },
                  invoice.status === "paid" && { label: t("Undo payment"), icon: <X className="size-3.5" />, onClick: () => setStatus("unpaid") },
                  {
                    label: t("Duplicate"),
                    icon: <Copy className="size-3.5" />,
                    onClick: async () => {
                      const copy = await api.duplicateInvoice(invoice.id);
                      applyInvoice(copy);
                      onChanged();
                      toast.success(t("Copy {number} created", { number: copy.invoice_number }));
                    },
                  },
                  invoice.status !== "canceled" &&
                    invoice.status !== "paid" && {
                      label: t("Cancel invoice"),
                      icon: <Ban className="size-3.5" />,
                      danger: true,
                      onClick: async () => {
                        const message = t("Cancel invoice {number}? Canceled invoices can no longer be edited.", { number: invoice.invoice_number });
                        if (await ask(message, { title: t("Cancel invoice"), kind: "warning" })) await setStatus("canceled");
                      },
                    },
                ]}
              />
            </div>
          )}

          <Section title={t("Recipient")}>
            <Field label={t("Customer")}>
              <Select value={customerId} onChange={(e) => onCustomer(e.target.value)} disabled={locked}>
                <option value="">{t("No customer")}</option>
                {customers.data?.map((c) => (
                  <option key={c.id!} value={c.id!}>
                    {c.name}
                  </option>
                ))}
              </Select>
            </Field>
          </Section>

          <Section title={t("Invoice details")}>
            <div className="grid grid-cols-6 gap-4">
              <Field label={t("Invoice number")} className="col-span-3">
                <Input value={number} onChange={(e) => setNumber(e.target.value)} disabled={locked} className="tabular-nums" />
              </Field>
              <Field label={t("Order no.")} className="col-span-3">
                <Input value={orderNumber} onChange={(e) => setOrderNumber(e.target.value)} placeholder={t("optional")} disabled={locked} />
              </Field>
              <Field label={t("Invoice date")} className="col-span-2">
                <DateInput value={issueDate} onChange={changeIssueDate} disabled={locked} />
              </Field>
              <Field label={t("Due on")} className="col-span-2">
                <DateInput value={dueDate} onChange={(v) => setDueDate(v)} disabled={locked} />
              </Field>
              <Field label={t("Currency")} className="col-span-2">
                <CurrencySelect value={currency} onChange={(e) => setCurrency(e.target.value)} disabled={locked} />
              </Field>
            </div>
          </Section>

          <Section title={t("Line items")}>
            <div className="overflow-hidden rounded-lg border border-line">
              <div className="grid grid-cols-[minmax(0,1fr)_72px_72px_104px_104px_32px] gap-2 border-b border-line bg-subtle px-3 py-2 text-xs font-medium text-muted">
                <span>{t("Description")}</span>
                <span className="text-right">{t("Qty")}</span>
                <span>{t("Unit")}</span>
                <span className="text-right">{t("Price")}</span>
                <span className="text-right">{t("Total")}</span>
                <span />
              </div>
              <div className="divide-y divide-line">
                {lines.map((l, i) => (
                  <div key={i} className="grid grid-cols-[minmax(0,1fr)_72px_72px_104px_104px_32px] items-center gap-2 px-3 py-2">
                    <Input value={l.name} onChange={(e) => setLine(i, { name: e.target.value })} placeholder={t("Service, e.g. Concept (01/09–15/09/26)")} disabled={locked} />
                    <Input value={l.quantity} onChange={(e) => setLine(i, { quantity: e.target.value })} className="tabular-nums text-right" disabled={locked} />
                    <UnitSelect value={l.unit} onChange={(unit) => setLine(i, { unit })} disabled={locked} />
                    <Input value={l.price} onChange={(e) => setLine(i, { price: e.target.value })} placeholder={centsToInput(0)} className="tabular-nums text-right" disabled={locked} />
                    <span className="tabular text-right text-[13px] font-medium">{money(Math.round(qty(l.quantity) * (parseMoney(l.price) ?? 0)), currency)}</span>
                    <IconButton label={t("Remove line item")} onClick={() => setLines(lines.length > 1 ? lines.filter((_, idx) => idx !== i) : [emptyLine()])} disabled={locked}>
                      <X className="size-3.5" />
                    </IconButton>
                  </div>
                ))}
              </div>
              {!locked && (
                <div className="flex flex-wrap items-center gap-2 border-t border-line bg-subtle/60 px-3 py-2">
                  <Button size="sm" variant="ghost" icon={<Plus className="size-3" />} onClick={() => setLines([...lines, emptyLine()])}>
                    {t("Line item")}
                  </Button>
                  {(products.data?.length ?? 0) > 0 && (
                    <Select value="" onChange={(e) => addProduct(e.target.value)} className="h-7 w-44 text-xs" aria-label={t("Add product")}>
                      <option value="">{t("+ Product…")}</option>
                      {products.data?.map((p) => (
                        <option key={p.id!} value={p.id!}>
                          {p.name} ({money(p.price_cents, p.currency)})
                        </option>
                      ))}
                    </Select>
                  )}
                  {(projects.data?.filter((p) => p.unbilled_seconds > 0).length ?? 0) > 0 && (
                    <Select value="" onChange={(e) => e.target.value && addTracker(e.target.value)} className="h-7 w-56 text-xs" aria-label={t("Add tracked time")}>
                      <option value="">{t("+ Add tracked time…")}</option>
                      {projects.data
                        ?.filter((p) => p.unbilled_seconds > 0)
                        .map((p) => (
                          <option key={p.id} value={p.id}>
                            {p.name} ({hours(p.unbilled_seconds)} {t("hrs")})
                          </option>
                        ))}
                    </Select>
                  )}
                  {entryIds.length > 0 && (
                    <span className="ml-auto inline-flex items-center gap-1 text-xs text-muted">
                      <Timer className="size-3" /> {t("{n} time entries will be billed", { n: entryIds.length })}
                    </span>
                  )}
                </div>
              )}
            </div>
          </Section>

          <div className="grid grid-cols-[minmax(0,1fr)_300px] items-start gap-6">
            <Section title={t("Texts")}>
              <div className="flex flex-col gap-4">
                <Field label={t("Note on the invoice")}>
                  <Textarea value={note} onChange={(e) => setNote(e.target.value)} disabled={locked} />
                </Field>
                <Field label={t("Internal note")} hint={t("Does not appear on the PDF")}>
                  <Textarea value={internalNote} onChange={(e) => setInternalNote(e.target.value)} />
                </Field>
              </div>
            </Section>

            <Section title={t("Totals")}>
              <div className="flex flex-col gap-2.5 rounded-lg border border-line bg-subtle/60 p-4 text-[13px]">
                <div className="flex items-center justify-between">
                  <span className="text-muted">{t("Subtotal")}</span>
                  <span className="tabular">{money(sum.subtotal, currency)}</span>
                </div>
                <div className="flex items-center justify-between gap-3">
                  <span className="text-muted">{t("Discount")}</span>
                  <Input value={discount} onChange={(e) => setDiscount(e.target.value)} placeholder={centsToInput(0)} disabled={locked} className="h-7 w-24 text-right tabular-nums" aria-label={t("Discount")} />
                </div>
                <div className="flex items-center justify-between gap-3">
                  <span className="inline-flex items-center gap-1.5 text-muted">
                    {t("VAT")}
                    <Input value={vatRate} onChange={(e) => setVatRate(e.target.value)} disabled={locked} className="h-7 w-14 text-right tabular-nums" aria-label={t("VAT rate in percent")} />
                    %
                  </span>
                  <span className="tabular">{money(sum.vat, currency)}</span>
                </div>
                <div className="mt-1 flex items-baseline justify-between border-t border-line pt-3">
                  <span className="font-medium">{t("Total")}</span>
                  <span className="tabular text-lg font-semibold">{money(sum.total, currency)}</span>
                </div>
                {settings.small_business && Number(vatRate.replace(",", ".")) === 0 &&<span className="text-xs text-muted">{t("The § 19 UStG notice is printed on the PDF.")}</span>}
              </div>
            </Section>
          </div>
        </div>
      </Sheet>
      {invoice && (
        <PaidDialog
          invoice={invoice}
          open={paidOpen}
          onClose={() => setPaidOpen(false)}
          onPaid={(inv) => {
            applyInvoice(inv);
            onChanged();
          }}
        />
      )}
    </>
  );
}
