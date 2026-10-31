import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { AnimatedMoney, cx, prefersReducedMotion } from "./ui";

export type MomentKind = "export" | "timesheet" | "sent" | "paid" | "update";

export interface Moment {
  kind: MomentKind;
  title: string;
  detail?: string;
  amount?: { cents: number; currency: string };
}

interface ActiveMoment extends Moment {
  id: number;
  working: boolean;
}

const HOLD_MS = 2600;
const AFTER_WORK_HOLD_MS = 2000;
const UPDATE_HOLD_MS = 3600;
const REDUCED_HOLD_MS = 1200;
const MIN_WORK_MS = 1400;
const EXIT_MS = 280;

const MomentContext = createContext<{
  show: (moment: Moment) => Promise<void>;
  run: <T>(start: Moment, task: () => Promise<T>, done: (result: T) => Moment) => Promise<T>;
}>({
  show: async () => {},
  run: (_start, task) => task(),
});

function Stroke({ d, at, ms }: { d: string; at: number; ms: number }) {
  return <path d={d} pathLength={1} className="moment-draw" style={{ "--at": `${at}ms`, "--ms": `${ms}ms` } as CSSProperties} />;
}

function MomentIcon({ kind, working }: { kind: MomentKind; working: boolean }) {
  return (
    <svg viewBox="0 0 64 64" className={cx("size-[88px]", kind === "paid" && "text-pos")} fill="none" stroke="currentColor" strokeWidth={1.25} strokeLinecap="round" strokeLinejoin="round" aria-hidden>
      {kind === "export" && (
        <>
          <Stroke d="M10 12h44a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H10a2 2 0 0 1-2-2v-8a2 2 0 0 1 2-2z" at={100} ms={700} />
          <Stroke d="M12 24v26a2 2 0 0 0 2 2h36a2 2 0 0 0 2-2V24" at={500} ms={700} />
          {working ? <Stroke d="M26 34h12" at={1000} ms={300} /> : <Stroke d="M24 37l6 6 11-12" at={100} ms={500} />}
        </>
      )}
      {kind === "timesheet" && (
        <>
          <Stroke d="M32 6a26 26 0 1 1 0 52a26 26 0 1 1 0-52" at={100} ms={900} />
          {working ? <Stroke d="M32 17v15l10 6" at={850} ms={400} /> : <Stroke d="M21 33l8 8 14-16" at={100} ms={500} />}
        </>
      )}
      {kind === "sent" && (
        <>
          <Stroke d="M16 6h24l12 12v38a2 2 0 0 1-2 2H16a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2z" at={100} ms={800} />
          <Stroke d="M40 6v12h12" at={700} ms={300} />
          <Stroke d="M22 24h12" at={850} ms={250} />
          <Stroke d="M22 31h20" at={950} ms={250} />
          <Stroke d="M22 44h20" at={1150} ms={350} />
          <Stroke d="M36 38l6 6-6 6" at={1400} ms={250} />
        </>
      )}
      {kind === "paid" && (
        <>
          <Stroke d="M32 6a26 26 0 1 1 0 52a26 26 0 1 1 0-52" at={100} ms={900} />
          <Stroke d="M21 33l8 8 14-16" at={850} ms={400} />
        </>
      )}
      {kind === "update" && (
        <>
          <Stroke d="M32 8a16 16 0 0 0 24 24a24 24 0 1 1-24-24z" at={100} ms={1300} />
          {!working && (
            <>
              <Stroke d="M47 8v8M43 12h8" at={1200} ms={350} />
              <Stroke d="M55 19v4M53 21h4" at={1450} ms={300} />
            </>
          )}
        </>
      )}
    </svg>
  );
}

function MomentOverlay({ moment, leaving, onDismiss }: { moment: ActiveMoment; leaving: boolean; onDismiss: () => void }) {
  return createPortal(
    <div className="moment fixed inset-0 z-[70] flex flex-col items-center justify-center gap-7 bg-bg" data-leaving={leaving || undefined} role="status" onClick={onDismiss}>
      <div className="sky" aria-hidden />
      <MomentIcon key={moment.id} kind={moment.kind} working={moment.working} />
      <div key={moment.working ? "working" : "done"} className="moment-text flex min-h-[112px] flex-col items-center gap-2 px-6 text-center">
        <h2 className="font-serif text-[30px] leading-tight font-normal tracking-tight">{moment.title}</h2>
        {moment.amount && <AnimatedMoney cents={moment.amount.cents} currency={moment.amount.currency} duration={1100} className={cx("tabular text-2xl font-semibold tracking-tight", moment.kind === "paid" && "text-pos")} />}
        {moment.detail && <p className="text-[13px] text-muted">{moment.detail}</p>}
      </div>
      {moment.working && <div className="moment-breathe absolute bottom-14 h-px w-[72px] bg-fg" />}
    </div>,
    document.body,
  );
}

function useMomentLifecycle(active: ActiveMoment | null, leaving: boolean, onLeave: () => void, onGone: () => void) {
  const id = active?.id;
  const working = active?.working;
  const kind = active?.kind;

  useEffect(() => {
    if (id === undefined || working) return;
    const hold = prefersReducedMotion() ? REDUCED_HOLD_MS : kind === "update" ? UPDATE_HOLD_MS : kind === "export" || kind === "timesheet" ? AFTER_WORK_HOLD_MS : HOLD_MS;
    const timer = setTimeout(onLeave, hold);
    return () => clearTimeout(timer);
  }, [id, working, kind, onLeave]);

  useEffect(() => {
    if (!leaving) return;
    const timer = setTimeout(onGone, EXIT_MS);
    return () => clearTimeout(timer);
  }, [leaving, onGone]);

  useEffect(() => {
    if (id === undefined) return;
    const onKey = (e: KeyboardEvent) => {
      e.stopImmediatePropagation();
      if (!e.metaKey && !e.ctrlKey) e.preventDefault();
      if (e.key === "Escape" && !working) onLeave();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [id, working, onLeave]);
}

export function MomentProvider({ children }: { children: ReactNode }) {
  const [active, setActive] = useState<ActiveMoment | null>(null);
  const [leaving, setLeaving] = useState(false);
  const nextId = useRef(0);
  const finished = useRef<(() => void) | null>(null);

  const closed = () => new Promise<void>((resolve) => (finished.current?.(), (finished.current = resolve)));

  const show = useCallback((moment: Moment) => {
    setLeaving(false);
    setActive({ ...moment, id: ++nextId.current, working: false });
    return closed();
  }, []);

  const run = useCallback(async <T,>(start: Moment, task: () => Promise<T>, done: (result: T) => Moment) => {
    const id = ++nextId.current;
    setLeaving(false);
    setActive({ ...start, id, working: true });
    try {
      const [result] = await Promise.all([task(), new Promise((resolve) => setTimeout(resolve, MIN_WORK_MS))]);
      setActive({ ...done(result), id, working: false });
      await closed();
      return result;
    } catch (e) {
      setActive(null);
      throw e;
    }
  }, []);

  const leave = useCallback(() => setLeaving(true), []);
  const gone = useCallback(() => {
    setActive(null);
    setLeaving(false);
    finished.current?.();
    finished.current = null;
  }, []);

  useMomentLifecycle(active, leaving, leave, gone);

  const value = useMemo(() => ({ show, run }), [show, run]);

  return (
    <MomentContext.Provider value={value}>
      {children}
      {active && <MomentOverlay moment={active} leaving={leaving} onDismiss={() => !active.working && leave()} />}
    </MomentContext.Provider>
  );
}

export function useMoment() {
  return useContext(MomentContext);
}
