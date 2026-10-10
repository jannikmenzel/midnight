import { useEffect, useMemo, useState } from "react";
import { ask, open as openDialog } from "@tauri-apps/plugin-dialog";
import { Building2, ExternalLink, FileText, FolderLock, Hash, Link2, Trash2, Upload, X } from "lucide-react";
import { api, errorMessage, type Customer, type DocumentItem } from "../lib/api";
import { date, fileSize } from "../lib/format";
import { useAsync, useDebounced, useFileDrop } from "../lib/hooks";
import { useApp } from "../lib/nav";
import { Badge, Button, Card, DateInput, Empty, Field, Input, MultiSelect, PageHeader, Select, Sheet, Skeleton, Tabs, TagInput, cx, useToast } from "../components/ui";
import { locale, t } from "../lib/i18n";

const DEFAULT_TYPES = [t("Contract"), t("Quote"), t("Invoice"), t("Receipt"), t("Tax"), t("Insurance"), t("Correspondence"), t("Other")];

const NONE = "__none__";
const ALL = "";

function DocumentSheet({
  doc,
  onClose,
  onChanged,
  customers,
  typeSuggestions,
  tagSuggestions,
}: {
  doc: DocumentItem | null;
  onClose: () => void;
  onChanged: () => void;
  customers: Customer[];
  typeSuggestions: string[];
  tagSuggestions: string[];
}) {
  const toast = useToast();
  const { go } = useApp();
  const linked = !!(doc?.invoice_id || doc?.expense_id);
  const [title, setTitle] = useState("");
  const [fileName, setFileName] = useState("");
  const [docDate, setDocDate] = useState("");
  const [customerId, setCustomerId] = useState("");
  const [docType, setDocType] = useState("");
  const [tags, setTags] = useState<string[]>([]);
  useEffect(() => {
    if (doc) {
      setTitle(doc.title ?? "");
      setFileName(doc.name);
      setDocDate(doc.date ?? "");
      setCustomerId(doc.customer_id ?? "");
      setDocType(doc.doc_type ?? "");
      setTags(doc.tags);
    }
  }, [doc]);
  const save = async () => {
    if (!doc) return;
    try {
      if (!linked && fileName.trim() && fileName.trim() !== doc.name) await api.renameDocument(doc.id, fileName);
      await api.updateDocument({ id: doc.id, title: title || null, date: docDate || null, customer_id: customerId || null, doc_type: docType || null, tags });
      onChanged();
      onClose();
    } catch (e) {
      toast.error(errorMessage(e));
    }
  };
  return (
    <Sheet
      open={doc !== null}
      onClose={onClose}
      title={doc?.title ?? doc?.name}
      footer={
        <>
          {linked ? (
            <span className="mr-auto" />
          ) : (
            <Button
              variant="danger"
              className="mr-auto"
              icon={<Trash2 className="size-3.5" />}
              onClick={async () => {
                if (doc && (await ask(t("Permanently delete document?"), { title: t("Delete"), kind: "warning" }))) {
                  await api.deleteDocument(doc.id);
                  onChanged();
                  onClose();
                }
              }}
            >
              {t("Delete")}
            </Button>
          )}
          <Button onClick={onClose}>{t("Cancel")}</Button>
          <Button variant="primary" onClick={save}>
            {t("Save")}
          </Button>
        </>
      }
    >
      {doc && (
        <div className="flex flex-col gap-4">
          <div className="flex items-center gap-3 rounded-lg bg-subtle px-4 py-3">
            <FileText className="size-5 text-muted" />
            <div className="min-w-0 flex-1">
              <div className="truncate text-[13px]">{doc.name}</div>
              <div className="text-xs text-muted">
                {fileSize(doc.size)} · {doc.has_text ? t("Full text indexed") : t("no text detected (image/scan)")}
              </div>
            </div>
            <Button size="sm" icon={<ExternalLink className="size-3" />} onClick={() => api.openPath(doc.path).catch((e) => toast.error(errorMessage(e)))}>
              {t("Open")}
            </Button>
          </div>
          {linked ? (
            <div className="flex items-center gap-3 rounded-lg border border-line px-4 py-3">
              <Link2 className="size-4 shrink-0 text-muted" />
              <div className="min-w-0 flex-1 truncate text-[13px]">
                {doc.invoice_id ? t("Synced with invoice {number}", { number: doc.invoice_number ?? "" }) : t("Synced with expense \"{name}\"", { name: doc.expense_name ?? "" })}
              </div>
              <Button size="sm" onClick={() => (doc.invoice_id ? go("invoices", { open: doc.invoice_id }) : go("ledger", { open: doc.expense_id! }))}>
                {t("Go to source")}
              </Button>
            </div>
          ) : (
            <Field label={t("File name")}>
              <Input value={fileName} onChange={(e) => setFileName(e.target.value)} placeholder={doc.name} />
            </Field>
          )}
          <Field label={t("Title")}>
            <Input value={title} onChange={(e) => setTitle(e.target.value)} placeholder={doc.name} />
          </Field>
          <div className="grid grid-cols-2 gap-3">
            <Field label={t("Customer")}>
              <Select value={customerId} onChange={(e) => setCustomerId(e.target.value)}>
                <option value="">{t("No customer")}</option>
                {customers.map((c) => (
                  <option key={c.id!} value={c.id!}>
                    {c.name}
                  </option>
                ))}
              </Select>
            </Field>
            <Field label={t("Document type")}>
              <Input list="vault-doc-types" value={docType} onChange={(e) => setDocType(e.target.value)} placeholder={t("e.g. Contract")} />
              <datalist id="vault-doc-types">
                {typeSuggestions.map((t) => (
                  <option key={t} value={t} />
                ))}
              </datalist>
            </Field>
          </div>
          <Field label={t("Date")}>
            <DateInput value={docDate} onChange={(v) => setDocDate(v)} />
          </Field>
          <Field label={t("Tags")}>
            <TagInput value={tags} onChange={setTags} suggestions={tagSuggestions} />
          </Field>
        </div>
      )}
    </Sheet>
  );
}

