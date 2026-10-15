import { useEffect, useState } from "react";
import { ask } from "@tauri-apps/plugin-dialog";
import { FileText, Plus, Star, Trash2, Users, X } from "lucide-react";
import { api, errorMessage, type Customer, type CustomerContact } from "../lib/api";
import { duration, money } from "../lib/format";
import { useAsync, useDebounced } from "../lib/hooks";
import { useApp } from "../lib/nav";
import { Badge, Button, Card, Checkbox, CurrencySelect, Empty, Field, IconButton, Input, PageHeader, Section, Sheet, Skeleton, Textarea, Toggle, useToast } from "../components/ui";
import { CountrySelect, PlaceInput } from "../components/fields";
import { t } from "../lib/i18n";

const EMPTY: Customer = { name: "", is_archived: false, contacts: [] };

const emptyContact = (): CustomerContact => ({ name: null, email: null, phone: null });

const filled = (k: CustomerContact) => [k.name, k.email, k.phone].some((v) => v?.trim());

function ContactList({ contacts, onChange }: { contacts: CustomerContact[]; onChange: (contacts: CustomerContact[]) => void }) {
  const rows = contacts.length ? contacts : [emptyContact()];
  const update = (i: number, patch: Partial<CustomerContact>) => onChange(rows.map((k, idx) => (idx === i ? { ...k, ...patch } : k)));
  const field = (i: number, key: "name" | "email" | "phone") => ({
    value: rows[i][key] ?? "",
    onChange: (e: React.ChangeEvent<HTMLInputElement>) => update(i, { [key]: e.target.value || null }),
  });
  const cols = "grid grid-cols-[minmax(0,1fr)_minmax(0,1.2fr)_minmax(0,0.9fr)_64px] items-center gap-2";

  return (
    <div className="overflow-hidden rounded-lg border border-line">
      <div className={`${cols} border-b border-line bg-subtle px-3 py-2 text-xs font-medium text-muted`}>
        <span>{t("Name")}</span>
        <span>{t("Email")}</span>
        <span>{t("Phone")}</span>
        <span />
      </div>
      <div className="divide-y divide-line">
        {rows.map((k, i) => (
          <div key={k.id ?? `new-${i}`} className={`${cols} px-3 py-2`}>
            <Input {...field(i, "name")} placeholder={t("Contact person")} aria-label={t("Name")} />
            <Input type="email" {...field(i, "email")} aria-label={t("Email")} />
            <Input {...field(i, "phone")} aria-label={t("Phone")} />
            <div className="flex items-center justify-end">
              {i === 0 ? (
                <span className="inline-flex size-7 items-center justify-center text-accent">
                  <Star className="size-3.5 fill-current" />
                </span>
              ) : (
                <IconButton label={t("Make main contact")} onClick={() => onChange([k, ...rows.filter((_, idx) => idx !== i)])}>
                  <Star className="size-3.5" />
                </IconButton>
              )}
              <IconButton label={t("Remove contact")} onClick={() => onChange(rows.filter((_, idx) => idx !== i))} disabled={rows.length === 1 && !filled(k)}>
                <X className="size-3.5" />
              </IconButton>
            </div>
          </div>
        ))}
      </div>
      <div className="flex items-center gap-2 border-t border-line bg-subtle/60 px-3 py-2">
        <Button size="sm" variant="ghost" icon={<Plus className="size-3" />} onClick={() => onChange([...rows, emptyContact()])}>
          {t("Contact")}
        </Button>
      </div>
    </div>
  );
}

