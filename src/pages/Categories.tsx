import { useState } from "react";
import { ask } from "@tauri-apps/plugin-dialog";
import { ArrowLeft, Plus } from "lucide-react";
import { api, errorMessage, type Category } from "../lib/api";
import { useAsync } from "../lib/hooks";
import { useApp } from "../lib/nav";
import { Button, Card, CategoryDot, Dialog, Empty, Field, Input, PageHeader, Skeleton, useToast } from "../components/ui";
import { t } from "../lib/i18n";

const PALETTE = ["#EF4444", "#F97316", "#92400E", "#EAB308", "#0EA5E9", "#1D4ED8", "#6366F1", "#A855F7", "#EC4899", "#BE123C", "#64748B", "#78716C"];

interface Draft {
  slug?: string;
  name: string;
  color: string;
  description: string;
}

export default function Categories() {
  const { go } = useApp();
  const toast = useToast();
  const categories = useAsync(() => api.listCategories(), []);
  const [draft, setDraft] = useState<Draft | null>(null);

  const edit = (c?: Category) =>
    setDraft(c ? { slug: c.slug, name: c.name, color: c.color, description: c.description ?? "" } : { name: "", color: PALETTE[0], description: "" });

  const save = async () => {
    if (!draft) return;
    try {
      await api.saveCategory({ slug: draft.slug, name: draft.name, color: draft.color, description: draft.description || null });
      setDraft(null);
      categories.reload();
    } catch (e) {
      toast.error(errorMessage(e));
    }
  };

  const remove = async (c: Category) => {
    const note = c.expense_count ? ` ${t("{n} expense(s) will become \"Uncategorised\".", { n: c.expense_count })}` : "";
    if (!(await ask(t("Delete category \"{name}\"?", { name: c.name }) + note, { title: t("Delete"), kind: "warning" }))) return;
    try {
      await api.deleteCategory(c.slug);
      setDraft(null);
      categories.reload();
    } catch (e) {
      toast.error(errorMessage(e));
    }
  };

  const cats = categories.data ?? [];
  const editing = draft?.slug ? cats.find((c) => c.slug === draft.slug) : undefined;

  return (
    <div className="flex flex-col gap-4">
      <PageHeader
        title={t("Categories")}
        subtitle={t("Use them to organise your expenses, e.g. for the EÜR tax form.")}
        actions={
          <>
            <Button variant="ghost" icon={<ArrowLeft className="size-3.5" />} onClick={() => go("ledger")}>
              {t("Income & expenses")}
            </Button>
            <Button variant="primary" icon={<Plus className="size-3.5" />} onClick={() => edit()}>
              {t("New category")}
            </Button>
          </>
        }
      />

      <Card>
        {categories.loading && !categories.data ? (
          <Skeleton />
        ) : cats.length === 0 ? (
          <Empty title={t("No categories")} action={<Button onClick={() => edit()}>{t("Create category")}</Button>} />
        ) : (
          <ul className="divide-y divide-line">
            {cats.map((c) => (
              <li key={c.slug}>
                <button type="button" onClick={() => edit(c)} className="flex w-full items-center gap-3 px-4 py-2.5 text-left hover:bg-subtle">
                  <CategoryDot color={c.color} />
                  <span className="min-w-0 flex-1">
                    <span className="block text-[13px] font-medium">{c.name}</span>
                    {c.description && <span className="block truncate text-xs text-muted">{c.description}</span>}
                  </span>
                  <span className="tabular-nums text-xs text-muted">{c.expense_count ? (c.expense_count === 1 ? t("1 expense") : t("{n} expenses", { n: c.expense_count })) : ""}</span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </Card>

      <Dialog
        open={draft !== null}
        onClose={() => setDraft(null)}
        title={draft?.slug ? t("Edit category") : t("New category")}
        footer={
          <>
            {editing && (
              <Button variant="danger" className="mr-auto" onClick={() => remove(editing)}>
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
            <Field label={t("Description")} hint={t("Optional, e.g. what belongs in it")}>
              <Input value={draft.description} onChange={(e) => setDraft({ ...draft, description: e.target.value })} />
            </Field>
            <Field label={t("Colour")}>
              <div className="flex flex-wrap gap-1.5">
                {PALETTE.map((c) => (
                  <button
                    key={c}
                    type="button"
                    aria-label={t("Colour {color}", { color: c })}
                    onClick={() => setDraft({ ...draft, color: c })}
                    className="size-6 rounded-full border-2"
                    style={{ background: c, borderColor: draft.color === c ? "var(--fg)" : "transparent" }}
                  />
                ))}
              </div>
            </Field>
          </>
        )}
      </Dialog>
    </div>
  );
}
