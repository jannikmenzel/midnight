import {
  Children,
  Fragment,
  createContext,
  isValidElement,
  forwardRef,
  useCallback,
  useContext,
  useEffect,
  useId,
  useRef,
  useState,
  type ButtonHTMLAttributes,
  type ChangeEvent,
  type CSSProperties,
  type InputHTMLAttributes,
  type KeyboardEvent as ReactKeyboardEvent,
  type ReactNode,
  type SelectHTMLAttributes,
  type TextareaHTMLAttributes,
} from "react";
import { createPortal } from "react-dom";
import { AlertCircle, CalendarDays, Check, CheckCircle2, ChevronDown, ChevronLeft, ChevronRight, Clock, Info, Loader2, MoreHorizontal, X } from "lucide-react";
import { errorMessage } from "../lib/api";
import { date as formatDate, money, STATUS_LABELS } from "../lib/format";
import { lang, locale, t } from "../lib/i18n";

export function cx(...classes: (string | false | null | undefined)[]): string {
  return classes.filter(Boolean).join(" ");
}

type ButtonVariant = "primary" | "secondary" | "ghost" | "danger";

const BUTTON_VARIANTS: Record<ButtonVariant, string> = {
  primary: "bg-accent text-accent-fg hover:opacity-90",
  secondary: "border border-line bg-panel text-fg hover:bg-subtle",
  ghost: "text-fg hover:bg-subtle",
  danger: "border border-line bg-panel text-neg hover:bg-subtle",
};

export const Button = forwardRef<
  HTMLButtonElement,
  ButtonHTMLAttributes<HTMLButtonElement> & { variant?: ButtonVariant; size?: "sm" | "md"; loading?: boolean; icon?: ReactNode }
>(function Button({ variant = "secondary", size = "md", loading, icon, className, children, disabled, ...rest }, ref) {
  return (
    <button
      ref={ref}
      type="button"
      disabled={disabled || loading}
      className={cx(
        "inline-flex shrink-0 items-center justify-center gap-1.5 rounded-md font-medium whitespace-nowrap transition-colors disabled:cursor-not-allowed disabled:opacity-50",
        size === "sm" ? "h-7 px-2.5 text-xs" : "h-8 px-3 text-[13px]",
        BUTTON_VARIANTS[variant],
        className,
      )}
      {...rest}
    >
      {loading ? <Loader2 className="size-3.5 animate-spin" /> : icon}
      {children}
    </button>
  );
});

export function IconButton({ label, className, ...rest }: ButtonHTMLAttributes<HTMLButtonElement> & { label: string }) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      className={cx("inline-flex size-7 items-center justify-center rounded-md text-muted hover:bg-subtle hover:text-fg", className)}
      {...rest}
    />
  );
}

function fieldWidth(className?: string) {
  return /(^|\s)w-/.test(className ?? "") ? "" : "w-full";
}

const FIELD = "rounded-md border border-line bg-panel px-2.5 text-[13px] text-fg outline-none placeholder:text-muted focus:border-muted disabled:opacity-60";

export const Input = forwardRef<HTMLInputElement, InputHTMLAttributes<HTMLInputElement>>(function Input({ className, ...rest }, ref) {
  return <input ref={ref} className={cx(FIELD, fieldWidth(className), "h-8", className)} {...rest} />;
});

const pad = (n: number) => String(n).padStart(2, "0");
const isoOf = (d: Date) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;

function parseDate(text: string): string | null {
  const s = text.trim();
  let m = s.match(/^(\d{4})-(\d{1,2})-(\d{1,2})$/);
  const [y, mo, d] = m ? [+m[1], +m[2], +m[3]] : (m = s.match(/^(\d{1,2})[./](\d{1,2})[./](\d{2}|\d{4})$/)) ? [+m[3] < 100 ? 2000 + +m[3] : +m[3], +m[2], +m[1]] : [0, 0, 0];
  const date = new Date(y, mo - 1, d);
  return y && date.getMonth() === mo - 1 && date.getDate() === d ? isoOf(date) : null;
}

