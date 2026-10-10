import { useEffect, useState } from "react";
import { ask, open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { CircleAlert, Download, ExternalLink, FileText, Paperclip, Pause, Play, Plus, Receipt as ReceiptIcon, Repeat, Tags, Trash2, Upload, X } from "lucide-react";
import { api, errorMessage, type Category, type ExpenseRecurring, type LedgerFilter, type Receipt } from "../lib/api";
import { centsToInput, date, FREQUENCY_LABELS, money, parseMoney, todayIso } from "../lib/format";
import { useAsync, useDebounced, useFileDrop } from "../lib/hooks";
import { useApp } from "../lib/nav";
import { Amount, Badge, Button, Card, CategoryBadge, Checkbox, DateInput, Empty, ErrorNote, Field, IconButton, Input, Kpi, PageHeader, PickSelect, Select, Sheet, Skeleton, Tabs, Textarea, cx, useToast, type PickOption } from "../components/ui";
import { t } from "../lib/i18n";

const PAGE = 300;
const RECEIPT_TYPES = [{ name: t("Receipts"), extensions: ["pdf", "png", "jpg", "jpeg", "heic"] }];

function CategorySelect({
  categories,
  value,
  onChange,
  emptyLabel = t("Uncategorised"),
  withUncategorized = false,
  className,
}: {
  categories: Category[];
  value: string | null | undefined;
  onChange: (slug: string) => void;
  emptyLabel?: string;
  withUncategorized?: boolean;
  className?: string;
}) {
  const options: PickOption[] = [
    { value: "", label: emptyLabel },
    ...(withUncategorized ? [{ value: "uncategorized", label: t("Uncategorised") }] : []),
    ...categories.map((c) => ({ value: c.slug, label: c.name, description: c.description, color: c.color })),
  ];
  return <PickSelect value={value ?? ""} onChange={onChange} options={options} className={className} />;
}

interface Draft {
  date: string;
  name: string;
  amount: string;
  category: string;
  description: string;
  note: string;
}

const emptyDraft = (): Draft => ({ date: todayIso(), name: "", amount: "", category: "", description: "", note: "" });

type SheetTarget = { id: string } | { draft: Draft; files: string[] };

const fileName = (path: string) => path.split(/[\\/]/).pop() ?? path;

const FREQUENCIES = ["weekly", "biweekly", "monthly_date", "monthly_last_day", "quarterly", "semi_annual", "annual", "custom"];

interface Repeat {
  frequency: string;
  interval: string;
  end_type: string;
  end_date: string;
  end_count: string;
}

const noRepeat = (): Repeat => ({ frequency: "", interval: "30", end_type: "never", end_date: "", end_count: "12" });

const frequencyLabel = (frequency: string, interval: number | null) =>
  frequency === "custom" ? t("Every {n} days", { n: interval ?? 0 }) : FREQUENCY_LABELS[frequency];

function RepeatFields({ value, onChange }: { value: Repeat; onChange: (r: Repeat) => void }) {
  const set = (patch: Partial<Repeat>) => onChange({ ...value, ...patch });
  return (
    <>
      <div className="grid grid-cols-2 gap-4">
        <Field label={t("Repeat")}>
          <Select value={value.frequency} onChange={(e) => set({ frequency: e.target.value })}>
            <option value="">{t("Never")}</option>
            {FREQUENCIES.map((f) => (
              <option key={f} value={f}>
                {FREQUENCY_LABELS[f]}
              </option>
            ))}
          </Select>
        </Field>
        {value.frequency === "custom" && (
          <Field label={t("Every how many days?")}>
            <Input type="number" min={1} value={value.interval} onChange={(e) => set({ interval: e.target.value })} />
          </Field>
        )}
      </div>
      {value.frequency && (
        <div className="grid grid-cols-2 gap-4">
          <Field label={t("Ends")}>
            <Select value={value.end_type} onChange={(e) => set({ end_type: e.target.value })}>
              <option value="never">{t("Never")}</option>
              <option value="on_date">{t("On a date")}</option>
              <option value="after_count">{t("After a number of expenses")}</option>
            </Select>
          </Field>
          {value.end_type === "on_date" && (
            <Field label={t("Last date")}>
              <DateInput value={value.end_date} onChange={(v) => set({ end_date: v })} />
            </Field>
          )}
          {value.end_type === "after_count" && (
            <Field label={t("Number including this one")}>
              <Input type="number" min={1} value={value.end_count} onChange={(e) => set({ end_count: e.target.value })} />
            </Field>
          )}
        </div>
      )}
    </>
  );
}

function ExpenseSheet({
  target,
  categories,
  onClose,
  onSaved,
  onShowSeries,
}: {
  target: SheetTarget | null;
  categories: Category[];
  onClose: () => void;
  onSaved: () => void;
  onShowSeries: () => void;
}) {
  const toast = useToast();
  const [draft, setDraft] = useState<Draft>(emptyDraft());
  const [receipts, setReceipts] = useState<Receipt[]>([]);
  const [pending, setPending] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [repeat, setRepeat] = useState<Repeat>(noRepeat);
  const [seriesId, setSeriesId] = useState<string | null>(null);
  const [createdId, setCreatedId] = useState<string | null>(null);
  const id = target && "id" in target ? target.id : null;

  useEffect(() => {
    setError(null);
    setRepeat(noRepeat());
    setSeriesId(null);
    setCreatedId(null);
    if (!target) return;
    if ("id" in target) {
      setPending([]);
      api
        .getExpense(target.id)
        .then((e) => {
          setDraft({ date: e.date, name: e.name, amount: centsToInput(e.amount_cents), category: e.category_slug ?? "", description: e.description ?? "", note: e.note ?? "" });
          setSeriesId(e.expense_recurring_id);
        })
        .catch((e) => setError(errorMessage(e)));
      api.listReceipts(target.id).then(setReceipts).catch(() => setReceipts([]));
    } else {
      setDraft(target.draft);
      setPending(target.files);
      setReceipts([]);
    }
  }, [target]);

  const set = <K extends keyof Draft>(k: K, v: Draft[K]) => setDraft((d) => ({ ...d, [k]: v }));

  const attach = async () => {
    const files = await openDialog({ multiple: true, filters: RECEIPT_TYPES });
    if (!files) return;
    const paths = Array.isArray(files) ? files : [files];
    if (id) {
      await api.addReceipts(id, paths);
      setReceipts(await api.listReceipts(id));
      onSaved();
    } else {
      setPending((p) => [...p, ...paths]);
    }
  };

  const save = async () => {
    const cents = parseMoney(draft.amount);
    if (cents === null || cents <= 0) return setError(t("Please enter an amount greater than 0."));
    setBusy(true);
    setError(null);
    try {
      const saved = await api.saveExpense({
        id: id ?? createdId ?? undefined,
        date: draft.date,
        name: draft.name,
        amount_cents: cents,
        category_slug: draft.category || null,
        description: draft.description || null,
        note: draft.note || null,
      });
      setCreatedId(saved.id);
      if (pending.length) {
        await api.addReceipts(saved.id, pending);
        setPending([]);
      }
      if (repeat.frequency && !seriesId) {
        const series = await api.saveExpenseRecurring({
          expense_id: saved.id,
          frequency: repeat.frequency,
          frequency_interval: repeat.frequency === "custom" ? Number(repeat.interval) : null,
          end_type: repeat.end_type,
          end_date: repeat.end_type === "on_date" ? repeat.end_date : null,
          end_count: repeat.end_type === "after_count" ? Number(repeat.end_count) : null,
        });
        toast.success(
          series.caught_up > 0 ? t("Recurring expense set up, {n} past date(s) booked", { n: series.caught_up }) : t("Recurring expense set up"),
        );
      } else {
        toast.success(id ? t("Expense saved") : t("Expense recorded"));
      }
      onSaved();
      onClose();
    } catch (e) {
      setError(errorMessage(e));
      onSaved();
    } finally {
      setBusy(false);
    }
  };

  const remove = async () => {
    if (!id || !(await ask(t("Delete this expense and all its receipts?"), { title: t("Delete"), kind: "warning" }))) return;
    await api.deleteExpense(id);
    onSaved();
    onClose();
  };

  const noReceipt = receipts.length === 0 && pending.length === 0;

  return (
    <Sheet
      open={target !== null}
      onClose={onClose}
      title={id ? draft.name || t("Expense") : t("New expense")}
      footer={
        <>
          {id && (
            <Button variant="danger" icon={<Trash2 className="size-3.5" />} onClick={remove} className="mr-auto">
              {t("Delete")}
            </Button>
          )}
          <Button onClick={onClose}>{t("Cancel")}</Button>
          <Button variant="primary" loading={busy} onClick={save} disabled={!draft.name.trim()}>
            {t("Save")}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-4">
        <Field label={t("What for?")} hint={t("Vendor or service, e.g. \"Figma Professional\"")}>
          <Input value={draft.name} onChange={(e) => set("name", e.target.value)} autoFocus={!id} />
        </Field>
        <div className="grid grid-cols-2 gap-4">
          <Field label={t("Paid on")}>
            <DateInput value={draft.date} onChange={(v) => set("date", v)} />
          </Field>
          <Field label={t("Amount")}>
            <Input value={draft.amount} onChange={(e) => set("amount", e.target.value)} placeholder={centsToInput(0)} className="tabular-nums" />
          </Field>
        </div>
        <Field label={t("Category")}>
          <CategorySelect categories={categories} value={draft.category} onChange={(v) => set("category", v)} />
        </Field>
        {seriesId ? (
          <div className="flex items-center gap-2 rounded-md bg-subtle px-3 py-2 text-xs">
            <Repeat className="size-3.5 shrink-0 text-muted" />
            <span className="flex-1">{t("Part of a recurring series. Changes here only affect this expense.")}</span>
            <Button
              size="sm"
              onClick={() => {
                onClose();
                onShowSeries();
              }}
            >
              {t("Show series")}
            </Button>
          </div>
        ) : (
          <RepeatFields value={repeat} onChange={setRepeat} />
        )}
        <Field label={t("Description")}>
          <Input value={draft.description} onChange={(e) => set("description", e.target.value)} />
        </Field>
        <Field label={t("Note")}>
          <Textarea value={draft.note} onChange={(e) => set("note", e.target.value)} />
        </Field>

        <div className="flex flex-col gap-2">
          <div className="flex items-center justify-between">
            <span className="text-xs font-medium text-muted">{t("Receipts")}</span>
            <Button size="sm" icon={<Paperclip className="size-3" />} onClick={attach}>
              {t("Attach receipt")}
            </Button>
          </div>
          {noReceipt && (
            <p className="flex items-center gap-1.5 text-xs text-warn">
              <CircleAlert className="size-3.5" />
              {t("No receipt attached yet.")}
            </p>
          )}
          {receipts.map((r) => (
            <div key={r.id} className="flex items-center gap-2 rounded-md border border-line px-2.5 py-1.5">
              <FileText className="size-3.5 text-muted" />
              <span className="min-w-0 flex-1 truncate text-[13px]">{r.name}</span>
              <IconButton label={t("Open")} onClick={() => api.openPath(r.path).catch((e) => toast.error(errorMessage(e)))}>
                <ExternalLink className="size-3.5" />
              </IconButton>
              <IconButton
                label={t("Remove")}
                onClick={async () => {
                  await api.deleteReceipt(r.id);
                  setReceipts((xs) => xs.filter((x) => x.id !== r.id));
                  onSaved();
                }}
              >
                <X className="size-3.5" />
              </IconButton>
            </div>
          ))}
          {pending.map((p) => (
            <div key={p} className="flex items-center gap-2 rounded-md border border-dashed border-line px-2.5 py-1.5">
              <FileText className="size-3.5 text-muted" />
              <span className="min-w-0 flex-1 truncate text-[13px]">{fileName(p)}</span>
              <span className="text-xs text-muted">{t("attached on save")}</span>
              <IconButton label={t("Remove")} onClick={() => setPending((xs) => xs.filter((x) => x !== p))}>
                <X className="size-3.5" />
              </IconButton>
            </div>
          ))}
        </div>
        {error && <ErrorNote message={error} />}
      </div>
    </Sheet>
  );
}

function RecurringExpenses() {
  const toast = useToast();
  const series = useAsync(() => api.listExpenseRecurring(), []);
  const STATUS: Record<ExpenseRecurring["status"], { label: string; tone: "pos" | "neutral" | "warn" }> = {
    active: { label: t("Active"), tone: "pos" },
    paused: { label: t("Paused"), tone: "warn" },
    completed: { label: t("Completed"), tone: "neutral" },
    canceled: { label: t("Ended"), tone: "neutral" },
  };
  const act = (p: Promise<void>) => p.then(series.reload).catch((e) => toast.error(errorMessage(e)));
  return (
    <Card>
      {series.error && (
        <div className="p-4">
          <ErrorNote message={series.error} />
        </div>
      )}
      {!series.data ? (
        <Skeleton rows={3} />
      ) : series.data.length === 0 ? (
        <Empty
          icon={<Repeat className="size-6" />}
          title={t("No recurring expenses")}
          text={t("Choose \"Repeat\" when recording an expense, e.g. for rent, software or insurance. Following expenses are booked automatically.")}
        />
      ) : (
        <table className="w-full table-fixed text-[13px]">
          <thead>
            <tr className="border-b border-line text-left text-xs text-muted">
              <th className="py-2.5 pl-4 font-medium">{t("Description")}</th>
              <th className="w-48 py-2.5 font-medium">{t("Category")}</th>
              <th className="w-40 py-2.5 font-medium">{t("Frequency")}</th>
              <th className="w-28 py-2.5 font-medium">{t("Next")}</th>
              <th className="w-20 py-2.5 font-medium">{t("Booked")}</th>
              <th className="w-28 py-2.5 text-right font-medium">{t("Amount")}</th>
              <th className="w-28 py-2.5 pl-3 font-medium" />
              <th className="w-24 py-2.5 pr-4" />
            </tr>
          </thead>
          <tbody>
            {series.data.map((r) => (
              <tr key={r.id} className="border-b border-line last:border-0">
                <td className="max-w-0 py-2.5 pr-3 pl-4">
                  <div className="truncate">{r.name}</div>
                  {r.description && <div className="truncate text-xs text-muted">{r.description}</div>}
                </td>
                <td className="py-2.5 pr-3">
                  <CategoryBadge color={r.category_color}>{r.category_name ?? t("Uncategorised")}</CategoryBadge>
                </td>
                <td className="py-2.5">{frequencyLabel(r.frequency, r.frequency_interval)}</td>
                <td className="tabular-nums py-2.5">{r.status === "active" ? date(r.next_scheduled_at) : "–"}</td>
                <td className="py-2.5 text-muted">
                  {r.expenses_generated}
                  {r.end_type === "after_count" && r.end_count ? ` / ${r.end_count}` : ""}
                </td>
                <td className="py-2.5 text-right">
                  <Amount cents={-r.amount_cents} currency={r.currency} />
                </td>
                <td className="py-2.5 pl-3">
                  <Badge tone={STATUS[r.status].tone}>{STATUS[r.status].label}</Badge>
                </td>
                <td className="py-2.5 pr-4 text-right">
                  {r.status === "active" && (
                    <IconButton label={t("Pause")} onClick={() => act(api.setExpenseRecurringStatus(r.id, "paused"))}>
                      <Pause className="size-3.5" />
                    </IconButton>
                  )}
                  {r.status === "paused" && (
                    <IconButton label={t("Resume")} onClick={() => act(api.setExpenseRecurringStatus(r.id, "active"))}>
                      <Play className="size-3.5" />
                    </IconButton>
                  )}
                  <IconButton
                    label={t("Delete series")}
                    onClick={async () => {
                      if (await ask(t("Delete series? Expenses already booked are kept."), { title: t("Delete"), kind: "warning" })) {
                        act(api.deleteExpenseRecurring(r.id));
                      }
                    }}
                  >
                    <Trash2 className="size-3.5" />
                  </IconButton>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </Card>
  );
}

export default function Ledger() {
  const { params, go, settings } = useApp();
  const toast = useToast();
  const [search, setSearch] = useState("");
  const [filter, setFilter] = useState<LedgerFilter>({
    kind: (params.kind as LedgerFilter["kind"]) ?? "",
    category: params.category ?? "",
    from: params.from ?? "",
    to: params.to ?? "",
    missing_receipt: params.missing_receipt === "1",
  });
  const [limit, setLimit] = useState(PAGE);
  const [target, setTarget] = useState<SheetTarget | null>(null);
  const [tab, setTab] = useState<"entries" | "recurring">(params.tab === "recurring" ? "recurring" : "entries");
  const [queue, setQueue] = useState<string[]>([]);
  const debounced = useDebounced(search);

  const query: LedgerFilter = { ...filter, search: debounced, limit };
  const page = useAsync(() => api.listLedger(query), [JSON.stringify(query)]);
  const categories = useAsync(() => api.listCategories(), []);

  const startFromReceipts = async (paths: string[]) => {
    const [first, ...rest] = paths;
    if (!first) return;
    setQueue(rest);
    const found = await api.extractReceipt(first).catch(() => null);
    setTarget({
      draft: {
        ...emptyDraft(),
        name: found?.name ?? fileName(first).replace(/\.[^.]+$/, ""),
        amount: found?.amount_cents ? centsToInput(found.amount_cents) : "",
        date: found?.date ?? todayIso(),
      },
      files: [first],
    });
  };

  const pickReceipts = async () => {
    const files = await openDialog({ multiple: true, filters: RECEIPT_TYPES });
    if (files) startFromReceipts(Array.isArray(files) ? files : [files]);
  };

  useEffect(() => {
    if (params.open) setTarget({ id: params.open });
    else if (params.new === "1") setTarget({ draft: emptyDraft(), files: [] });
    else if (params.upload === "1") pickReceipts();
  }, []);

  const dragging = useFileDrop(startFromReceipts);

  const closeSheet = () => {
    setTarget(null);
    if (queue.length) startFromReceipts(queue);
  };

  const exportCsv = async () => {
    const path = await saveDialog({ defaultPath: `${t("income-expenses")}-${todayIso()}.csv`, filters: [{ name: "CSV", extensions: ["csv"] }] });
    if (!path) return;
    try {
      const n = await api.exportLedger({ ...filter, search: debounced }, path);
      toast.success(t("{n} transactions exported", { n }));
    } catch (e) {
      toast.error(errorMessage(e));
    }
  };

  const d = page.data;
  const items = d?.items ?? [];
  const filtersActive = Boolean(filter.kind || filter.category || filter.from || filter.to || filter.missing_receipt || debounced);

  return (
    <div className="flex flex-col gap-4">
      <PageHeader
        title={t("Income & expenses")}
        subtitle={t("Paid invoices count as income; record expenses with a receipt.")}
        actions={
          <>
            <Button variant="ghost" icon={<Tags className="size-3.5" />} onClick={() => go("categories")}>
              {t("Categories")}
            </Button>
            <Button icon={<Download className="size-3.5" />} onClick={exportCsv}>
              {t("Export")}
            </Button>
            <Button icon={<Upload className="size-3.5" />} onClick={pickReceipts}>
              {t("Upload receipt")}
            </Button>
            <Button variant="primary" icon={<Plus className="size-3.5" />} onClick={() => setTarget({ draft: emptyDraft(), files: [] })}>
              {t("New expense")}
            </Button>
          </>
        }
      />

      <div className="grid grid-cols-3 gap-4">
        <Kpi label={t("Income")} value={d ? money(d.income_cents, settings.base_currency) : "…"} tone="pos" />
        <Kpi label={t("Expenses")} value={d ? money(d.expense_cents, settings.base_currency) : "…"} />
        <Kpi label={t("Result")} value={d ? money(d.income_cents - d.expense_cents, settings.base_currency) : "…"} tone={d && d.income_cents - d.expense_cents < 0 ? "neg" : undefined} />
      </div>

      <Tabs
        items={[
          { value: "entries", label: t("Transactions") },
          { value: "recurring", label: t("Recurring") },
        ]}
        value={tab}
        onChange={setTab}
      />

      {tab === "recurring" ? (
        <RecurringExpenses />
      ) : (
        <>
        <div className="flex flex-wrap items-center gap-2">
          <Input value={search} onChange={(e) => setSearch(e.target.value)} placeholder={t("Search…")} className="w-64" />
          <Select value={filter.kind ?? ""} onChange={(e) => setFilter({ ...filter, kind: e.target.value as LedgerFilter["kind"] })} className="w-36" aria-label={t("Type")}>
            <option value="">{t("All")}</option>
            <option value="income">{t("Income")}</option>
            <option value="expense">{t("Expenses")}</option>
          </Select>
          <CategorySelect
            categories={categories.data ?? []}
            value={filter.category}
            onChange={(v) => setFilter({ ...filter, category: v })}
            emptyLabel={t("All categories")}
            withUncategorized
            className="w-48"
          />
          <DateInput value={filter.from ?? ""} onChange={(v) => setFilter({ ...filter, from: v })} className="w-36" aria-label={t("From")} />
          <DateInput value={filter.to ?? ""} onChange={(v) => setFilter({ ...filter, to: v })} className="w-36" aria-label={t("To")} />
          <label className="flex cursor-pointer items-center gap-2 px-1 text-[13px] whitespace-nowrap text-muted">
            <Checkbox checked={Boolean(filter.missing_receipt)} onChange={(v) => setFilter({ ...filter, missing_receipt: v })} />
            {t("Without receipt")}
          </label>
          <Button
            variant="ghost"
            size="sm"
            className={cx(!filtersActive && "invisible")}
            aria-hidden={!filtersActive}
            tabIndex={filtersActive ? undefined : -1}
            onClick={() => {
              setFilter({ kind: "", category: "", from: "", to: "", missing_receipt: false });
              setSearch("");
            }}
          >
            {t("Reset filters")}
          </Button>
        </div>

        <Card className={cx(dragging && "border-dashed border-info")}>
          {page.error && (
            <div className="p-4">
              <ErrorNote message={page.error} />
            </div>
          )}
          {page.loading && !d ? (
            <Skeleton rows={8} />
          ) : items.length === 0 ? (
            <Empty
              icon={<ReceiptIcon className="size-6" />}
              title={dragging ? t("Release to create an expense") : filtersActive ? t("No results") : t("No transactions yet")}
              text={filtersActive ? t("Adjust your search or filters.") : t("Paid invoices show up here automatically. Add expenses yourself or drop a receipt here.")}
              action={!filtersActive && <Button onClick={() => setTarget({ draft: emptyDraft(), files: [] })}>{t("Record expense")}</Button>}
            />
          ) : (
            <table className="w-full table-fixed text-[13px]">
              <thead>
                <tr className="border-b border-line text-left text-xs text-muted">
                  <th className="w-28 py-2.5 pl-4 font-medium">{t("Date")}</th>
                  <th className="py-2.5 font-medium">{t("Description")}</th>
                  <th className="w-56 py-2.5 font-medium">{t("Category")}</th>
                  <th className="w-32 py-2.5 pr-4 text-right font-medium">{t("Amount")}</th>
                </tr>
              </thead>
              <tbody>
                {items.map((e) => (
                  <tr
                    key={`${e.kind}-${e.id}`}
                    onClick={() => (e.kind === "income" ? go("invoices", { open: e.id }) : setTarget({ id: e.id }))}
                    className="cursor-pointer border-b border-line last:border-0 hover:bg-subtle"
                  >
                    <td className="tabular-nums py-2.5 pl-4 text-muted">{date(e.date)}</td>
                    <td className="max-w-0 py-2.5 pr-3">
                      <div className="flex items-center gap-1.5">
                        <span className="truncate">{e.name}</span>
                        {e.recurring && <Repeat className="size-3 shrink-0 text-muted" aria-label={t("Recurring")} />}
                        {e.kind === "expense" &&
                          (e.receipt_count > 0 ? (
                            <Paperclip className="size-3 shrink-0 text-muted" aria-label={t("Receipt attached")} />
                          ) : (
                            <span title={t("No receipt attached")} className="inline-flex shrink-0">
                              <CircleAlert className="size-3 text-warn" aria-label={t("No receipt")} />
                            </span>
                          ))}
                      </div>
                      {e.description && <div className="truncate text-xs text-muted">{e.description}</div>}
                    </td>
                    <td className="py-2.5 pr-3">
                      {e.kind === "income" ? (
                        <Badge tone="pos">{t("Income")}</Badge>
                      ) : (
                        <CategoryBadge color={e.category_color}>{e.category_name ?? t("Uncategorised")}</CategoryBadge>
                      )}
                    </td>
                    <td className="py-2.5 pr-4 text-right">
                      <Amount cents={e.amount_cents} currency={e.currency} />
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
          {d && d.total > items.length && (
            <div className="border-t border-line px-4 py-3 text-center">
              <Button size="sm" onClick={() => setLimit(limit + PAGE)}>
                {t("Load more ({n})", { n: d.total - items.length })}
              </Button>
            </div>
          )}
        </Card>
        </>
      )}

      <ExpenseSheet target={target} categories={categories.data ?? []} onClose={closeSheet} onSaved={page.reload} onShowSeries={() => setTab("recurring")} />
    </div>
  );
}
