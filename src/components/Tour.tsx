import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties } from "react";
import { createPortal } from "react-dom";
import { Button, usePresence } from "./ui";
import { useApp, type Route } from "../lib/nav";
import { t } from "../lib/i18n";
import { markTourDone } from "../lib/tour";
import { keys } from "../lib/platform";

const CARD_WIDTH = 368;
const GAP = 14;
const EDGE = 12;

interface Step {
  target?: string;
  route?: Route;
  title: string;
  text: string;
}

const steps = (): Step[] => [
  { route: "overview", title: t("Welcome to Midnight"), text: t("One minute through every part of Midnight. If you'd rather look around on your own, you can skip the tour.") },
  { target: "search", title: t("Search and actions"), text: t("Searches invoices, expenses, customers, projects and the vault. You can also create a new invoice, expense or customer from here. Shortcut: {key}.", { key: keys("⌘K", "Ctrl+K") }) },
  { target: "overview", route: "overview", title: t("Overview"), text: t("Income, expenses and profit for the period you pick, plus the cash flow of recent months and your latest bookings and tracked time.") },
  { target: "invoices", route: "invoices", title: t("Invoices"), text: t("Write invoices here and create the PDF with LaTeX. A draft can still be changed, and once the money is in, you mark the invoice as paid.") },
  { target: "ledger", route: "ledger", title: t("Income & expenses"), text: t("Paid invoices land here on their own. Add expenses yourself or drop a receipt onto the page. Set up rent and subscriptions once as recurring and Midnight books them automatically.") },
  { target: "customers", route: "customers", title: t("Customers"), text: t("Addresses and contacts for your invoices. For each customer you also see revenue, open amounts and the time tracked so far.") },
  { target: "tracker", route: "tracker", title: t("Time tracking"), text: t("Start the timer or add time by hand, always for a project. Hours you haven't billed yet can later be turned into an invoice.") },
  { target: "vault", route: "vault", title: "Vault", text: t("Contracts, receipts and other documents. Search also finds text that is inside the files themselves.") },
  { target: "reports", route: "reports", title: t("Reports"), text: t("Breakdowns by customer, category and project, the numbers for your tax return and a forecast of how much tax will be due at the end of the year.") },
  { target: "notifications", title: t("Notifications"), text: t("Midnight lets you know here when an invoice is overdue, a recurring expense was booked or a new version of the app is ready.") },
  { target: "settings", route: "settings", title: t("Settings"), text: t("Keep your company details, invoice numbers and LaTeX templates up to date. You also set up the PIN lock here and decide what AI assistants may do.") },
  { target: "tour", route: "overview", title: t("That's it"), text: t("The question mark starts this tour again. In the command palette it's called \"Start tutorial\".") },
];

function useTargetRect(target: string | undefined, step: number): DOMRect | null {
  const [rect, setRect] = useState<DOMRect | null>(null);
  useLayoutEffect(() => {
    const measure = () => setRect(target ? (document.querySelector(`[data-tour="${target}"]`)?.getBoundingClientRect() ?? null) : null);
    measure();
    window.addEventListener("resize", measure);
    return () => window.removeEventListener("resize", measure);
  }, [target, step]);
  return rect;
}

function cardPosition(rect: DOMRect | null): CSSProperties {
  if (!rect) return { left: "50%", top: "50%", transform: "translate(-50%, -50%)" };
  const vw = window.innerWidth;
  const vh = window.innerHeight;
  if (rect.top > 60 && rect.right + GAP + CARD_WIDTH + EDGE <= vw) {
    const left = rect.right + GAP;
    return rect.top + 240 > vh ? { left, bottom: Math.max(EDGE, vh - rect.bottom - 8) } : { left, top: Math.max(EDGE, rect.top - 8) };
  }
  const left = Math.min(Math.max(rect.left + rect.width / 2 - CARD_WIDTH / 2, EDGE), vw - CARD_WIDTH - EDGE);
  return { left, top: rect.bottom + GAP };
}

export function Tour({ open, onClose }: { open: boolean; onClose: () => void }) {
  const { route, go } = useApp();
  const [step, setStep] = useState(0);
  const { visible, state } = usePresence(open);
  const list = steps();
  const current = list[Math.min(step, list.length - 1)];
  const last = step === list.length - 1;
  const rect = useTargetRect(current.target, step);
  const primary = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!visible) setStep(0);
  }, [visible]);

  useEffect(() => {
    if (open && current.route && current.route !== route) go(current.route);
  }, [open, current.route, route, go]);

  useEffect(() => {
    if (open) primary.current?.focus();
  }, [open, step]);

  const finish = () => {
    markTourDone();
    onClose();
  };
  const next = () => (last ? finish() : setStep((s) => s + 1));
  const back = () => setStep((s) => Math.max(0, s - 1));
  const handlers = useRef({ finish, next, back });
  handlers.current = { finish, next, back };

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") handlers.current.finish();
      if (e.key === "ArrowRight") handlers.current.next();
      if (e.key === "ArrowLeft") handlers.current.back();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open]);

  if (!visible) return null;

  return createPortal(
    <div className="fixed inset-0 z-[65]" data-state={state}>
      {rect ? (
        <div className="anim-overlay absolute inset-0">
          <div
            className="pointer-events-none absolute rounded-lg"
            style={{
              left: rect.left - 4,
              top: rect.top - 4,
              width: rect.width + 8,
              height: rect.height + 8,
              boxShadow: "0 0 0 2px color-mix(in srgb, var(--info) 70%, transparent), 0 0 0 9999px var(--overlay)",
            }}
          />
        </div>
      ) : (
        <div className="anim-overlay absolute inset-0 bg-overlay backdrop-blur-sm" />
      )}
      <div
        key={step}
        role="dialog"
        aria-modal="true"
        aria-labelledby="tour-title"
        className="floating anim-dialog absolute rounded-xl border border-line bg-panel p-4"
        style={{ width: CARD_WIDTH, ...cardPosition(rect) }}
      >
        <h2 id="tour-title" className="text-[15px] font-semibold">
          {current.title}
        </h2>
        <p className="mt-1.5 text-[13px] leading-relaxed text-pretty text-muted">
          {current.text}
        </p>
        <div className="mt-4 flex items-center gap-2">
          <span className="tabular mr-auto text-xs whitespace-nowrap text-muted">{t("{n} of {total}", { n: step + 1, total: list.length })}</span>
          {!last && (
            <Button size="sm" variant="ghost" onClick={finish}>
              {t("Skip")}
            </Button>
          )}
          {step > 0 && (
            <Button size="sm" onClick={back}>
              {t("Back")}
            </Button>
          )}
          <Button ref={primary} size="sm" variant="primary" onClick={next}>
            {step === 0 ? t("Start tour") : last ? t("Done") : t("Continue")}
          </Button>
        </div>
      </div>
    </div>,
    document.body,
  );
}
