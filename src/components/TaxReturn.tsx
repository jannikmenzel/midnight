import { useEffect, useRef, useState, type ReactNode } from "react";
import { AlertTriangle, ChevronDown, Plus, X } from "lucide-react";
import { api, errorMessage, type TaxInput, type TaxJob, type TaxReport, type TaxVolunteer } from "../lib/api";
import { centsToInput, money, parseMoney } from "../lib/format";
import { useApp } from "../lib/nav";
import { Button, Card, Field, IconButton, Input, Kpi, Select, Skeleton, cx, useToast } from "./ui";
import { t } from "../lib/i18n";

const emptyJob = (): TaxJob => ({ employer: "", gross_cents: 0, wage_tax_cents: 0, soli_cents: 0, church_tax_cents: 0, pension_cents: 0, commute_days: 0, commute_km: 0, equipment_cents: 0 });
const emptyVolunteer = (): TaxVolunteer => ({ organisation: "", kind: "trainer", income_cents: 0, costs_cents: 0 });

function MoneyInput({ cents, onChange, label }: { cents: number; onChange: (c: number) => void; label?: string }) {
  const [text, setText] = useState(cents ? centsToInput(cents) : "");
  useEffect(() => setText(cents ? centsToInput(cents) : ""), [cents]);
  return (
    <Input
      value={text}
      aria-label={label}
      placeholder={centsToInput(0)}
      className="tabular-nums text-right"
      onChange={(e) => setText(e.target.value)}
      onBlur={() => onChange(parseMoney(text) ?? 0)}
    />
  );
}

function Item({ title, hint, value, open, onToggle, children }: { title: string; hint: string; value: string; open: boolean; onToggle: () => void; children: ReactNode }) {
  return (
    <li className="border-b border-line last:border-0">
      <button type="button" onClick={onToggle} aria-expanded={open} className="flex w-full items-center gap-4 px-4 py-3 text-left hover:bg-subtle">
        <span className="min-w-0 flex-1">
          <span className="block text-[13px] font-medium">{title}</span>
          <span className="block truncate text-xs text-muted">{hint}</span>
        </span>
        <span className="tabular text-[13px]">{value}</span>
        <ChevronDown className={cx("size-4 shrink-0 text-muted transition-transform", open && "rotate-180")} />
      </button>
      {open && <div className="flex flex-col gap-4 px-4 pt-1 pb-4">{children}</div>}
    </li>
  );
}

function statusLabel(s: TaxInput["health_status"]): string {
  return { family: t("family insurance"), student: t("student"), voluntary: t("voluntary"), private: t("private") }[s] ?? "";
}

