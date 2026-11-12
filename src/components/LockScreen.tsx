import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { Fingerprint, Lock } from "lucide-react";
import { api } from "../lib/api";
import { t } from "../lib/i18n";
import { Button, cx } from "./ui";

export const PIN_LENGTH = 5;

export const PIN_CHANGED = "pin-changed";

export const pinDigits = (value: string) => value.replace(/\D/g, "").slice(0, PIN_LENGTH);

export function usePinCooldown() {
  const [cooling, setCooling] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout>>();
  const check = useCallback(async () => {
    try {
      const seconds = await api.pinCooldown();
      if (!seconds) return;
      clearTimeout(timer.current);
      setCooling(true);
      timer.current = setTimeout(() => setCooling(false), seconds * 1000);
    } catch {
    }
  }, []);
  useEffect(() => {
    check();
    return () => clearTimeout(timer.current);
  }, [check]);
  return [cooling, check] as const;
}

export function PinInput({
  label,
  value,
  onChange,
  autoFocus,
  invalid,
  disabled,
}: {
  label?: string;
  value: string;
  onChange: (value: string) => void;
  autoFocus?: boolean;
  invalid?: boolean;
  disabled?: boolean;
}) {
  const [focused, setFocused] = useState(false);
  const input = useRef<HTMLInputElement>(null);
  const active = Math.min(value.length, PIN_LENGTH - 1);
  useEffect(() => {
    if (autoFocus && !disabled) input.current?.focus();
  }, [autoFocus, disabled]);
  return (
    <label className={cx("flex flex-col items-center gap-2", disabled && "opacity-50")}>
      {label && <span className="text-xs font-medium text-muted">{label}</span>}
      <span className="relative flex gap-3">
        <input
          ref={input}
          aria-label={label ? undefined : "PIN"}
          aria-invalid={invalid || undefined}
          type="password"
          inputMode="numeric"
          autoComplete="off"
          value={value}
          disabled={disabled}
          onChange={(e) => onChange(pinDigits(e.target.value))}
          onFocus={() => setFocused(true)}
          onBlur={() => setFocused(false)}
          className="absolute inset-0 cursor-text caret-transparent opacity-0 disabled:cursor-not-allowed"
        />
        {Array.from({ length: PIN_LENGTH }, (_, i) => (
          <span
            key={i}
            aria-hidden
            className={cx(
              "field flex size-12 items-center justify-center rounded-lg border",
              invalid ? "border-neg bg-neg/10" : focused && i === active ? "border-fg ring-3 ring-fg/10" : "border-line",
            )}
          >
            {i < value.length && <span className="anim-pop size-2.5 rounded-full bg-fg" />}
          </span>
        ))}
      </span>
    </label>
  );
}

export function PinScreen({ title, children, className, ready = true }: { title: string; children: ReactNode; className?: string; ready?: boolean }) {
  return (
    <div
      className={cx("isolate flex h-full flex-col items-center justify-center", className ?? "relative")}
      onClick={(e) => e.currentTarget.querySelector<HTMLInputElement>("input:not(:disabled)")?.focus()}
    >
      <div className="sky" aria-hidden />
      <div className="fixed inset-x-0 top-0 h-12" data-tauri-drag-region />
      {ready && (
        <div className="anim-page floating flex min-w-80 flex-col items-center gap-5 rounded-xl border border-line bg-panel px-8 py-8">
          <Lock className="size-5 text-muted" />
          <h1 className="font-serif text-[22px] leading-tight">{title}</h1>
          {children}
        </div>
      )}
    </div>
  );
}

export function LockScreen({ onUnlock, ready }: { onUnlock: () => void; ready?: boolean }) {
  const [pin, setPin] = useState("");
  const [wrong, setWrong] = useState(false);
  const [busy, setBusy] = useState(false);
  const [cooling, checkCooldown] = usePinCooldown();
  const [touch, setTouch] = useState(false);
  const [touching, setTouching] = useState(false);
  const asked = useRef(false);

  useEffect(() => {
    api
      .touchIdStatus()
      .then((s) => setTouch(s.enabled))
      .catch(() => {});
  }, []);

  const touchId = useCallback(async () => {
    setTouching(true);
    const ok = await api.unlockTouchId().catch(() => false);
    if (ok) return onUnlock();
    setTouching(false);
  }, [onUnlock]);

  useEffect(() => {
    if (!touch || ready === false) return;
    const auto = () => {
      if (asked.current || document.visibilityState !== "visible" || !document.hasFocus()) return;
      asked.current = true;
      touchId();
    };
    auto();
    window.addEventListener("focus", auto);
    document.addEventListener("visibilitychange", auto);
    return () => {
      window.removeEventListener("focus", auto);
      document.removeEventListener("visibilitychange", auto);
    };
  }, [touch, ready, touchId]);

  const submit = async (value: string) => {
    setBusy(true);
    const ok = await api.unlock(value).catch(() => false);
    if (ok) return onUnlock();
    setWrong(true);
    setPin("");
    setBusy(false);
    checkCooldown();
  };

  return (
    <PinScreen title={t("Midnight is locked")} ready={ready}>
      <PinInput
        value={pin}
        autoFocus
        invalid={wrong || cooling}
        disabled={cooling}
        onChange={(value) => {
          if (busy) return;
          setPin(value);
          setWrong(false);
          if (value.length === PIN_LENGTH) submit(value);
        }}
      />
      {touch && (
        <Button variant="ghost" icon={<Fingerprint className="size-4" />} onClick={touchId} disabled={touching}>
          Touch ID
        </Button>
      )}
    </PinScreen>
  );
}