function useFloatingPanel(height: number, width: number) {
  const [pos, setPos] = useState<{ top?: number; bottom?: number; left: number; width: number } | null>(null);
  const anchor = useRef<HTMLDivElement>(null);
  const popup = useRef<HTMLDivElement>(null);
  const open = pos !== null;

  const show = () => {
    if (!anchor.current) return;
    const r = anchor.current.getBoundingClientRect();
    const w = Math.max(width, r.width);
    const alignRight = r.left + r.width / 2 > window.innerWidth / 2;
    const left = Math.max(8, Math.min(alignRight ? r.right - w : r.left, window.innerWidth - w - 8));
    const above = r.bottom + height + 8 > window.innerHeight && r.top > window.innerHeight - r.bottom;
    setPos(above ? { bottom: window.innerHeight - r.top + 4, left, width: w } : { top: r.bottom + 4, left, width: w });
  };
  const hide = () => setPos(null);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (!anchor.current?.contains(e.target as Node) && !popup.current?.contains(e.target as Node)) hide();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.stopPropagation();
      hide();
    };
    const onScroll = (e: Event) => !popup.current?.contains(e.target as Node) && hide();
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey, true);
    window.addEventListener("scroll", onScroll, true);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey, true);
      window.removeEventListener("scroll", onScroll, true);
    };
  }, [open]);

  const panel = (children: ReactNode, className?: string) =>
    pos &&
    createPortal(
      <div
        ref={popup}
        style={{ top: pos.top, bottom: pos.bottom, left: pos.left, width: pos.width, maxHeight: (pos.top !== undefined ? window.innerHeight - pos.top : window.innerHeight - pos.bottom!) - 8 }}
        className={cx("anim-pop fixed z-[100] flex flex-col overflow-auto rounded-lg border border-line bg-panel shadow-xl", pos.bottom !== undefined && "anim-pop-up origin-bottom", className)}
        onMouseDown={(e) => e.preventDefault()}
      >
        {children}
      </div>,
      document.body,
    );

  return { open, show, hide, anchor, panel };
}