export function TaxReturn({ year }: { year: number }) {
  const { go } = useApp();
  const toast = useToast();
  const [report, setReport] = useState<TaxReport | null>(null);
  const [input, setInput] = useState<TaxInput | null>(null);
  const [saving, setSaving] = useState<"idle" | "saving" | "saved">("idle");
  const [open, setOpen] = useState<string | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout>>();
  const yearRef = useRef(year);
  yearRef.current = year;

  const load = () =>
    api
      .getTaxReport(year)
      .then((r) => {
        if (yearRef.current !== year) return;
        setReport(r);
        setInput(r.input);
      })
      .catch((e) => toast.error(errorMessage(e)));

  useEffect(() => {
    setReport(null);
    setInput(null);
    setSaving("idle");
    load();
  }, [year]);

  const savingRef = useRef(saving);
  savingRef.current = saving;
  useEffect(() => {
    const onFocus = () => savingRef.current !== "saving" && load();
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [year]);

  const update = (patch: Partial<TaxInput>) => {
    if (!input) return;
    const next = { ...input, ...patch };
    setInput(next);
    setSaving("saving");
    clearTimeout(timer.current);
    timer.current = setTimeout(() => {
      api
        .saveTaxInput(year, next)
        .then((r) => {
          if (yearRef.current !== year) return;
          setReport(r);
          setSaving("saved");
        })
        .catch((e) => {
          if (yearRef.current === year) setSaving("idle");
          toast.error(errorMessage(e));
        });
    }, 500);
  };

  if (!report || !input) return <Skeleton rows={6} />;
  const r = report;
  const p = r.params;
  const toggle = (key: string) => setOpen(open === key ? null : key);
  const setJob = (i: number, patch: Partial<TaxJob>) => update({ jobs: input.jobs.map((j, idx) => (idx === i ? { ...j, ...patch } : j)) });
  const setVol = (i: number, patch: Partial<TaxVolunteer>) => update({ volunteering: input.volunteering.map((v, idx) => (idx === i ? { ...v, ...patch } : v)) });
  const deductions = r.provision_cents + r.special_cents + r.medical_deductible_cents;
  const filledIds = [input.tax_id, input.tax_number, input.tax_office, input.refund_iban].filter((v) => v.trim()).length;
  const warnings = r.warnings.filter((w) => w.level === "warn");

  return (
    <div className="flex flex-col gap-4">
      <Card>
        <div className="flex flex-col gap-4 p-5">
          <div className="flex items-start justify-between gap-4">
            <div>
              <div className="text-xs text-muted">
                {r.balance_cents > 0 ? t("Current additional payment") : r.balance_cents < 0 ? t("Current refund") : t("Currently nothing to pay")}
              </div>
              <div className={cx("tabular text-3xl font-semibold", r.balance_cents > 0 ? "text-neg" : r.balance_cents < 0 ? "text-pos" : "")}>{money(Math.abs(r.balance_cents))}</div>
            </div>
            {saving !== "idle" && <span className="text-[11px] text-muted">{saving === "saving" ? t("Saving…") : t("Saved")}</span>}
          </div>
          {warnings.map((w, i) => (
            <p key={i} className="flex items-center gap-2 rounded-md bg-warn/10 px-3 py-2 text-xs leading-relaxed">
              <AlertTriangle className="size-3.5 shrink-0 text-warn" />
              {w.text}
            </p>
          ))}
        </div>
      </Card>

      <div className="grid grid-cols-2 gap-4 lg:grid-cols-4">
        <Kpi label={t("Total income")} value={money(r.total_income_cents)} />
        <Kpi label={t("Deductions")} value={`− ${money(deductions)}`} />
        <Kpi label={t("Taxable")} value={money(r.taxable_income_cents)} />
        <Kpi label={t("Filing deadline")} value={r.deadline} />
      </div>

      <Card title={t("Your details")}>
        <ul>
          <Item title={t("Self-employment")} hint={t("From invoices and expenses, minus home office")} value={money(r.profit_cents)} open={open === "business"} onToggle={() => toggle("business")}>
            <dl className="flex max-w-md flex-col text-[13px]">
              <div className="flex h-9 items-center justify-between">
                <dt>
                  <button type="button" onClick={() => go("invoices")} className="hover:underline">
                    {t("Income")}
                  </button>
                </dt>
                <dd className="tabular">{money(r.income_cents)}</dd>
              </div>
              <div className="flex h-9 items-center justify-between">
                <dt>
                  <button type="button" onClick={() => go("ledger", { kind: "expense", from: `${year}-01-01`, to: `${year}-12-31` })} className="hover:underline">
                    {t("Expenses")}
                  </button>
                </dt>
                <dd className="tabular">− {money(r.expenses_cents)}</dd>
              </div>
              <div className="flex items-start justify-between gap-3 py-0.5">
                <dt className="flex flex-col gap-1">
                  <span className="flex items-center gap-2">
                    {t("Home office")}
                    <span className="text-muted tabular">
                      {r.home_office_business_days} {t("days ×")} {money(p.home_office_per_day)}
                    </span>
                  </span>
                  <span className="text-xs text-muted">
                    {t("{n} working days in time tracking, at most {max} count", { n: r.home_office_days, max: p.home_office_max_days })}
                  </span>
                </dt>
                <dd className="flex h-8 items-center tabular">− {money(r.home_office_business_cents)}</dd>
              </div>
              <div className="mt-2 flex h-9 items-center justify-between border-t border-line font-medium">
                <dt>{t("Profit")}</dt>
                <dd className="tabular">{money(r.profit_cents)}</dd>
              </div>
            </dl>
          </Item>

          <Item
            title={t("Employment")}
            hint={input.jobs.length ? input.jobs.map((j) => j.employer || t("Employer")).join(", ") : t("Figures from your annual payroll tax certificate")}
            value={money(r.employment_gross_cents)}
            open={open === "jobs"}
            onToggle={() => toggle("jobs")}
          >
            {input.jobs.map((j, i) => (
              <div key={i} className="grid grid-cols-[minmax(0,1fr)_120px_120px_120px_28px] items-end gap-2">
                <Field label={t("Employer")}>
                  <Input value={j.employer} onChange={(e) => setJob(i, { employer: e.target.value })} />
                </Field>
                <Field label={t("Gross")}>
                  <MoneyInput cents={j.gross_cents} onChange={(c) => setJob(i, { gross_cents: c })} />
                </Field>
                <Field label={t("Wage tax")}>
                  <MoneyInput cents={j.wage_tax_cents} onChange={(c) => setJob(i, { wage_tax_cents: c })} />
                </Field>
                <Field label={t("Pension (employee share)")}>
                  <MoneyInput cents={j.pension_cents} onChange={(c) => setJob(i, { pension_cents: c })} />
                </Field>
                <IconButton label={t("Remove")} onClick={() => update({ jobs: input.jobs.filter((_, idx) => idx !== i) })}>
                  <X className="size-3.5" />
                </IconButton>
              </div>
            ))}
            <Button size="sm" variant="ghost" icon={<Plus className="size-3" />} className="self-start" onClick={() => update({ jobs: [...input.jobs, emptyJob()] })}>
              {t("Employer")}
            </Button>
          </Item>

          <Item
            title={t("Volunteering")}
            hint={t("Coaching tax-free up to {trainer}, otherwise up to {volunteer}", { trainer: money(p.trainer_allowance), volunteer: money(p.volunteer_allowance) })}
            value={money(r.volunteer_income_cents)}
            open={open === "volunteer"}
            onToggle={() => toggle("volunteer")}
          >
            {input.volunteering.map((v, i) => (
              <div key={i} className="grid grid-cols-[minmax(0,1fr)_160px_120px_28px] items-end gap-2">
                <Field label={t("Organisation")}>
                  <Input value={v.organisation} onChange={(e) => setVol(i, { organisation: e.target.value })} />
                </Field>
                <Field label={t("Type")}>
                  <Select value={v.kind} onChange={(e) => setVol(i, { kind: e.target.value as TaxVolunteer["kind"] })}>
                    <option value="trainer">{t("Coach / instructor")}</option>
                    <option value="volunteer">{t("Volunteering")}</option>
                  </Select>
                </Field>
                <Field label={t("Per year")}>
                  <MoneyInput cents={v.income_cents} onChange={(c) => setVol(i, { income_cents: c })} />
                </Field>
                <IconButton label={t("Remove")} onClick={() => update({ volunteering: input.volunteering.filter((_, idx) => idx !== i) })}>
                  <X className="size-3.5" />
                </IconButton>
              </div>
            ))}
            {r.volunteer_taxable_cents > 0 && <p className="text-xs text-muted">{t("Taxable portion:")} {money(r.volunteer_taxable_cents)}</p>}
            <Button size="sm" variant="ghost" icon={<Plus className="size-3" />} className="self-start" onClick={() => update({ volunteering: [...input.volunteering, emptyVolunteer()] })}>
              {t("Volunteering")}
            </Button>
          </Item>

          <Item
            title={t("Health insurance")}
            hint={[input.health_insurer, statusLabel(input.health_status)].filter(Boolean).join(" · ")}
            value={money(r.basic_insurance_cents)}
            open={open === "health"}
            onToggle={() => toggle("health")}
          >
            <div className="grid grid-cols-3 gap-4">
              <Field label={t("Insurer")}>
                <Input value={input.health_insurer} onChange={(e) => update({ health_insurer: e.target.value })} />
              </Field>
              <Field label={t("Status")}>
                <Select value={input.health_status} onChange={(e) => update({ health_status: e.target.value as TaxInput["health_status"] })}>
                  <option value="family">{t("Family insurance")}</option>
                  <option value="student">{t("Student")}</option>
                  <option value="voluntary">{t("Voluntary")}</option>
                  <option value="private">{t("Private")}</option>
                </Select>
              </Field>
              <Field label={t("Annual contributions (health + long-term care)")}>
                <MoneyInput cents={input.health_cents + input.care_cents} onChange={(c) => update({ health_cents: c, care_cents: 0 })} />
              </Field>
            </div>
          </Item>

          <Item title={t("Other deductions")} hint={t("First degree and donations")} value={money(input.study_cents + input.donations_cents)} open={open === "special"} onToggle={() => toggle("special")}>
            <div className="grid grid-cols-2 gap-4">
              <Field label={t("First degree")} hint={t("Semester fees, books, materials")}>
                <MoneyInput cents={input.study_cents} onChange={(c) => update({ study_cents: c })} />
              </Field>
              <Field label={t("Donations")}>
                <MoneyInput cents={input.donations_cents} onChange={(c) => update({ donations_cents: c })} />
              </Field>
            </div>
          </Item>

          <Item
            title={t("Church tax")}
            hint={input.church_tax_rate > 0 ? t("{rate} % of income tax", { rate: input.church_tax_rate }) : t("Not a church member")}
            value={money(r.church_tax_cents)}
            open={open === "church"}
            onToggle={() => toggle("church")}
          >
            <Field label={t("Church membership")} className="max-w-sm">
              <Select value={String(input.church_tax_rate)} onChange={(e) => update({ church_tax_rate: Number(e.target.value) })}>
                <option value="0">{t("No church member")}</option>
                <option value="8">{t("Member, 8 % (Bavaria, Baden-Württemberg)")}</option>
                <option value="9">{t("Member, 9 % (all other states)")}</option>
              </Select>
            </Field>
          </Item>

          <Item
            title={t("Prepayments")}
            hint={t("Income tax already paid to the tax office for this year")}
            value={money(input.prepayments_cents)}
            open={open === "prepayments"}
            onToggle={() => toggle("prepayments")}
          >
            <Field label={t("Prepayments made")} hint={t("Total of all quarterly payments, as shown on your bank statements")} className="max-w-sm">
              <MoneyInput cents={input.prepayments_cents} onChange={(c) => update({ prepayments_cents: c })} />
            </Field>
          </Item>

          <Item title={t("Personal details")} hint={t("Tax ID, tax number, tax office, IBAN")} value={t("{n} of {total}", { n: filledIds, total: 4 })} open={open === "personal"} onToggle={() => toggle("personal")}>
            <div className="grid grid-cols-2 gap-4">
              {(
                [
                  ["tax_id", t("Tax ID")],
                  ["tax_number", t("Tax number")],
                  ["tax_office", t("Tax office")],
                  ["refund_iban", t("IBAN for refunds")],
                ] as [keyof TaxInput, string][]
              ).map(([key, label]) => (
                <Field key={key} label={label}>
                  <Input value={String(input[key] ?? "")} onChange={(e) => update({ [key]: e.target.value } as Partial<TaxInput>)} />
                </Field>
              ))}
            </div>
          </Item>
        </ul>
      </Card>
    </div>
  );
}
