import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useId,
  useRef,
  useState,
  type CSSProperties,
  type ReactNode,
} from "react";
import { createPortal } from "react-dom";
import { AlertCircle, CheckCircle2, ChevronDown, Info, MoreHorizontal, X } from "lucide-react";
import { errorMessage } from "../lib/api";
import { money, STATUS_LABELS } from "../lib/format";
import { t } from "../lib/i18n";

import { Button, cx, FIELD, IconButton, Input } from "./ui-base";

export { Button, CategoryDot, cx, IconButton, Input } from "./ui-base";
export { AutocompleteInput, CurrencySelect, DateInput, DateTimeInput, PickSelect, Select, Textarea, type PickOption, type Suggestion } from "./ui-fields";

export function Field({ label, hint, children, className }: { label: string; hint?: ReactNode; children: ReactNode; className?: string }) {
  return (
    <label className={cx("flex flex-col gap-1", className)}>
      <span className="text-xs font-medium text-muted">{label}</span>
      {children}
      {hint && <span className="text-xs text-muted">{hint}</span>}
    </label>
  );
}

export function PercentInput({ value, onChange }: { value: string | number; onChange: (value: string) => void }) {
  return (
    <div className="relative">
      <Input type="number" min={0} max={100} value={value} className="w-full pr-7 tabular-nums text-right" onChange={(e) => onChange(e.target.value)} />
      <span className="pointer-events-none absolute top-1/2 right-2.5 -translate-y-1/2 text-xs text-muted">%</span>
    </div>
  );
}

export function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-3">
      <h3 className="text-xs font-semibold tracking-wide text-muted uppercase">{title}</h3>
      {children}
    </section>
  );
}

export function Toggle({ checked, onChange, label, hint }: { checked: boolean; onChange: (v: boolean) => void; label: string; hint?: string }) {
  return (
    <label className="flex cursor-pointer items-start justify-between gap-4">
      <span className="flex flex-col">
        <span className="text-[13px]">{label}</span>
        {hint && <span className="text-xs text-muted">{hint}</span>}
      </span>
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        onClick={() => onChange(!checked)}
        className={cx("relative mt-0.5 h-5 w-9 shrink-0 rounded-full transition-colors", checked ? "bg-accent" : "bg-line")}
      >
        <span className={cx("absolute top-0.5 size-4 rounded-full bg-panel shadow transition-all", checked ? "left-[18px]" : "left-0.5")} />
      </button>
    </label>
  );
}

export function Checkbox({ checked, onChange, label }: { checked: boolean; onChange: (v: boolean) => void; label?: string }) {
  return (
    <input
      type="checkbox"
      aria-label={label}
      checked={checked}
      onChange={(e) => onChange(e.target.checked)}
      onClick={(e) => e.stopPropagation()}
      className="size-3.5 cursor-pointer accent-[var(--accent)]"
    />
  );
}

function usePopover() {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => ref.current && !ref.current.contains(e.target as Node) && setOpen(false);
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);
  return { open, setOpen, ref };
}

type MenuItem = { label: string; icon?: ReactNode; onClick: () => void; danger?: boolean };