export function DateInput({
  value,
  onChange,
  min,
  disabled,
  autoFocus,
  className,
  "aria-label": ariaLabel,
}: {
  value: string;
  onChange: (iso: string) => void;
  min?: string;
  disabled?: boolean;
  autoFocus?: boolean;
  className?: string;
  "aria-label"?: string;
}) {
  const [text, setText] = useState(value ? formatDate(value) : "");
  const [month, setMonth] = useState(() => (value ? new Date(`${value.slice(0, 7)}-01T00:00`) : new Date()));
  const popover = useFloatingPanel(360, 320);

  useEffect(() => setText(value ? formatDate(value) : ""), [value]);

  const show = () => {
    if (disabled) return;
    setMonth(value ? new Date(`${value.slice(0, 7)}-01T00:00`) : new Date());
    popover.show();
  };

  const commit = () => {
    if (!text.trim()) return value && onChange("");
    const iso = parseDate(text);
    if (iso && iso !== value) onChange(iso);
    else setText(value ? formatDate(value) : "");
  };
  const pick = (iso: string) => {
    onChange(iso);
    popover.hide();
  };

  const today = isoOf(new Date());
  const first = new Date(month.getFullYear(), month.getMonth(), 1);
  const offset = (first.getDay() + 6) % 7;
  const days = Array.from({ length: 42 }, (_, i) => new Date(month.getFullYear(), month.getMonth(), 1 - offset + i));
  const weekdays = Array.from({ length: 7 }, (_, i) => new Date(2024, 0, 1 + i).toLocaleDateString(locale(), { weekday: "short" }).replace(".", ""));
  const shift = (n: number) => setMonth(new Date(month.getFullYear(), month.getMonth() + n, 1));

  return (
    <div ref={popover.anchor} className={cx("relative", fieldWidth(className), className)}>
      <input
        className={cx(FIELD, "h-8 w-full pr-8")}
        value={text}
        placeholder={lang() === "de" ? "TT.MM.JJJJ" : "DD/MM/YYYY"}
        aria-label={ariaLabel}
        disabled={disabled}
        autoFocus={autoFocus}
        onChange={(e) => setText(e.target.value)}
        onFocus={show}
        onClick={() => !popover.open && show()}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            commit();
            popover.hide();
          }
        }}
      />
      <CalendarDays className="pointer-events-none absolute top-2 right-2.5 size-4 text-muted" />
      {popover.panel(
        <>
            <div className="mb-2 flex items-center justify-between">
              <button type="button" onClick={() => shift(-1)} aria-label={t("Previous month")} className="rounded-md p-1.5 text-muted hover:bg-subtle hover:text-fg">
                <ChevronLeft className="size-4" />
              </button>
              <span className="text-sm font-medium">{month.toLocaleDateString(locale(), { month: "long", year: "numeric" })}</span>
              <button type="button" onClick={() => shift(1)} aria-label={t("Next month")} className="rounded-md p-1.5 text-muted hover:bg-subtle hover:text-fg">
                <ChevronRight className="size-4" />
              </button>
            </div>
            <div className="grid grid-cols-7 gap-0.5 text-center">
              {weekdays.map((w) => (
                <div key={w} className="pb-1 text-[11px] text-muted">
                  {w}
                </div>
              ))}
              {days.map((d) => {
                const iso = isoOf(d);
                const outside = d.getMonth() !== month.getMonth();
                const blocked = !!min && iso < min;
                return (
                  <button
                    key={iso}
                    type="button"
                    disabled={blocked}
                    onClick={() => pick(iso)}
                    className={cx(
                      "tabular-nums h-10 rounded-md text-[13px]",
                      iso === value ? "bg-accent font-medium text-accent-fg" : "hover:bg-subtle",
                      iso === today && iso !== value && "font-semibold ring-1 ring-line ring-inset",
                      outside && iso !== value && "text-muted",
                      blocked && "opacity-30 hover:bg-transparent",
                    )}
                  >
                    {d.getDate()}
                  </button>
                );
              })}
            </div>
            <div className="mt-2 flex justify-between border-t border-line pt-2">
              <button type="button" onClick={() => pick(today)} disabled={!!min && today < min} className="rounded-md px-2 py-1 text-xs hover:bg-subtle disabled:opacity-40">
                {t("Today")}
              </button>
              {value && (
                <button type="button" onClick={() => pick("")} className="rounded-md px-2 py-1 text-xs text-muted hover:bg-subtle hover:text-fg">
                  {t("Clear")}
                </button>
              )}
            </div>
        </>,
        "p-3",
      )}
    </div>
  );
}

function parseTime(text: string): string | null {
  const m = text.trim().match(/^(\d{1,2})(?:[:.]?(\d{2}))?$/);
  if (!m) return null;
  const [h, min] = [+m[1], +(m[2] ?? 0)];
  return h < 24 && min < 60 ? `${pad(h)}:${pad(min)}` : null;
}

const TIME_SLOTS = Array.from({ length: 96 }, (_, i) => `${pad(Math.floor(i / 4))}:${pad((i % 4) * 15)}`);

