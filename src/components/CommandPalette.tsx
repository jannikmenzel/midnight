import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { ArrowLeftRight, FileText, FolderLock, Plus, Search, Timer, Upload, User } from "lucide-react";
import { api, type SearchHit } from "../lib/api";
import { date, money } from "../lib/format";
import { useDebounced } from "../lib/hooks";
import { useApp, type Route, type RouteParams } from "../lib/nav";
import { cx, usePresence } from "./ui";
import { t } from "../lib/i18n";

interface Item {
  id: string;
  group: string;
  title: string;
  subtitle?: string | null;
  meta?: string;
  icon: ReactNode;
  run: () => void;
}

const KIND: Record<SearchHit["kind"], { group: string; route: Route; icon: ReactNode }> = {
  expense: { group: t("Expenses"), route: "ledger", icon: <ArrowLeftRight className="size-4" /> },
  invoice: { group: t("Invoices"), route: "invoices", icon: <FileText className="size-4" /> },
  customer: { group: t("Customers"), route: "customers", icon: <User className="size-4" /> },
  project: { group: t("Projects"), route: "tracker", icon: <Timer className="size-4" /> },
  document: { group: "Vault", route: "vault", icon: <FolderLock className="size-4" /> },
};

export function CommandPalette({ open, onClose }: { open: boolean; onClose: () => void }) {
  const { go } = useApp();
  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<SearchHit[]>([]);
  const [active, setActive] = useState(0);
  const debounced = useDebounced(query, 180);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (open) {
      setQuery("");
      setHits([]);
      setActive(0);
      setTimeout(() => inputRef.current?.focus(), 0);
    }
  }, [open]);

  useEffect(() => {
    if (!open || debounced.trim().length < 2) {
      setHits([]);
      return;
    }
    api.search(debounced).then(setHits).catch(() => setHits([]));
  }, [debounced, open]);

  const navigate = (route: Route, params?: RouteParams) => {
    go(route, params);
    onClose();
  };

  const items: Item[] = useMemo(() => {
    const actions: Item[] = [
      { id: "a-invoice", group: t("Actions"), title: t("New invoice"), icon: <Plus className="size-4" />, run: () => navigate("invoices", { new: "1" }) },
      { id: "a-expense", group: t("Actions"), title: t("Record expense"), icon: <Plus className="size-4" />, run: () => navigate("ledger", { new: "1" }) },
      { id: "a-receipt", group: t("Actions"), title: t("Upload receipt"), icon: <Upload className="size-4" />, run: () => navigate("ledger", { upload: "1" }) },
      { id: "a-timer", group: t("Actions"), title: t("Open time tracking"), icon: <Timer className="size-4" />, run: () => navigate("tracker") },
      { id: "a-customer", group: t("Actions"), title: t("New customer"), icon: <User className="size-4" />, run: () => navigate("customers", { new: "1" }) },
    ];
    const q = query.trim().toLowerCase();
    const filteredActions = q ? actions.filter((a) => a.title.toLowerCase().includes(q)) : actions;
    const found: Item[] = hits.map((h) => ({
      id: `${h.kind}-${h.id}`,
      group: KIND[h.kind].group,
      title: h.title,
      subtitle: h.subtitle,
      meta: [h.date ? date(h.date) : null, h.amount_cents !== null ? money(h.amount_cents, h.currency ?? "EUR") : null].filter(Boolean).join(" · "),
      icon: KIND[h.kind].icon,
      run: () => navigate(KIND[h.kind].route, { open: h.id }),
    }));
    return [...found, ...filteredActions];
  }, [hits, query]);

  useEffect(() => setActive(0), [items.length]);

  const { visible, state } = usePresence(open);
  if (!visible) return null;

  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setActive((a) => Math.min(a + 1, items.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActive((a) => Math.max(a - 1, 0));
    } else if (e.key === "Enter") {
      items[active]?.run();
    } else if (e.key === "Escape") {
      onClose();
    }
  };

  let lastGroup = "";
  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center pt-[12vh]" data-state={state}>
      <div className="anim-overlay absolute inset-0 bg-overlay" onClick={onClose} />
      <div role="dialog" aria-modal="true" aria-label={t("Search")} className="anim-dialog relative w-full max-w-xl overflow-hidden rounded-xl border border-line bg-panel shadow-2xl">
        <div className="flex items-center gap-2 border-b border-line px-4">
          <Search className="size-4 text-muted" />
          <input
            ref={inputRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={onKey}
            placeholder={t("Expenses, invoices, customers, projects, documents…")}
            className="h-12 flex-1 bg-transparent text-[14px] outline-none placeholder:text-muted"
          />
        </div>
        <ul className="max-h-[52vh] overflow-y-auto py-1" role="listbox">
          {items.length === 0 && <li className="px-4 py-6 text-center text-[13px] text-muted">{t("No results")}</li>}
          {items.map((it, i) => {
            const header = it.group !== lastGroup ? it.group : null;
            lastGroup = it.group;
            return (
              <li key={it.id}>
                {header && <div className="px-4 pt-2 pb-1 text-[11px] font-medium tracking-wide text-muted uppercase">{header}</div>}
                <button
                  type="button"
                  role="option"
                  aria-selected={i === active}
                  onMouseEnter={() => setActive(i)}
                  onClick={it.run}
                  className={cx("flex w-full items-center gap-3 px-4 py-2.5 text-left", i === active && "bg-subtle")}
                >
                  <span className="text-muted">{it.icon}</span>
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-[13px]">{it.title}</span>
                    {it.subtitle && <span className="block truncate text-xs text-muted">{it.subtitle}</span>}
                  </span>
                  {it.meta && <span className="tabular-nums shrink-0 text-xs text-muted">{it.meta}</span>}
                </button>
              </li>
            );
          })}
        </ul>
      </div>
    </div>
  );
}
