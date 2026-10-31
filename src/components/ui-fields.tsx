import {
  Children,
  Fragment,
  isValidElement,
  useEffect,
  useRef,
  useState,
  type ChangeEvent,
  type KeyboardEvent as ReactKeyboardEvent,
  type ReactNode,
  type SelectHTMLAttributes,
  type TextareaHTMLAttributes,
} from "react";
import { createPortal } from "react-dom";
import { CalendarDays, Check, ChevronDown, ChevronLeft, ChevronRight, Clock } from "lucide-react";
import { date as formatDate } from "../lib/format";
import { lang, locale, t } from "../lib/i18n";

import { CategoryDot, cx, fieldWidth, FIELD } from "./ui-base";

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
        className={cx("floating anim-pop fixed z-[100] flex flex-col overflow-auto rounded-lg border border-line bg-panel", pos.bottom !== undefined && "anim-pop-up origin-bottom", className)}
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
