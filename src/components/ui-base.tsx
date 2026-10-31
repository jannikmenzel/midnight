import { forwardRef, type ButtonHTMLAttributes, type InputHTMLAttributes, type ReactNode } from "react";
import { Loader2 } from "lucide-react";

export function cx(...classes: (string | false | null | undefined)[]): string {
  return classes.filter(Boolean).join(" ");
}

type ButtonVariant = "primary" | "secondary" | "ghost" | "danger";

const BUTTON_VARIANTS: Record<ButtonVariant, string> = {
  primary: "primary bg-accent text-accent-fg hover:opacity-90",
  secondary: "raised border border-line bg-panel text-fg hover:bg-subtle hover:bg-none",
  ghost: "text-fg hover:bg-hover",
  danger: "raised border border-line bg-panel text-neg hover:bg-subtle hover:bg-none",
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
        "inline-flex shrink-0 items-center justify-center gap-1.5 rounded-md font-medium whitespace-nowrap disabled:cursor-not-allowed disabled:opacity-50",
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

export function fieldWidth(className?: string) {
  return /(^|\s)w-/.test(className ?? "") ? "" : "w-full";
}

export const FIELD = "field rounded-md border border-line px-2.5 text-[13px] text-fg outline-none placeholder:text-muted focus:border-muted disabled:opacity-60";

export const Input = forwardRef<HTMLInputElement, InputHTMLAttributes<HTMLInputElement>>(function Input({ className, ...rest }, ref) {
  return <input ref={ref} className={cx(FIELD, fieldWidth(className), "h-8", className)} {...rest} />;
});

export function CategoryDot({ color, className }: { color: string | null | undefined; className?: string }) {
  return <span className={cx("inline-block size-2 shrink-0 rounded-full", className)} style={{ background: color ?? "var(--line)" }} />;
}
