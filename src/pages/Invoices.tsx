import { useEffect, useState } from "react";
import { FileText, Package, Plus } from "lucide-react";
import { api, errorMessage, type Product, type TrackerInvoiceDraft } from "../lib/api";
import { centsToInput, date, HOURS, money, parseMoney } from "../lib/format";
import { useAsync, useDebounced } from "../lib/hooks";
import { useApp } from "../lib/nav";
import { Button, Card, CurrencySelect, Dialog, Empty, ErrorNote, Field, Input, Kpi, PageHeader, Select, Skeleton, StatusBadge, Tabs, useToast } from "../components/ui";
import { InvoiceEditor } from "./InvoiceEditor";
import { UnitSelect, useUnits } from "../components/fields";
import { t } from "../lib/i18n";

type Tab = "invoices" | "products";

function ProductsTab() {
  const toast = useToast();
  const { settings } = useApp();
  const products = useAsync(() => api.listProducts(), []);
  const units = useUnits();
  const [draft, setDraft] = useState<(Omit<Product, "price_cents" | "tax_rate"> & { price: string; tax: string }) | null>(null);

  const openProduct = (p: Product) => setDraft({ ...p, price: centsToInput(p.price_cents), tax: p.tax_rate?.toString() ?? "" });

  const save = async () => {
    if (!draft) return;
    try {
      await api.saveProduct({
        id: draft.id,
        name: draft.name,
        description: draft.description || null,
        price_cents: parseMoney(draft.price) ?? 0,
        currency: draft.currency,
        unit: draft.unit || null,
        tax_rate: draft.tax ? Number(draft.tax.replace(",", ".")) : null,
      });
      setDraft(null);
      products.reload();
    } catch (e) {
      toast.error(errorMessage(e));
    }
  };

  return (
    <Card
      title={t("Products & services")}
      actions={
        <Button size="sm" icon={<Plus className="size-3" />} onClick={() => setDraft({ name: "", description: "", price: "", currency: settings.base_currency, unit: HOURS, tax: "" })}>
          {t("New")}
        </Button>
      }
    >
      {!products.data ? (
        <Skeleton rows={3} />
      ) : products.data.length === 0 ? (
        <Empty icon={<Package className="size-6" />} title={t("No products")} text={t("Create recurring line items such as hourly rates or flat fees once and reuse them in every invoice.")} />
      ) : (
        <table className="w-full text-[13px]">
          <thead>
            <tr className="border-b border-line text-left text-xs text-muted">
              <th className="py-2.5 pl-4 font-medium">{t("Name")}</th>
              <th className="py-2.5 font-medium">{t("Unit")}</th>
              <th className="py-2.5 text-right font-medium">{t("Price")}</th>
              <th className="py-2.5 pr-4 text-right font-medium">{t("Used")}</th>
            </tr>
          </thead>
          <tbody>
            {products.data?.map((p) => (
              <tr key={p.id!} className="cursor-pointer border-b border-line last:border-0 hover:bg-subtle" onClick={() => openProduct(p)}>
                <td className="py-2.5 pl-4">
                  {p.name}
                  {p.description && <div className="text-xs text-muted">{p.description}</div>}
                </td>
                <td className="py-2.5 text-muted">{units.find((u) => u.code === p.unit)?.label ?? (p.unit || "–")}</td>
                <td className="tabular py-2.5 text-right">{money(p.price_cents, p.currency)}</td>
                <td className="py-2.5 pr-4 text-right text-muted">{p.usage_count}×</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      <Dialog
        open={draft !== null}
        onClose={() => setDraft(null)}
        title={draft?.id ? t("Edit product") : t("New product")}
        footer={
          <>
            {draft?.id && (
              <Button
                variant="danger"
                className="mr-auto"
                onClick={async () => {
                  await api.deleteProduct(draft.id!);
                  setDraft(null);
                  products.reload();
                }}
              >
                {t("Delete")}
              </Button>
            )}
            <Button onClick={() => setDraft(null)}>{t("Cancel")}</Button>
            <Button variant="primary" onClick={save} disabled={!draft?.name.trim()}>
              {t("Save")}
            </Button>
          </>
        }
      >
        {draft && (
          <>
            <Field label={t("Name")}>
              <Input value={draft.name} onChange={(e) => setDraft({ ...draft, name: e.target.value })} autoFocus />
            </Field>
            <Field label={t("Description")}>
              <Input value={draft.description ?? ""} onChange={(e) => setDraft({ ...draft, description: e.target.value })} />
            </Field>
            <div className="grid grid-cols-3 gap-4">
              <Field label={t("Price")}>
                <Input value={draft.price} onChange={(e) => setDraft({ ...draft, price: e.target.value })} placeholder={centsToInput(0)} />
              </Field>
              <Field label={t("Unit")}>
                <UnitSelect value={draft.unit ?? ""} onChange={(unit) => setDraft({ ...draft, unit })} />
              </Field>
              <Field label={t("Currency")}>
                <CurrencySelect value={draft.currency} onChange={(e) => setDraft({ ...draft, currency: e.target.value })} />
              </Field>
            </div>
          </>
        )}
      </Dialog>
    </Card>
  );
}

export default function Invoices() {
  const { params, settings } = useApp();
  const toast = useToast();
  const [tab, setTab] = useState<Tab>("invoices");
  const [status, setStatus] = useState("all");
  const [search, setSearch] = useState("");
  const [openId, setOpenId] = useState<string | null>(params.open ?? null);
  const [creating, setCreating] = useState(params.new === "1");
  const [prefill, setPrefill] = useState<TrackerInvoiceDraft | null>(
    params.customer ? { customer_id: params.customer, currency: "", lines: [], entry_ids: [] } : null,
  );
  const debounced = useDebounced(search);
  const list = useAsync(() => api.listInvoices({ status, search: debounced }), [status, debounced]);

  useEffect(() => {
    if (params.fromProject) {
      api
        .invoiceFromTracker(params.fromProject)
        .then((draft) => {
          setPrefill(draft);
          setCreating(true);
        })
        .catch((e) => toast.error(errorMessage(e)));
    }
  }, [params.fromProject]);

  const d = list.data;
  const cur = settings.base_currency;

  return (
    <div className="flex flex-col gap-4">
      <PageHeader
        title={t("Invoices")}
        subtitle={t("Create and track invoices.")}
        actions={
          <Button
            variant="primary"
            icon={<Plus className="size-3.5" />}
            onClick={() => {
              setPrefill(null);
              setCreating(true);
            }}
          >
            {t("New invoice")}
          </Button>
        }
      />
      <Tabs
        items={[
          { value: "invoices", label: t("Invoices") },
          { value: "products", label: t("Products") },
        ]}
        value={tab}
        onChange={setTab}
      />

      {tab === "products" && <ProductsTab />}
      {tab === "invoices" && (
        <>
          <div className="grid grid-cols-2 gap-4 lg:grid-cols-4">
            <Kpi label={t("Drafts ({n})", { n: d?.draft.count ?? 0 })} value={money(d?.draft.cents ?? 0, cur)} onClick={() => setStatus("draft")} />
            <Kpi label={t("Open ({n})", { n: d?.open.count ?? 0 })} value={money(d?.open.cents ?? 0, cur)} onClick={() => setStatus("open")} />
            <Kpi label={t("Overdue ({n})", { n: d?.overdue.count ?? 0 })} value={money(d?.overdue.cents ?? 0, cur)} tone={d && d.overdue.count > 0 ? "neg" : undefined} onClick={() => setStatus("overdue")} />
            <Kpi label={t("Paid ({n})", { n: d?.paid.count ?? 0 })} value={money(d?.paid.cents ?? 0, cur)} tone="pos" onClick={() => setStatus("paid")} />
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <Input value={search} onChange={(e) => setSearch(e.target.value)} placeholder={t("Number or customer…")} className="w-64" />
            <Select value={status} onChange={(e) => setStatus(e.target.value)} className="w-40" aria-label={t("Status")}>
              <option value="all">{t("All statuses")}</option>
              <option value="draft">{t("Draft")}</option>
              <option value="open">{t("Open + overdue")}</option>
              <option value="overdue">{t("Overdue")}</option>
              <option value="paid">{t("Paid")}</option>
              <option value="canceled">{t("Canceled")}</option>
            </Select>
          </div>
          <Card>
            {list.error && <div className="p-4"><ErrorNote message={list.error} /></div>}
            {list.loading && !d ? (
              <Skeleton />
            ) : d?.items.length === 0 ? (
              <Empty icon={<FileText className="size-6" />} title={t("No invoices")} text={t("Create an invoice or bill tracked time from time tracking.")} />
            ) : (
              <table className="w-full text-[13px]">
                <thead>
                  <tr className="border-b border-line text-left text-xs text-muted">
                    <th className="py-2.5 pl-4 font-medium">{t("Number")}</th>
                    <th className="py-2.5 font-medium">{t("Customer")}</th>
                    <th className="py-2.5 font-medium">{t("Date")}</th>
                    <th className="py-2.5 font-medium">{t("Due")}</th>
                    <th className="py-2.5 font-medium">{t("Status")}</th>
                    <th className="py-2.5 font-medium">{t("Paid on")}</th>
                    <th className="py-2.5 pr-4 text-right font-medium">{t("Amount")}</th>
                  </tr>
                </thead>
                <tbody>
                  {d?.items.map((inv) => (
                    <tr key={inv.id} onClick={() => setOpenId(inv.id)} className="cursor-pointer border-b border-line last:border-0 hover:bg-subtle">
                      <td className="tabular py-2.5 pl-4">
                        {inv.invoice_number}
                      </td>
                      <td className="py-2.5">{inv.customer_name ?? <span className="text-muted">{t("No customer")}</span>}</td>
                      <td className="tabular-nums py-2.5 text-muted">{date(inv.issue_date)}</td>
                      <td className="tabular-nums py-2.5 text-muted">{date(inv.due_date)}</td>
                      <td className="py-2.5">
                        <StatusBadge status={inv.status} />
                      </td>
                      <td className="tabular-nums py-2.5 text-muted">{inv.paid_at ? date(inv.paid_at) : "–"}</td>
                      <td className="tabular py-2.5 pr-4 text-right">{money(inv.amount_cents, inv.currency)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </Card>
        </>
      )}

      <InvoiceEditor
        invoiceId={openId}
        isNew={creating}
        prefill={prefill}
        onClose={() => {
          setOpenId(null);
          setCreating(false);
          setPrefill(null);
        }}
        onChanged={list.reload}
      />
    </div>
  );
}