export function ActionMenu({
  items,
  label = t("More"),
}: {
  items: (MenuItem | false | null | undefined)[];
  label?: string;
}) {
  const { open, setOpen, ref } = usePopover();
  const visible = items.filter((i): i is MenuItem => Boolean(i));
  if (!visible.length) return null;
  return (
    <div ref={ref} className="relative">
      <Button size="sm" icon={<MoreHorizontal className="size-3.5" />} aria-haspopup="menu" aria-expanded={open} onClick={() => setOpen(!open)}>
        {label}
      </Button>
      {open && (
        <div role="menu" className="anim-pop absolute top-8 right-0 z-30 min-w-52 overflow-hidden rounded-lg border border-line bg-panel py-1 shadow-xl">
          {visible.map((item) => (
            <button
              key={item.label}
              type="button"
              role="menuitem"
              onClick={() => {
                setOpen(false);
                item.onClick();
              }}
              className={cx("flex w-full items-center gap-2 px-3 py-1.5 text-left text-[13px] hover:bg-subtle", item.danger ? "text-neg" : "text-fg")}
            >
              <span className={cx("inline-flex size-3.5 shrink-0 items-center justify-center", !item.danger && "text-muted")}>{item.icon}</span>
              {item.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

export function InfoDialog({ title, children, label = t("Info") }: { title: string; children: ReactNode; label?: string }) {
  const [open, setOpen] = useState(false);
  return (
    <>
      <IconButton label={label} onClick={() => setOpen(true)}>
        <Info className="size-3.5" />
      </IconButton>
      <Dialog open={open} onClose={() => setOpen(false)} title={title} wide>
        <div className="flex flex-col gap-3 text-[13px] leading-relaxed text-muted">{children}</div>
      </Dialog>
    </>
  );
}

export function MultiSelect({
  options,
  value,
  onChange,
  allLabel,
  pluralLabel,
  icon,
  className,
}: {
  options: { value: string; label: string; count?: number }[];
  value: string[];
  onChange: (v: string[]) => void;
  allLabel: string;
  pluralLabel: string;
  icon?: ReactNode;
  className?: string;
}) {
  const { open, setOpen, ref } = usePopover();
  const toggle = (v: string) => onChange(value.includes(v) ? value.filter((x) => x !== v) : [...value, v]);
  const summary =
    value.length === 0 ? allLabel : value.length === 1 ? (options.find((o) => o.value === value[0])?.label ?? allLabel) : `${value.length} ${pluralLabel}`;

  return (
    <div ref={ref} className={cx("relative", className)}>
      <button
        type="button"
        aria-haspopup="listbox"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
        className={cx(FIELD, "flex h-8 w-full items-center gap-1.5 text-left", value.length > 0 && "border-muted")}
      >
        {icon && <span className="shrink-0 text-muted">{icon}</span>}
        <span className="min-w-0 flex-1 truncate">{summary}</span>
        <ChevronDown className="size-3.5 shrink-0 text-muted" />
      </button>
      {open && (
        <div className="anim-pop absolute top-9 left-0 z-30 w-80 max-w-[90vw] overflow-hidden rounded-lg border border-line bg-panel shadow-xl">
          <ul role="listbox" aria-multiselectable className="max-h-72 overflow-y-auto py-1">
            {options.map((o) => (
              <li key={o.value}>
                <label className="flex cursor-pointer items-center gap-2.5 px-3 py-1.5 text-[13px] hover:bg-subtle">
                  <Checkbox checked={value.includes(o.value)} onChange={() => toggle(o.value)} />
                  <span className="min-w-0 flex-1 truncate" title={o.label}>
                    {o.label}
                  </span>
                  {o.count !== undefined && <span className="tabular text-[11px] text-muted">{o.count}</span>}
                </label>
              </li>
            ))}
          </ul>
          {value.length > 0 && (
            <div className="border-t border-line px-3 py-1.5">
              <button type="button" onClick={() => onChange([])} className="text-xs text-muted hover:text-fg">
                {t("Clear selection")}
              </button>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

type Tone = "neutral" | "pos" | "neg" | "warn" | "info";

const TONES: Record<Tone, string> = {
  neutral: "bg-subtle text-muted",
  pos: "bg-pos/12 text-pos",
  neg: "bg-neg/12 text-neg",
  warn: "bg-warn/14 text-warn",
  info: "bg-info/12 text-info",
};

export function Badge({ tone = "neutral", children, className, style }: { tone?: Tone; children: ReactNode; className?: string; style?: CSSProperties }) {
  return <span className={cx("inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-[11px] font-medium whitespace-nowrap", TONES[tone], className)} style={style}>{children}</span>;
}

export function CategoryBadge({ color, children, className }: { color: string | null | undefined; children: ReactNode; className?: string }) {
  return (
    <Badge className={cx("max-w-full", className)} style={color ? { background: `color-mix(in srgb, ${color} 12%, transparent)`, color } : undefined}>
      <span className="truncate">{children}</span>
    </Badge>
  );
}

const STATUS_TONES: Record<string, Tone> = {
  draft: "neutral",
  unpaid: "info",
  overdue: "neg",
  paid: "pos",
  canceled: "neutral",
  scheduled: "warn",
  refunded: "warn",
};

export function StatusBadge({ status }: { status: string }) {
  return <Badge tone={STATUS_TONES[status] ?? "neutral"}>{STATUS_LABELS[status] ?? status}</Badge>;
}

export function Amount({ cents, currency = "EUR", className, signed = true }: { cents: number; currency?: string; className?: string; signed?: boolean }) {
  return (
    <span className={cx("tabular whitespace-nowrap", signed && cents > 0 && "text-pos", className)}>{money(cents, currency, { sign: signed })}</span>
  );
}

export const prefersReducedMotion = () => window.matchMedia("(prefers-reduced-motion: reduce)").matches;

export function AnimatedMoney({ cents, currency = "EUR", duration = 600, className }: { cents: number; currency?: string; duration?: number; className?: string }) {
  const [value, setValue] = useState(() => (prefersReducedMotion() ? cents : 0));
  const current = useRef(value);
  useEffect(() => {
    if (prefersReducedMotion()) {
      current.current = cents;
      setValue(cents);
      return;
    }
    const from = current.current;
    if (from === cents) return;
    const start = performance.now();
    let frame = requestAnimationFrame(function tick(now) {
      const progress = Math.min(1, Math.max(0, (now - start) / duration));
      current.current = progress === 1 ? cents : from + (cents - from) * (1 - Math.pow(1 - progress, 4));
      setValue(current.current);
      if (progress < 1) frame = requestAnimationFrame(tick);
    });
    return () => cancelAnimationFrame(frame);
  }, [cents, duration]);
  return (
    <span className={cx("inline-grid justify-items-end", className)}>
      <span data-final aria-hidden="true" className="invisible col-start-1 row-start-1">{money(cents, currency)}</span>
      <span data-count aria-hidden="true" className="col-start-1 row-start-1">{money(Math.round(value), currency)}</span>
      <span className="sr-only">{money(cents, currency)}</span>
    </span>
  );
}

export function Card({ children, className, title, actions }: { children: ReactNode; className?: string; title?: string; actions?: ReactNode }) {
  return (
    <section className={cx("surface rounded-[10px] border border-line bg-panel", className)}>
      {(title || actions) && (
        <header className="flex h-11 items-center justify-between gap-2 border-b border-line px-4">
          {title && <h2 className="text-[13px] font-medium">{title}</h2>}
          {actions}
        </header>
      )}
      {children}
    </section>
  );
}

export function CardBody({ title, children, className }: { title?: string; children: ReactNode; className?: string }) {
  return (
    <div className={cx("flex flex-col gap-4 p-4", className)}>
      {title && <h3 className="text-[13px] font-medium">{title}</h3>}
      {children}
    </div>
  );
}

export function Kpi({
  label,
  value,
  sub,
  tone,
  onClick,
}: {
  label: string;
  value: ReactNode;
  sub?: ReactNode;
  tone?: "pos" | "neg" | "warn";
  onClick?: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={!onClick}
      className="surface flex min-w-0 flex-col gap-1 rounded-[10px] border border-line bg-panel p-4 text-left enabled:hover:bg-subtle enabled:hover:bg-none disabled:cursor-default"
    >
      <span className="text-xs text-muted">{label}</span>
      <span className={cx("tabular text-xl font-semibold tracking-tight", tone === "pos" && "text-pos", tone === "neg" && "text-neg", tone === "warn" && "text-warn")}>
        {value}
      </span>
      {sub && <span className="text-xs text-muted">{sub}</span>}
    </button>
  );
}

export function PageHeader({ title, subtitle, actions }: { title: string; subtitle: ReactNode; actions?: ReactNode }) {
  return (
    <div className="flex flex-wrap items-end justify-between gap-3 pb-4">
      <div>
        <h1 className="font-serif text-2xl leading-tight font-normal tracking-tight">{title}</h1>
        <p className="mt-0.5 text-[13px] text-muted">{subtitle}</p>
      </div>
      {actions && <div className="flex flex-wrap items-center gap-2">{actions}</div>}
    </div>
  );
}

export function Empty({ icon, title, text, action }: { icon?: ReactNode; title: string; text?: ReactNode; action?: ReactNode }) {
  return (
    <div className="flex flex-col items-center justify-center gap-2 px-6 py-14 text-center">
      {icon && <div className="mb-1 text-muted">{icon}</div>}
      <p className="text-[13px] font-medium">{title}</p>
      {text && <p className="max-w-sm text-[13px] text-muted">{text}</p>}
      {action && <div className="mt-2">{action}</div>}
    </div>
  );
}

export function ErrorNote({ message }: { message: string }) {
  return (
    <div className="flex items-center gap-2 rounded-md border border-neg/30 bg-neg/8 px-3 py-2 text-[13px] text-neg">
      <AlertCircle className="size-4 shrink-0" />
      <span data-selectable>{message}</span>
    </div>
  );
}

export function Skeleton({ rows = 5, height }: { rows?: number; height?: number }) {
  if (height) return <div className="animate-pulse rounded bg-subtle" style={{ height }} />;
  return (
    <div className="flex flex-col gap-2 p-4">
      {Array.from({ length: rows }).map((_, i) => (
        <div key={i} className="h-7 animate-pulse rounded bg-subtle" />
      ))}
    </div>
  );
}

export function Tabs<T extends string>({
  items,
  value,
  onChange,
  actions,
}: {
  items: { value: T; label: string; count?: number }[];
  value: T;
  onChange: (v: T) => void;
  actions?: ReactNode;
}) {
  return (
    <div className="flex min-h-9 items-end gap-1 border-b border-line">
      {items.map((it) => (
        <button
          key={it.value}
          type="button"
          onClick={() => onChange(it.value)}
          className={cx(
            "-mb-px flex items-center gap-1.5 border-b-2 px-2.5 py-2 text-[13px]",
            value === it.value ? "border-fg font-medium text-fg" : "border-transparent text-muted hover:text-fg",
          )}
        >
          {it.label}
          {it.count !== undefined && it.count > 0 && <span className="rounded bg-subtle px-1 text-[11px] text-muted">{it.count}</span>}
        </button>
      ))}
      {actions && <div className="ml-auto flex items-center gap-2 pb-1">{actions}</div>}
    </div>
  );
}

export function Segmented<T extends string>({ items, value, onChange }: { items: { value: T; label: string }[]; value: T; onChange: (v: T) => void }) {
  return (
    <div className="flex rounded-md border border-line p-0.5 text-xs">
      {items.map((it) => (
        <button
          key={it.value}
          type="button"
          onClick={() => onChange(it.value)}
          className={cx("rounded px-2 py-0.5", value === it.value ? "bg-subtle font-medium text-fg" : "text-muted hover:text-fg")}
        >
          {it.label}
        </button>
      ))}
    </div>
  );
}

export function TagInput({ value, onChange, suggestions = [], placeholder = t("Add tag…") }: { value: string[]; onChange: (v: string[]) => void; suggestions?: string[]; placeholder?: string }) {
  const [draft, setDraft] = useState("");
  const add = (raw: string) => {
    const tag = raw.trim().replace(/,$/, "");
    if (tag && !value.some((v) => v.toLowerCase() === tag.toLowerCase())) onChange([...value, tag]);
    setDraft("");
  };
  const listId = useId();
  return (
    <div className="flex min-h-8 flex-wrap items-center gap-1 rounded-md border border-line bg-panel px-1.5 py-1">
      {value.map((tag) => (
        <span key={tag} className="inline-flex items-center gap-1 rounded bg-subtle px-1.5 py-0.5 text-xs">
          {tag}
          <button type="button" aria-label={t("Remove {tag}", { tag })} onClick={() => onChange(value.filter((v) => v !== tag))} className="text-muted hover:text-fg">
            <X className="size-3" />
          </button>
        </span>
      ))}
      <input
        list={listId}
        value={draft}
        placeholder={value.length ? "" : placeholder}
        onChange={(e) => (e.target.value.endsWith(",") ? add(e.target.value) : setDraft(e.target.value))}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            add(draft);
          } else if (e.key === "Backspace" && !draft && value.length) {
            onChange(value.slice(0, -1));
          }
        }}
        onBlur={() => draft && add(draft)}
        className="min-w-[90px] flex-1 bg-transparent px-1 text-[13px] outline-none placeholder:text-muted"
      />
      <datalist id={listId}>
        {suggestions.filter((s) => !value.includes(s)).map((s) => (
          <option key={s} value={s} />
        ))}
      </datalist>
    </div>
  );
}

const EXIT_MS = 140;

export type PresenceState = "pre" | "open" | "closed";

export function usePresence(open: boolean): { visible: boolean; state: PresenceState } {
  const [mounted, setMounted] = useState(open);
  const [entered, setEntered] = useState(false);
  useEffect(() => {
    if (open) {
      setMounted(true);
      let inner = 0;
      const outer = requestAnimationFrame(() => {
        inner = requestAnimationFrame(() => setEntered(true));
      });
      return () => {
        cancelAnimationFrame(outer);
        cancelAnimationFrame(inner);
      };
    }
    setEntered(false);
    const timer = setTimeout(() => setMounted(false), EXIT_MS);
    return () => clearTimeout(timer);
  }, [open]);
  return { visible: open || mounted, state: !open ? "closed" : entered ? "open" : "pre" };
}

function useFrozen<T>(value: T, live: boolean): T {
  const ref = useRef(value);
  if (live) ref.current = value;
  return ref.current;
}

function useEscape(open: boolean, onClose: () => void) {
  useEffect(() => {
    if (!open) return;
    const handler = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [open, onClose]);
}

export function Sheet({
  open,
  onClose,
  title,
  children,
  footer,
  wide,
  extraWide,
}: {
  open: boolean;
  onClose: () => void;
  title: ReactNode;
  children: ReactNode;
  footer?: ReactNode;
  wide?: boolean;
  extraWide?: boolean;
}) {
  useEscape(open, onClose);
  const { visible, state } = usePresence(open);
  const content = useFrozen({ title, children, footer }, open);
  if (!visible) return null;
  return createPortal(
    <div className="fixed inset-0 z-40 flex justify-end" data-state={state}>
      <div className="anim-overlay absolute inset-0 bg-overlay" onClick={onClose} />
      <aside
        role="dialog"
        aria-modal="true"
        className={cx("anim-sheet relative flex h-full w-full flex-col border-l border-line bg-panel shadow-2xl", extraWide ? "max-w-[980px]" : wide ? "max-w-[820px]" : "max-w-[480px]")}
      >
        <header className="flex items-center justify-between gap-2 border-b border-line px-4 py-3">
          <h2 className="truncate text-[15px] font-semibold">{content.title}</h2>
          <IconButton label={t("Close")} onClick={onClose}>
            <X className="size-4" />
          </IconButton>
        </header>
        <div className="flex-1 overflow-y-auto p-4">{content.children}</div>
        {content.footer && <footer className="flex flex-wrap items-center justify-end gap-2 border-t border-line px-4 py-3">{content.footer}</footer>}
      </aside>
    </div>,
    document.body,
  );
}

export function Dialog({
  open,
  onClose,
  title,
  children,
  footer,
  wide,
}: {
  open: boolean;
  onClose: () => void;
  title: ReactNode;
  children: ReactNode;
  footer?: ReactNode;
  wide?: boolean;
}) {
  useEscape(open, onClose);
  const { visible, state } = usePresence(open);
  const content = useFrozen({ title, children, footer }, open);
  if (!visible) return null;
  return createPortal(
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4" data-state={state}>
      <div className="anim-overlay absolute inset-0 bg-overlay backdrop-blur-sm" onClick={onClose} />
      <div role="dialog" aria-modal="true" className={cx("anim-dialog relative w-full rounded-xl border border-line bg-panel shadow-2xl", wide ? "max-w-2xl" : "max-w-md")}>
        <header className="flex items-center justify-between gap-2 border-b border-line px-4 py-3">
          <h2 className="text-[15px] font-semibold">{content.title}</h2>
          <IconButton label={t("Close")} onClick={onClose}>
            <X className="size-4" />
          </IconButton>
        </header>
        <div className="flex flex-col gap-4 p-4">{content.children}</div>
        {content.footer && <footer className="flex justify-end gap-2 border-t border-line px-4 py-3">{content.footer}</footer>}
      </div>
    </div>,
    document.body,
  );
}

interface ToastItem {
  id: number;
  kind: "success" | "warn" | "error";
  text: string;
}

const ToastContext = createContext<{ success: (t: string) => void; warn: (t: string) => void; error: (t: string) => void }>({
  success: () => {},
  warn: () => {},
  error: () => {},
});

export function ToastProvider({ children }: { children: ReactNode }) {
  const [items, setItems] = useState<ToastItem[]>([]);
  const push = useCallback((kind: ToastItem["kind"], text: string) => {
    const id = Date.now() + Math.random();
    setItems((xs) => [...xs, { id, kind, text }]);
    setTimeout(() => setItems((xs) => xs.filter((x) => x.id !== id)), kind === "success" ? 3500 : 7000);
  }, []);
  useEffect(() => {
    const onRejection = (e: PromiseRejectionEvent) => push("error", errorMessage(e.reason));
    window.addEventListener("unhandledrejection", onRejection);
    return () => window.removeEventListener("unhandledrejection", onRejection);
  }, [push]);
  const value = { success: (t: string) => push("success", t), warn: (t: string) => push("warn", t), error: (t: string) => push("error", t) };
  return (
    <ToastContext.Provider value={value}>
      {children}
      <div className="pointer-events-none fixed right-4 bottom-4 z-[60] flex w-[340px] flex-col gap-2" aria-live="polite">
        {items.map((t) => (
          <div key={t.id} className="anim-toast pointer-events-auto flex items-center gap-2 rounded-lg border border-line bg-panel px-4 py-3 text-[13px] shadow-lg">
            {t.kind === "success" ? <CheckCircle2 className="size-4 shrink-0 text-pos" /> : <AlertCircle className={cx("size-4 shrink-0", t.kind === "warn" ? "text-warn" : "text-neg")} />}
            <span data-selectable>{t.text}</span>
          </div>
        ))}
      </div>
    </ToastContext.Provider>
  );
}

export function useToast() {
  return useContext(ToastContext);
}