function TimeInput({
  value,
  onChange,
  disabled,
  className,
  "aria-label": ariaLabel,
}: {
  value: string;
  onChange: (hhmm: string) => void;
  disabled?: boolean;
  className?: string;
  "aria-label"?: string;
}) {
  const [text, setText] = useState(value);
  const popover = useFloatingPanel(280, 120);
  const list = useRef<HTMLUListElement>(null);

  useEffect(() => setText(value), [value]);
  useEffect(() => {
    if (!popover.open) return;
    const near = TIME_SLOTS.findIndex((s) => s >= (value || "08:00"));
    list.current?.children[Math.max(0, near)]?.scrollIntoView({ block: "center" });
  }, [popover.open]);

  const commit = () => {
    const hhmm = parseTime(text);
    if (hhmm && hhmm !== value) onChange(hhmm);
    else setText(value);
  };
  const pick = (hhmm: string) => {
    onChange(hhmm);
    popover.hide();
  };

  return (
    <div ref={popover.anchor} className={cx("relative", fieldWidth(className), className)}>
      <input
        className={cx(FIELD, "tabular-nums h-8 w-full pr-8")}
        value={text}
        placeholder="HH:MM"
        aria-label={ariaLabel}
        disabled={disabled}
        onChange={(e) => setText(e.target.value)}
        onFocus={() => !disabled && popover.show()}
        onClick={() => !disabled && !popover.open && popover.show()}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            commit();
            popover.hide();
          }
        }}
      />
      <Clock className="pointer-events-none absolute top-2 right-2.5 size-4 text-muted" />
      {popover.panel(
        <ul ref={list} className="max-h-[17rem] min-h-0 flex-1 overflow-y-auto p-1">
          {TIME_SLOTS.map((s) => (
            <li key={s}>
              <button
                type="button"
                onClick={() => pick(s)}
                className={cx("tabular-nums w-full rounded-md px-2.5 py-1.5 text-left text-[13px]", s === value ? "bg-accent font-medium text-accent-fg" : "hover:bg-subtle")}
              >
                {s}
              </button>
            </li>
          ))}
        </ul>,
      )}
    </div>
  );
}

export function DateTimeInput({ value, onChange, disabled }: { value: string; onChange: (value: string) => void; disabled?: boolean }) {
  const [date, hhmm = ""] = value ? value.split("T") : ["", ""];
  return (
    <div className="flex gap-2">
      <DateInput value={date} onChange={(d) => onChange(d ? `${d}T${hhmm || "00:00"}` : "")} disabled={disabled} className="min-w-0 flex-1" aria-label={t("Date")} />
      <TimeInput value={hhmm} onChange={(h) => onChange(`${date || isoOf(new Date())}T${h}`)} disabled={disabled} className="w-24" aria-label={t("Time")} />
    </div>
  );
}

export interface Suggestion<T> {
  key: string;
  label: string;
  sub?: string;
  data: T;
}

export function AutocompleteInput<T>({
  value,
  onChange,
  suggest,
  onPick,
  className,
  inputClassName,
  "aria-label": ariaLabel,
}: {
  value: string;
  onChange: (text: string) => void;
  suggest: (query: string) => Promise<Suggestion<T>[]>;
  onPick: (item: T) => void;
  className?: string;
  inputClassName?: string;
  "aria-label"?: string;
}) {
  const popover = useFloatingPanel(320, 300);
  const [items, setItems] = useState<Suggestion<T>[]>([]);
  const [active, setActive] = useState(0);
  const [typed, setTyped] = useState(false);
  const run = useRef(0);

  useEffect(() => {
    if (!typed) return;
    const id = ++run.current;
    const timer = setTimeout(() => {
      suggest(value)
        .then((found) => {
          if (id !== run.current) return;
          setItems(found);
          setActive(0);
          if (found.length) popover.show();
          else popover.hide();
        })
        .catch(() => popover.hide());
    }, 120);
    return () => clearTimeout(timer);
  }, [value, typed]);

  const pick = (item: Suggestion<T>) => {
    onPick(item.data);
    setTyped(false);
    popover.hide();
  };

  return (
    <div ref={popover.anchor} className={cx("relative", fieldWidth(className), className)}>
      <input
        className={cx(FIELD, "h-8 w-full", inputClassName)}
        value={value}
        aria-label={ariaLabel}
        autoComplete="off"
        onChange={(e) => {
          setTyped(true);
          onChange(e.target.value);
        }}
        onBlur={() => popover.hide()}
        onKeyDown={(e) => {
          if (!popover.open) return;
          if (e.key === "ArrowDown" || e.key === "ArrowUp") {
            e.preventDefault();
            setActive(Math.max(0, Math.min(items.length - 1, active + (e.key === "ArrowDown" ? 1 : -1))));
          } else if (e.key === "Enter" && items[active]) {
            e.preventDefault();
            pick(items[active]);
          }
        }}
      />
      {popover.panel(
        <ul role="listbox" className="min-h-0 flex-1 overflow-y-auto p-1">
          {items.map((it, i) => (
            <li key={it.key} role="option" aria-selected={i === active}>
              <button
                type="button"
                onClick={() => pick(it)}
                onMouseEnter={() => setActive(i)}
                className={cx("flex w-full flex-col rounded-md px-2.5 py-2 text-left", i === active && "bg-subtle")}
              >
                <span className="truncate text-[13px]">{it.label}</span>
                {it.sub && <span className="truncate text-xs text-muted">{it.sub}</span>}
              </button>
            </li>
          ))}
        </ul>,
      )}
    </div>
  );
}