function CustomerSheet({ customer, onClose, onSaved }: { customer: Customer | null; onClose: () => void; onSaved: () => void }) {
  const toast = useToast();
  const { go, settings } = useApp();
  const [form, setForm] = useState<Customer>(EMPTY);
  const [busy, setBusy] = useState(false);
  useEffect(() => setForm(customer?.id ? customer : { ...EMPTY, country: settings.company_country }), [customer, settings.company_country]);
  const set = (patch: Partial<Customer>) => setForm({ ...form, ...patch });
  const text = (key: keyof Customer) => ({
    value: (form[key] as string | null | undefined) ?? "",
    onChange: (e: React.ChangeEvent<HTMLInputElement | HTMLTextAreaElement>) => set({ [key]: e.target.value || null } as Partial<Customer>),
  });

  const save = async () => {
    setBusy(true);
    try {
      await api.saveCustomer({ ...form, name: form.name ?? "", contacts: (form.contacts ?? []).filter(filled) });
      toast.success(t("Customer saved"));
      onSaved();
      onClose();
    } catch (e) {
      toast.error(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const remove = async () => {
    if (!form.id) return;
    if (!(await ask(t("Delete \"{name}\"? Invoices are kept but lose the link.", { name: form.name }), { title: t("Delete"), kind: "warning" }))) return;
    await api.deleteCustomer(form.id);
    onSaved();
    onClose();
  };

  return (
    <Sheet
      open={customer !== null}
      onClose={onClose}
      wide
      title={form.id ? form.name : t("New customer")}
      footer={
        <>
          {form.id && (
            <>
              <Button variant="danger" icon={<Trash2 className="size-3.5" />} onClick={remove}>
                {t("Delete")}
              </Button>
              <Button variant="ghost" icon={<FileText className="size-3.5" />} className="mr-auto" onClick={() => go("invoices", { new: "1", customer: form.id! })}>
                {t("New invoice")}
              </Button>
            </>
          )}
          <Button onClick={onClose}>{t("Cancel")}</Button>
          <Button variant="primary" loading={busy} onClick={save} disabled={!form.name?.trim()}>
            {t("Save")}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-8 px-2 py-2">
        {form.id && (
          <div className="grid grid-cols-3 divide-x divide-line rounded-lg border border-line">
            {[
              [t("Paid"), money(form.paid_cents ?? 0, settings.base_currency)],
              [t("Outstanding"), money(form.open_cents ?? 0, settings.base_currency)],
              [t("Tracked time"), duration(form.tracked_seconds ?? 0)],
            ].map(([label, value]) => (
              <div key={label} className="flex flex-col gap-1 px-5 py-4">
                <span className="text-xs text-muted">{label}</span>
                <span className="tabular text-[15px] font-medium">{value}</span>
              </div>
            ))}
          </div>
        )}

        <div className="grid grid-cols-[minmax(0,1fr)_160px] gap-x-6 gap-y-4">
          <Field label={t("Name / company")}>
            <Input {...text("name")} value={form.name} autoFocus />
          </Field>
          <Field label={t("Customer no.")}>
            <Input value={form.customer_number ?? ""} placeholder={t("automatic")} disabled className="tabular-nums" />
          </Field>
        </div>

        <Section title={t("Contacts")}>
          <ContactList contacts={form.contacts ?? []} onChange={(contacts) => set({ contacts })} />
        </Section>

        <Section title={t("Address")}>
          <div className="grid grid-cols-2 gap-x-6 gap-y-4">
            <Field label={t("Street and number")}>
              <Input {...text("address_line1")} />
            </Field>
            <Field label={t("Postcode and town")}>
              <PlaceInput
                key={customer?.id ?? "new"}
                zip={customer?.zip ?? ""}
                city={customer?.city ?? ""}
                onChange={(p) => set({ zip: p.zip || null, city: p.city || null, ...(p.country && { country: p.country }) })}
              />
            </Field>
            <Field label={t("Country")}>
              <CountrySelect value={form.country ?? ""} onChange={(country) => set({ country })} />
            </Field>
            <Field label={t("Website")}>
              <Input {...text("website")} />
            </Field>
          </div>
        </Section>

        <Section title={t("Invoices")}>
          <div className="grid grid-cols-2 gap-x-6 gap-y-4">
            <Field label={t("Billing email")}>
              <Input type="email" {...text("billing_email")} placeholder={t("if different")} />
            </Field>
            <Field label={t("VAT ID")}>
              <Input {...text("vat_number")} />
            </Field>
            <Field label={t("Leitweg-ID")} hint={t("For public-sector clients (XRechnung)")}>
              <Input {...text("leitweg_id")} placeholder="04011000-12345-67" />
            </Field>
            <div className="grid grid-cols-2 gap-x-6">
              <Field label={t("Payment terms (days)")}>
                <Input value={form.default_payment_terms ?? ""} onChange={(e) => set({ default_payment_terms: e.target.value ? Number(e.target.value) : null })} />
              </Field>
              <Field label={t("Currency")}>
                <CurrencySelect value={form.preferred_currency ?? ""} onChange={(e) => set({ preferred_currency: e.target.value || null })}>
                  <option value="">{t("Default")}</option>
                </CurrencySelect>
              </Field>
            </div>
          </div>
        </Section>

        <Section title={t("Note")}>
          <Textarea {...text("note")} aria-label={t("Note")} />
          {form.id && <Toggle checked={form.is_archived ?? false} onChange={(v) => set({ is_archived: v })} label={t("Archived")} hint={t("Hidden in lists and dropdowns")} />}
        </Section>
      </div>
    </Sheet>
  );
}

export default function Customers() {
  const { params, settings } = useApp();
  const [search, setSearch] = useState("");
  const [archived, setArchived] = useState(false);
  const [open, setOpen] = useState<Customer | null>(params.new === "1" ? EMPTY : null);
  const debounced = useDebounced(search);
  const customers = useAsync(() => api.listCustomers(debounced, archived), [debounced, archived]);

  useEffect(() => {
    if (params.open && customers.data) {
      const c = customers.data.find((x) => x.id === params.open);
      if (c) setOpen(c);
    }
  }, [params.open, customers.data]);

  return (
    <div className="flex flex-col gap-4">
      <PageHeader
        title={t("Customers")}
        subtitle={t("Contacts for invoices and projects.")}
        actions={
          <Button variant="primary" icon={<Plus className="size-3.5" />} onClick={() => setOpen(EMPTY)}>
            {t("New customer")}
          </Button>
        }
      />
      <div className="flex items-center gap-2">
        <Input value={search} onChange={(e) => setSearch(e.target.value)} placeholder={t("Name, email, contact person…")} className="w-72" />
        <label className="flex cursor-pointer items-center gap-2 text-[13px] whitespace-nowrap text-muted">
          <Checkbox checked={archived} onChange={setArchived} />
          {t("Show archived")}
        </label>
      </div>
      <Card>
        {customers.loading && !customers.data ? (
          <Skeleton />
        ) : customers.data?.length === 0 ? (
          <Empty icon={<Users className="size-6" />} title={t("No customers yet")} text={t("Customers are used for invoices and for projects in time tracking.")} action={<Button onClick={() => setOpen(EMPTY)}>{t("Create customer")}</Button>} />
        ) : (
          <table className="w-full text-[13px]">
            <thead>
              <tr className="border-b border-line text-left text-xs text-muted">
                <th className="w-24 py-2.5 pl-4 font-medium">{t("No.")}</th>
                <th className="py-2.5 font-medium">{t("Name")}</th>
                <th className="py-2.5 font-medium">{t("Contact")}</th>
                <th className="py-2.5 text-right font-medium">{t("Invoices")}</th>
                <th className="py-2.5 text-right font-medium">{t("Outstanding")}</th>
                <th className="py-2.5 pr-4 text-right font-medium">{t("Paid")}</th>
              </tr>
            </thead>
            <tbody>
              {customers.data?.map((c) => (
                <tr key={c.id!} onClick={() => setOpen(c)} className="cursor-pointer border-b border-line last:border-0 hover:bg-subtle">
                  <td className="tabular py-2.5 pl-4 text-muted">{c.customer_number}</td>
                  <td className="py-2.5">
                    {c.name} {c.is_archived && <Badge>{t("archived")}</Badge>}
                    {c.city && <div className="text-xs text-muted">{c.city}</div>}
                  </td>
                  <td className="py-2.5 text-muted">
                    {c.contact}
                    {(c.contacts?.length ?? 0) > 1 && <span className="ml-1.5 text-xs">+{c.contacts!.length - 1}</span>}
                    {c.email && <div className="text-xs">{c.email}</div>}
                  </td>
                  <td className="py-2.5 text-right text-muted">{c.invoice_count}</td>
                  <td className="tabular py-2.5 text-right">{c.open_cents ? money(c.open_cents, settings.base_currency) : "–"}</td>
                  <td className="tabular py-2.5 pr-4 text-right">{money(c.paid_cents ?? 0, settings.base_currency)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Card>
      <CustomerSheet customer={open} onClose={() => setOpen(null)} onSaved={customers.reload} />
    </div>
  );
}