function DocumentRow({ d, onOpen }: { d: DocumentItem; onOpen: () => void }) {
  return (
    <button type="button" onClick={onOpen} className="flex w-full items-center gap-3 px-4 py-2.5 text-left hover:bg-subtle">
      <FileText className="size-4 shrink-0 text-muted" />
      <div className="min-w-0 flex-1">
        <div className="truncate text-[13px]">{d.title ?? d.name}</div>
        <div className="truncate text-xs text-muted">
          {d.snippet ?? `${d.date ? date(d.date) : date(d.created_at.slice(0, 10))} · ${fileSize(d.size)}`}
        </div>
      </div>
      <div className="flex shrink-0 items-center gap-2">
        {d.tags.map((t) => (
          <span key={t} className="inline-flex items-center text-[11px] text-muted">
            <Hash className="size-3" />
            {t}
          </span>
        ))}
        {d.doc_type && <Badge tone="info">{d.doc_type}</Badge>}
      </div>
    </button>
  );
}

export default function Vault() {
  const { params } = useApp();
  const toast = useToast();
  const [search, setSearch] = useState("");
  const [selCustomers, setSelCustomers] = useState<string[]>([]);
  const [docType, setDocType] = useState(ALL);
  const [tag, setTag] = useState(ALL);
  const [openDoc, setOpenDoc] = useState<DocumentItem | null>(null);
  const debounced = useDebounced(search);
  const docs = useAsync(() => api.listDocuments(debounced), [debounced]);
  const customers = useAsync(() => api.listCustomers(undefined, true), []);
  const allTags = useAsync(() => api.listTags(), []);

  const upload = async (paths: string[]) => {
    if (!paths.length) return;
    try {
      const n = await api.vaultUpload(paths);
      toast.success(t("{n} document(s) added", { n }));
      docs.reload();
    } catch (e) {
      toast.error(errorMessage(e));
    }
  };

  const dragging = useFileDrop(upload);

  useEffect(() => {
    if (params.open && docs.data) {
      const d = docs.data.find((x) => x.id === params.open);
      if (d) setOpenDoc(d);
    }
  }, [params.open, docs.data]);

  const all = docs.data ?? [];
  const customerName = (id: string) => (customers.data ?? []).find((c) => c.id === id)?.name ?? t("Unknown customer");

  const matches = (d: DocumentItem, skip?: "customer" | "type") =>
    (skip === "customer" || selCustomers.length === 0 || selCustomers.includes(d.customer_id ?? NONE)) &&
    (skip === "type" || docType === ALL || (docType === NONE ? !d.doc_type : d.doc_type === docType)) &&
    (tag === ALL || d.tags.includes(tag));

  const count = (list: DocumentItem[], key: (d: DocumentItem) => string) => {
    const m = new Map<string, number>();
    for (const d of list) m.set(key(d), (m.get(key(d)) ?? 0) + 1);
    return m;
  };

  const customerCounts = count(all.filter((d) => matches(d, "customer")), (d) => d.customer_id ?? NONE);
  const typeCounts = count(all.filter((d) => matches(d, "type")), (d) => d.doc_type ?? NONE);
  const sum = (m: Map<string, number>) => [...m.values()].reduce((s, n) => s + n, 0);

  const typeTabs = [
    { value: ALL, label: t("All"), count: sum(typeCounts) },
    ...[...typeCounts.entries()]
      .filter(([k]) => k !== NONE)
      .sort(([a], [b]) => a.localeCompare(b, locale()))
      .map(([k, n]) => ({ value: k, label: k, count: n })),
    ...(typeCounts.has(NONE) ? [{ value: NONE, label: t("No type"), count: typeCounts.get(NONE)! }] : []),
  ];

  const customerOptions = [
    ...(customers.data ?? [])
      .map((c) => ({ value: c.id!, label: c.name, count: customerCounts.get(c.id!) ?? 0 }))
      .sort((a, b) => a.label.localeCompare(b.label, locale())),
    ...(customerCounts.has(NONE) ? [{ value: NONE, label: t("No customer"), count: customerCounts.get(NONE)! }] : []),
  ];

  const tagOptions = useMemo(() => [...new Set(all.flatMap((d) => d.tags))].sort((a, b) => a.localeCompare(b, locale())), [all]);
  const typeSuggestions = [...new Set([...DEFAULT_TYPES, ...all.map((d) => d.doc_type).filter((x): x is string => !!x)])];

  const visible = all.filter((d) => matches(d));
  const sections =
    selCustomers.length === 1
      ? [{ key: selCustomers[0], title: null as string | null, items: visible }]
      : [...count(visible, (d) => d.customer_id ?? NONE).keys()]
          .map((key) => ({ key, title: key === NONE ? t("No customer") : customerName(key), items: visible.filter((d) => (d.customer_id ?? NONE) === key) }))
          .sort((a, b) => (a.key === NONE ? 1 : b.key === NONE ? -1 : a.title.localeCompare(b.title, locale())));

  const filtersActive = selCustomers.length > 0 || docType !== ALL || tag !== ALL;
  const resetFilters = () => {
    setSelCustomers([]);
    setDocType(ALL);
    setTag(ALL);
  };

  const pick = async () => {
    const files = await openDialog({ multiple: true });
    if (files) upload(Array.isArray(files) ? files : [files]);
  };

  return (
    <div className="flex flex-col gap-4">
      <PageHeader
        title="Vault"
        subtitle={t("Contracts and documents, full-text searchable.")}
        actions={
          <Button variant="primary" icon={<Upload className="size-3.5" />} onClick={pick}>
            {t("Add documents")}
          </Button>
        }
      />

      <div className="flex flex-wrap items-center gap-2">
        <Input value={search} onChange={(e) => setSearch(e.target.value)} placeholder={t("Full-text search…")} className="w-72" />
        <MultiSelect
          options={customerOptions}
          value={selCustomers}
          onChange={setSelCustomers}
          allLabel={t("All customers")}
          pluralLabel={t("Customers")}
          icon={<Building2 className="size-3.5" />}
          className="w-64"
        />
        {tagOptions.length > 0 && (
          <Select value={tag} onChange={(e) => setTag(e.target.value)} className="w-44" aria-label={t("Tag")}>
            <option value={ALL}>{t("All tags")}</option>
            {tagOptions.map((t) => (
              <option key={t} value={t}>
                #{t}
              </option>
            ))}
          </Select>
        )}
        {filtersActive && (
          <Button size="sm" variant="ghost" icon={<X className="size-3" />} onClick={resetFilters}>
            {t("Reset filters")}
          </Button>
        )}
      </div>

      <div className="flex flex-col gap-4">
        <Card className={cx("min-w-0", dragging && "border-dashed border-info")}>
          {typeTabs.length > 2 && (
            <div className="overflow-x-auto px-2">
              <Tabs items={typeTabs} value={docType} onChange={setDocType} />
            </div>
          )}
          {docs.loading && !docs.data ? (
            <Skeleton />
          ) : visible.length === 0 ? (
            <Empty
              icon={<FolderLock className="size-6" />}
              title={debounced || filtersActive ? t("No results") : dragging ? t("Release to add") : t("No documents yet")}
              text={debounced || filtersActive ? undefined : t("Drop files here or add them via \"Add documents\".")}
              action={filtersActive ? <Button onClick={resetFilters}>{t("Reset filters")}</Button> : undefined}
            />
          ) : (
            sections.map((sec) => (
              <section key={sec.key} className="border-line [&+&]:border-t">
                {sec.title && (
                  <div className="flex items-center gap-1.5 border-b border-line bg-subtle px-4 py-1.5 text-xs text-muted">
                    {sec.key !== NONE && <Building2 className="size-3" />}
                    <span className="font-medium text-fg">{sec.title}</span>
                    <span className="tabular-nums">· {sec.items.length}</span>
                  </div>
                )}
                <ul className="divide-y divide-line">
                  {sec.items.map((d) => (
                    <li key={d.id}>
                      <DocumentRow d={d} onOpen={() => setOpenDoc(d)} />
                    </li>
                  ))}
                </ul>
              </section>
            ))
          )}
        </Card>
      </div>

      <DocumentSheet
        doc={openDoc}
        onClose={() => setOpenDoc(null)}
        onChanged={() => {
          docs.reload();
          allTags.reload();
        }}
        customers={customers.data ?? []}
        typeSuggestions={typeSuggestions}
        tagSuggestions={(allTags.data ?? []).map((t) => t.name)}
      />
    </div>
  );
}