export interface PickOption {
  value: string;
  label: string;
  description?: string | null;
  color?: string | null;
}

export function PickSelect({
  value,
  onChange,
  options,
  placeholder,
  disabled,
  className,
  "aria-label": ariaLabel,
}: {
  value: string;
  onChange: (value: string) => void;
  options: PickOption[];
  placeholder?: string;
  disabled?: boolean;
  className?: string;
  "aria-label"?: string;
}) {
  const popover = useFloatingPanel(Math.min(360, options.length * 44 + 56), 280);
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const list = useRef<HTMLUListElement>(null);
  const selected = options.find((o) => o.value === value);
  const q = query.trim().toLowerCase();
  const shown = q ? options.filter((o) => `${o.label} ${o.description ?? ""}`.toLowerCase().includes(q)) : options;
  const filter = options.length > 8;

  const open = () => {
    if (disabled) return;
    setQuery("");
    setActive(Math.max(0, options.findIndex((o) => o.value === value)));
    popover.show();
  };
  const pick = (v: string) => {
    if (v !== value) onChange(v);
    popover.hide();
  };
  const onKey = (e: ReactKeyboardEvent) => {
    if (!popover.open) {
      if (["ArrowDown", "ArrowUp", "Enter", " "].includes(e.key)) {
        e.preventDefault();
        open();
      }
      return;
    }
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const next = Math.max(0, Math.min(shown.length - 1, active + (e.key === "ArrowDown" ? 1 : -1)));
      setActive(next);
      list.current?.children[next]?.scrollIntoView({ block: "nearest" });
    } else if (e.key === "Enter" && shown[active]) {
      e.preventDefault();
      pick(shown[active].value);
    }
  };

  const own = (className ?? "").split(/\s+/).filter((c) => /^(h-|text-)/.test(c));
  const outer = (className ?? "").split(/\s+/).filter((c) => c && !own.includes(c)).join(" ");
  return (
    <div ref={popover.anchor} className={cx("relative", fieldWidth(outer), outer)}>
      <button
        type="button"
        disabled={disabled}
        aria-label={ariaLabel}
        aria-haspopup="listbox"
        aria-expanded={popover.open}
        onClick={() => (popover.open ? popover.hide() : open())}
        onKeyDown={onKey}
        className={cx(FIELD, "flex w-full items-center gap-2 pr-8 text-left", !own.some((c) => c.startsWith("h-")) && "h-8", popover.open && "border-muted", ...own)}
      >
        {selected?.color && <CategoryDot color={selected.color} />}
        <span className={cx("truncate", !selected && "text-muted")}>{selected?.label ?? placeholder ?? ""}</span>
      </button>
      <ChevronDown className="pointer-events-none absolute top-1/2 right-2.5 size-4 -translate-y-1/2 text-muted" />
      {popover.panel(
        <>
          {filter && (
            <div className="border-b border-line p-2">
              <input
                autoFocus
                value={query}
                onChange={(e) => {
                  setQuery(e.target.value);
                  setActive(0);
                }}
                onKeyDown={onKey}
                placeholder={t("Search…")}
                className={cx(FIELD, "h-8 w-full")}
              />
            </div>
          )}
          <ul ref={list} role="listbox" className="max-h-80 min-h-0 flex-1 overflow-y-auto p-1">
            {shown.length === 0 && <li className="px-3 py-2 text-[13px] text-muted">{t("No results")}</li>}
            {shown.map((o, i) => (
              <li key={o.value} role="option" aria-selected={o.value === value}>
                <button
                  type="button"
                  onClick={() => pick(o.value)}
                  onMouseEnter={() => setActive(i)}
                  className={cx("flex w-full items-center gap-2.5 rounded-md px-2.5 py-2 text-left", i === active && "bg-subtle")}
                >
                  {o.color !== undefined && <CategoryDot color={o.color} className="size-2.5" />}
                  <span className="min-w-0 flex-1">
                    <span className={cx("block truncate text-[13px]", o.value === value && "font-medium")}>{o.label}</span>
                    {o.description && <span className="block truncate text-xs text-muted">{o.description}</span>}
                  </span>
                  {o.value === value && <Check className="size-4 shrink-0 text-fg" />}
                </button>
              </li>
            ))}
          </ul>
        </>,
      )}
    </div>
  );
}

export function Textarea({ className, ...rest }: TextareaHTMLAttributes<HTMLTextAreaElement>) {
  return <textarea className={cx(FIELD, fieldWidth(className), "min-h-[72px] py-1.5 leading-relaxed", className)} {...rest} />;
}

function textOf(node: ReactNode): string {
  if (node === null || node === undefined || typeof node === "boolean") return "";
  if (typeof node === "string" || typeof node === "number") return String(node);
  if (Array.isArray(node)) return node.map(textOf).join("");
  if (isValidElement<{ children?: ReactNode }>(node)) return textOf(node.props.children);
  return "";
}

function optionsOf(children: ReactNode): PickOption[] {
  const out: PickOption[] = [];
  Children.forEach(children, (child) => {
    if (!isValidElement<{ value?: string | number; children?: ReactNode }>(child)) return;
    if (child.type === Fragment) return out.push(...optionsOf(child.props.children));
    if (child.type !== "option") return;
    const label = textOf(child.props.children);
    out.push({ value: String(child.props.value ?? label), label });
  });
  return out;
}

export function Select({ className, children, value, onChange, disabled, "aria-label": ariaLabel }: SelectHTMLAttributes<HTMLSelectElement>) {
  return (
    <PickSelect
      value={String(value ?? "")}
      options={optionsOf(children)}
      onChange={(v) => onChange?.({ target: { value: v }, currentTarget: { value: v } } as unknown as ChangeEvent<HTMLSelectElement>)}
      disabled={disabled}
      className={className}
      aria-label={ariaLabel}
    />
  );
}

const CURRENCIES = ["EUR", "USD", "GBP", "CHF"];

export function CurrencySelect({ children, ...rest }: SelectHTMLAttributes<HTMLSelectElement>) {
  return (
    <Select {...rest}>
      {children}
      {CURRENCIES.map((c) => (
        <option key={c}>{c}</option>
      ))}
    </Select>
  );
}

export function Field({ label, hint, children, className }: { label: string; hint?: ReactNode; children: ReactNode; className?: string }) {
  return (
    <label className={cx("flex flex-col gap-1", className)}>
      <span className="text-xs font-medium text-muted">{label}</span>
      {children}
      {hint && <span className="text-xs text-muted">{hint}</span>}
    </label>
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

export function CategoryDot({ color, className }: { color: string | null | undefined; className?: string }) {
  return <span className={cx("inline-block size-2 shrink-0 rounded-full", className)} style={{ background: color ?? "var(--line)" }} />;
}

export function Card({ children, className, title, actions }: { children: ReactNode; className?: string; title?: string; actions?: ReactNode }) {
  return (
    <section className={cx("rounded-lg border border-line bg-panel", className)}>
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
      className="flex min-w-0 flex-col gap-1 rounded-lg border border-line bg-panel p-4 text-left enabled:hover:bg-subtle disabled:cursor-default"
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
