import { useEffect, useState } from "react";
import { Check, Copy, FolderOpen, RotateCcw, Trash2 } from "lucide-react";
import { api, errorMessage, type Limits, type Settings, type TaxOverrides, type TaxParamKey } from "../lib/api";
import { centsToInput, money, parseMoney } from "../lib/format";
import { useAsync } from "../lib/hooks";
import { useApp, type Theme } from "../lib/nav";
import { Badge, Button, Card, CardBody, CurrencySelect, Dialog, ErrorNote, Field, Input, PageHeader, Section, Select, Tabs, Textarea, Toggle, useToast } from "../components/ui";
import { isLang, lang, LANGUAGES, setLang, t } from "../lib/i18n";
import LatexTemplates from "../components/LatexTemplates";
import { CountrySelect, PlaceInput } from "../components/fields";

type Tab = "general" | "company" | "invoices" | "latex" | "limits" | "data";

const IS_MAC = navigator.userAgent.includes("Mac");

function useSettingsForm(afterSave?: () => Promise<void>) {
  const { settings, reloadSettings } = useApp();
  const toast = useToast();
  const [form, setForm] = useState<Settings>(settings);
  const [busy, setBusy] = useState(false);
  useEffect(() => setForm(settings), [settings]);
  const set = <K extends keyof Settings>(key: K, value: Settings[K]) => setForm((f) => ({ ...f, [key]: value }));
  const setMany = (patch: Partial<Settings>) => setForm((f) => ({ ...f, ...patch }));
  const text = (key: keyof Settings) => ({
    value: String(form[key] ?? ""),
    onChange: (e: React.ChangeEvent<HTMLInputElement | HTMLTextAreaElement>) => set(key, e.target.value as never),
  });
  const save = async () => {
    setBusy(true);
    try {
      await api.saveSettings(form);
      await afterSave?.();
      if (isLang(form.language) && form.language !== lang()) return setLang(form.language);
      await reloadSettings();
      toast.success(t("Settings saved"));
    } catch (e) {
      toast.error(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };
  const dirty = JSON.stringify(form) !== JSON.stringify(settings);
  return { form, set, setMany, text, save, busy, dirty, settings };
}

function SaveBar({ onSave, busy, dirty }: { onSave: () => void; busy: boolean; dirty: boolean }) {
  return (
    <div className="flex justify-end border-t border-line px-4 py-3">
      <Button variant="primary" onClick={onSave} loading={busy} disabled={!dirty}>
        {t("Save")}
      </Button>
    </div>
  );
}

function GeneralTab() {
  const { theme, setTheme } = useApp();
  const { form, set, save, busy, dirty } = useSettingsForm(() => api.refreshMenuBar());
  return (
    <div className="flex flex-col gap-4">
      <Card title={t("General")}>
        <div className="flex flex-col gap-6 p-4">
          <Section title={t("Appearance")}>
            <div className="grid gap-4 md:grid-cols-2">
              <Field label={t("Language")}>
                <Select value={form.language} onChange={(e) => set("language", e.target.value)}>
                  {LANGUAGES.map((l) => (
                    <option key={l.value} value={l.value}>
                      {l.label}
                    </option>
                  ))}
                </Select>
              </Field>
              <Field label={t("Theme")}>
                <Select value={theme} onChange={(e) => setTheme(e.target.value as Theme)}>
                  <option value="system">{t("Match system")}</option>
                  <option value="light">{t("Light")}</option>
                  <option value="dark">{t("Dark")}</option>
                </Select>
              </Field>
            </div>
          </Section>
          {IS_MAC && (
            <Section title={t("Menu bar")}>
              <Toggle
                checked={form.menu_bar_icon}
                onChange={(v) => set("menu_bar_icon", v)}
                label={t("Show icon in the menu bar")}
                hint={t("Start and stop time tracking for a project right from the menu bar")}
              />
              <Toggle
                checked={form.menu_bar_show_time}
                onChange={(v) => set("menu_bar_show_time", v)}
                label={t("Show running time next to the icon")}
                hint={t("Shows how long the current timer has been running")}
              />
              <Toggle
                checked={form.menu_bar_keep_running}
                onChange={(v) => set("menu_bar_keep_running", v)}
                label={t("Keep running when the window is closed")}
                hint={t("Closing the window only hides it. Quit with ⌘Q or from the menu bar.")}
              />
            </Section>
          )}
        </div>
        <SaveBar onSave={save} busy={busy} dirty={dirty} />
      </Card>
      <AutostartCard menuBar={IS_MAC && form.menu_bar_icon} />
      <McpCard />
    </div>
  );
}

function AutostartCard({ menuBar }: { menuBar: boolean }) {
  const toast = useToast();
  const enabled = useAsync(() => api.autostartEnabled(), []);
  const toggle = async (next: boolean) => {
    try {
      enabled.setData(await api.setAutostart(next));
    } catch (e) {
      toast.error(errorMessage(e));
    }
  };
  return (
    <Card title={t("System start")}>
      <CardBody>
        <Toggle
          checked={enabled.data ?? false}
          onChange={toggle}
          label={t("Open Midnight at login")}
          hint={menuBar ? t("Starts quietly in the menu bar without opening the window") : t("Opens the window when you log in")}
        />
      </CardBody>
    </Card>
  );
}

function CopyBlock({ text }: { text: string }) {
  const [copied, setCopied] = useState(false);
  const copy = async () => {
    await navigator.clipboard.writeText(text);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };
  return (
    <div className="flex items-start gap-2">
      <pre className="min-w-0 flex-1 overflow-x-auto rounded-md bg-subtle px-2.5 py-2 text-xs" data-selectable>
        {text}
      </pre>
      <Button icon={copied ? <Check className="size-3.5" /> : <Copy className="size-3.5" />} onClick={copy}>
        {copied ? t("Copied") : t("Copy")}
      </Button>
    </div>
  );
}

function McpCard() {
  const toast = useToast();
  const setup = useAsync(() => api.mcpSetup(), []);
  const [installing, setInstalling] = useState(false);
  const s = setup.data;
  const install = async () => {
    setInstalling(true);
    try {
      await api.mcpInstallClaudeDesktop();
      setup.reload();
      toast.success(t("Added to Claude Desktop. Restart Claude Desktop to use it."));
    } catch (e) {
      toast.error(errorMessage(e));
    } finally {
      setInstalling(false);
    }
  };
  return (
    <Card title={t("MCP server")} actions={s && (s.server_exists ? <Badge tone="pos">{t("Available")}</Badge> : <Badge tone="warn">{t("Not found")}</Badge>)}>
      <CardBody>
        <p className="text-xs text-muted">
          {t("Lets AI assistants such as Claude read and edit your bookkeeping. The server runs locally on the same data, even while the app is closed.")}
        </p>
        {setup.error && <ErrorNote message={setup.error} />}
        {s && !s.server_exists && (
          <p className="text-xs text-warn">{t("midnight-mcp was not found next to the app. In development, build it with: cargo build --bin midnight-mcp")}</p>
        )}
        {s && (
          <>
            <Section title="Claude Desktop">
              <div className="flex flex-wrap items-center gap-3">
                <Button variant={s.desktop_configured ? undefined : "primary"} loading={installing} disabled={!s.server_exists} onClick={install}>
                  {s.desktop_configured ? t("Add again") : t("Add to Claude Desktop")}
                </Button>
                {s.desktop_configured ? <Badge tone="pos">{t("Configured")}</Badge> : <Badge>{t("Not configured")}</Badge>}
              </div>
              <p className="text-xs text-muted">{t("Or add this to the configuration file yourself:")}</p>
              {s.desktop_config_path && (
                <code className="block text-xs text-muted" data-selectable>
                  {s.desktop_config_path}
                </code>
              )}
              <CopyBlock text={s.desktop_snippet} />
            </Section>
            <Section title="Claude Code">
              <p className="text-xs text-muted">{t("Run in the terminal:")}</p>
              <CopyBlock text={s.claude_code_command} />
            </Section>
          </>
        )}
      </CardBody>
    </Card>
  );
}

function CompanyTab() {
  const { form, set, setMany, text, save, busy, dirty, settings } = useSettingsForm();
  const settingsKey = `${settings.company_zip} ${settings.company_city}`;
  return (
    <Card title={t("Company details")}>
      <div className="grid gap-4 p-4 md:grid-cols-2">
        <Field label={t("Company name")}>
          <Input {...text("company_name")} />
        </Field>
        <Field label={t("Owner")}>
          <Input {...text("company_owner")} />
        </Field>
        <Field label={t("Street and number")}>
          <Input {...text("company_address_line1")} />
        </Field>
        <Field label={t("Address line 2")}>
          <Input {...text("company_address_line2")} placeholder={t("optional")} />
        </Field>
        <Field label={t("Postcode and town")}>
          <PlaceInput
            key={settingsKey}
            zip={form.company_zip}
            city={form.company_city}
            onChange={(p) => setMany({ company_zip: p.zip, company_city: p.city, ...(p.country && { company_country: p.country }) })}
          />
        </Field>
        <Field label={t("Country")}>
          <CountrySelect value={form.company_country} onChange={(v) => set("company_country", v)} />
        </Field>
        <Field label={t("Email")}>
          <Input {...text("company_email")} />
        </Field>
        <Field label={t("Phone")}>
          <Input {...text("company_phone")} />
        </Field>
        <Field label={t("Website")}>
          <Input {...text("company_website")} />
        </Field>
        <Field label={t("Tax number")}>
          <Input {...text("company_tax_number")} />
        </Field>
        <Field label={t("VAT ID")}>
          <Input {...text("company_vat_id")} />
        </Field>
        <Field label={t("Bank")}>
          <Input {...text("company_bank_name")} />
        </Field>
        <Field label={t("IBAN (for invoices)")}>
          <Input {...text("company_iban")} />
        </Field>
        <Field label="BIC">
          <Input {...text("company_bic")} />
        </Field>
        <Field label={t("Base currency")}>
          <CurrencySelect value={form.base_currency} onChange={(e) => set("base_currency", e.target.value)} />
        </Field>
        <Field label={t("Maximum hours per week")}>
          <Input
            type="number"
            min={0}
            max={168}
            step={0.5}
            placeholder="40"
            value={form.weekly_hours_limit > 0 ? form.weekly_hours_limit : ""}
            onChange={(e) => set("weekly_hours_limit", Math.min(168, Math.max(0, Number(e.target.value) || 0)))}
          />
        </Field>
        <div className="md:col-span-2">
          <Toggle
            checked={form.small_business}
            onChange={(v) => {
              set("small_business", v);
              if (!v && form.default_vat_rate === 0) set("default_vat_rate", 19);
            }}
            label={t("Small business (§ 19 UStG)")} hint={t("New invoices without VAT and with a notice on the PDF")} />
        </div>
      </div>
      <SaveBar onSave={save} busy={busy} dirty={dirty} />
    </Card>
  );
}

function InvoicesTab() {
  const { form, set, text, save, busy, dirty } = useSettingsForm();
  const year = new Date().getFullYear();
  const preview = `${form.invoice_prefix.replace("{YYYY}", String(year))}${"1".padStart(form.invoice_number_digits, "0")}`;
  return (
    <Card title={t("Invoice defaults")}>
      <div className="grid gap-4 p-4 md:grid-cols-2">
        <Field label={t("Number format")} hint={t("Preview: {preview}", { preview })}>
          <Input {...text("invoice_prefix")} />
        </Field>
        <Field label={t("Digits of the sequence number")}>
          <Input type="number" min={1} max={10} value={form.invoice_number_digits} onChange={(e) => set("invoice_number_digits", Number(e.target.value) || 4)} />
        </Field>
        <Field label={t("Default VAT %")}>
          <Input type="number" value={form.default_vat_rate} onChange={(e) => set("default_vat_rate", Number(e.target.value))} />
        </Field>
        <Field label={t("Payment terms (days)")}>
          <Input type="number" value={form.default_payment_terms} onChange={(e) => set("default_payment_terms", Number(e.target.value))} />
        </Field>
        <Field label={t("Default note on invoices")} className="md:col-span-2">
          <Textarea {...text("invoice_note")} />
        </Field>
        <div className="md:col-span-2">
          <Toggle checked={form.notify_overdue} onChange={(v) => set("notify_overdue", v)} label={t("Report overdue invoices")} hint={t("Post to the activity feed as soon as an invoice becomes overdue")} />
        </div>
        <div className="md:col-span-2">
          <Toggle
            checked={form.e_invoice}
            onChange={(v) => set("e_invoice", v)}
            label={t("E-invoice (ZUGFeRD)")}
            hint={t("Embeds the invoice data as EN 16931 XML in every invoice PDF (PDF/A-3), the format for B2B invoices in Germany")}
          />
        </div>
      </div>
      <SaveBar onSave={save} busy={busy} dirty={dirty} />
    </Card>
  );
}

function DataTab() {
  const toast = useToast();
  const location = useAsync(() => api.dataLocation(), []);
  const [running, setRunning] = useState(false);
  return (
    <div className="flex flex-col gap-4">
      <Card title={t("Storage location")}>
        <CardBody>
          <p className="text-xs text-muted">{t("This is where the database (midnight.db) and all files live. Back up this folder to keep your data safe.")}</p>
          <code className="block rounded-md bg-subtle px-2.5 py-2 text-xs" data-selectable>
            {location.data}
          </code>
          <Button className="self-start" icon={<FolderOpen className="size-3.5" />} onClick={() => api.revealDataDir().catch((e) => toast.error(errorMessage(e)))}>
            {t("Open data folder")}
          </Button>
        </CardBody>
      </Card>
      <Card title={t("Background jobs")}>
        <CardBody>
          <p className="text-xs text-muted">{t("Run every 15 minutes: mark overdue invoices and book recurring expenses.")}</p>
          <Button
            className="self-start"
            loading={running}
            onClick={async () => {
              setRunning(true);
              try {
                const r = await api.runJobs();
                toast.success(
                  t("{overdue} overdue, {expenses} recurring expense(s) booked", {
                    overdue: r.overdue.length,
                    expenses: r.expenses_created.length,
                  }),
                );
              } catch (e) {
                toast.error(errorMessage(e));
              } finally {
                setRunning(false);
              }
            }}
          >
            {t("Run now")}
          </Button>
        </CardBody>
      </Card>
      <ResetCard />
    </div>
  );
}

function CentsInput({ cents, onChange, placeholder }: { cents: number | null | undefined; onChange: (cents: number | null) => void; placeholder?: string }) {
  const [text, setText] = useState(centsToInput(cents));
  useEffect(() => setText(centsToInput(cents)), [cents]);
  return (
    <div className="relative">
      <Input
        value={text}
        placeholder={placeholder}
        className="w-full pr-7 tabular-nums text-right"
        onChange={(e) => setText(e.target.value)}
        onBlur={() => onChange(parseMoney(text))}
      />
      <span className="pointer-events-none absolute top-1/2 right-2.5 -translate-y-1/2 text-xs text-muted">€</span>
    </div>
  );
}

const TAX_FIELDS: { key: TaxParamKey; label: string; unit: "money" | "days" }[] = [
  { key: "home_office_per_day", label: t("Home office allowance per day"), unit: "money" },
  { key: "home_office_max_days", label: t("Home office days per year, maximum"), unit: "days" },
  { key: "commute_first_20", label: t("Commuter allowance per km, up to 20 km"), unit: "money" },
  { key: "commute_beyond", label: t("Commuter allowance per km, from 21 km"), unit: "money" },
  { key: "employee_allowance", label: t("Employee lump-sum allowance"), unit: "money" },
  { key: "special_allowance", label: t("Special expenses lump sum"), unit: "money" },
  { key: "trainer_allowance", label: t("Coaching allowance"), unit: "money" },
  { key: "volunteer_allowance", label: t("Volunteer allowance"), unit: "money" },
  { key: "family_insurance_limit", label: t("Family insurance, monthly income limit"), unit: "money" },
  { key: "study_cost_max", label: t("First degree, maximum amount"), unit: "money" },
];

function LimitsTab() {
  const { form, set, save, busy, dirty } = useSettingsForm();
  const thisYear = new Date().getFullYear();
  const [year, setYear] = useState(thisYear);
  const knownYears = useAsync(() => api.taxYears(), []);
  const years = knownYears.data ?? [thisYear];
  useEffect(() => {
    if (years.length > 0 && !years.includes(year)) setYear(years.find((y) => y <= thisYear) ?? years[0]);
  }, [knownYears.data]);
  const defaults = useAsync(() => api.defaultTaxParams(year), [year]);
  const limits = form.limits;
  const setLimit = (patch: Partial<Limits>) => set("limits", { ...limits, ...patch });
  const overrides = form.tax_overrides[year] ?? {};
  const setOverride = (key: TaxParamKey, value: number | null) => {
    const next: TaxOverrides = { ...overrides, [key]: value };
    if (value === null) delete next[key];
    const all: Record<string, TaxOverrides> = { ...form.tax_overrides, [year]: next };
    if (Object.keys(next).length === 0) delete all[year];
    set("tax_overrides", all);
  };
  const changed = Object.keys(overrides).length;

  return (
    <div className="flex flex-col gap-4">
      <Card title={t("VAT and expenses")}>
        <div className="grid gap-x-6 gap-y-4 p-4 md:grid-cols-2">
          <Field label={t("Small business limit, previous year")}>
            <CentsInput cents={limits.small_business_cents} onChange={(c) => setLimit({ small_business_cents: c ?? 0 })} />
          </Field>
          <Field label={t("Small business limit, current year")}>
            <CentsInput cents={limits.small_business_upper_cents} onChange={(c) => setLimit({ small_business_upper_cents: c ?? 0 })} />
          </Field>
          <Field label={t("Low-value asset limit (GWG), gross")}>
            <CentsInput cents={limits.gwg_gross_cents} onChange={(c) => setLimit({ gwg_gross_cents: c ?? 0 })} />
          </Field>
          <Field label={t("Business meals, deductible share")}>
            <div className="relative">
              <Input
                type="number"
                min={0}
                max={100}
                value={limits.meals_deductible_percent}
                className="w-full pr-7 tabular-nums text-right"
                onChange={(e) => setLimit({ meals_deductible_percent: Math.min(100, Math.max(0, Math.round(Number(e.target.value) || 0))) })}
              />
              <span className="pointer-events-none absolute top-1/2 right-2.5 -translate-y-1/2 text-xs text-muted">%</span>
            </div>
          </Field>
        </div>
        <SaveBar onSave={save} busy={busy} dirty={dirty} />
      </Card>

      <Card
        title={t("Tax allowances")}
        actions={
          <Select value={year} onChange={(e) => setYear(Number(e.target.value))} className="h-7 w-24" aria-label={t("Year")}>
            {years.map((y) => (
              <option key={y} value={y}>
                {y}
              </option>
            ))}
          </Select>
        }
      >
        {changed > 0 && (
          <div className="flex justify-end px-4 pt-4">
            <Button size="sm" variant="ghost" icon={<RotateCcw className="size-3" />} onClick={() => set("tax_overrides", Object.fromEntries(Object.entries(form.tax_overrides).filter(([y]) => Number(y) !== year)))}>
              {t("Defaults")}
            </Button>
          </div>
        )}
        <div className="grid gap-x-6 gap-y-4 p-4 md:grid-cols-2">
          {TAX_FIELDS.map(({ key, label, unit }) => {
            const fallback = defaults.data?.[key];
            const value = overrides[key];
            const hint = fallback === undefined ? undefined : t("Default: {value}", { value: unit === "days" ? t("{n} days", { n: fallback }) : money(fallback) });
            return (
              <Field key={key} label={label} hint={hint}>
                {unit === "days" ? (
                  <Input
                    type="number"
                    min={0}
                    value={value ?? ""}
                    placeholder={fallback === undefined ? "" : String(fallback)}
                    className="w-full tabular-nums text-right"
                    onChange={(e) => setOverride(key, e.target.value === "" ? null : Math.max(0, Math.round(Number(e.target.value) || 0)))}
                  />
                ) : (
                  <CentsInput cents={value} placeholder={fallback === undefined ? "" : centsToInput(fallback)} onChange={(c) => setOverride(key, c)} />
                )}
              </Field>
            );
          })}
        </div>
        <SaveBar onSave={save} busy={busy} dirty={dirty} />
      </Card>
    </div>
  );
}

const RESET_WORD = t("RESET");

function ResetCard() {
  const toast = useToast();
  const [open, setOpen] = useState(false);
  const [confirm, setConfirm] = useState("");
  const [busy, setBusy] = useState(false);
  const close = () => {
    setOpen(false);
    setConfirm("");
  };
  const reset = async () => {
    setBusy(true);
    try {
      await api.resetAllData();
      try {
        localStorage.clear();
      } catch {
      }
      window.location.reload();
    } catch (e) {
      toast.error(errorMessage(e));
      setBusy(false);
    }
  };
  return (
    <Card title={t("Reset all data")}>
      <CardBody>
        <p className="text-xs text-muted">
          {t("Deletes all data and settings irreversibly.")}
        </p>
        <Button variant="danger" icon={<Trash2 className="size-3.5" />} className="self-start" onClick={() => setOpen(true)}>
          {t("Reset all data")}
        </Button>
      </CardBody>
      <Dialog
        open={open}
        onClose={close}
        title={t("Reset all data?")}
        footer={
          <>
            <Button onClick={close}>{t("Cancel")}</Button>
            <Button variant="danger" loading={busy} disabled={confirm !== RESET_WORD} onClick={reset}>
              {t("Delete permanently")}
            </Button>
          </>
        }
      >
        <p className="text-[13px]">{t("All data in this app will be deleted. If you want to keep it, back up the data folder first.")}</p>
        <Field label={t("Type \"{word}\" to confirm", { word: RESET_WORD })}>
          <Input value={confirm} onChange={(e) => setConfirm(e.target.value)} autoFocus />
        </Field>
      </Dialog>
    </Card>
  );
}

export default function SettingsPage() {
  const { params } = useApp();
  const [tab, setTab] = useState<Tab>((params.tab as Tab) ?? "general");
  return (
    <div className="flex flex-col gap-4">
      <PageHeader title={t("Settings")} subtitle={t("General settings, company details, invoice defaults, PDF templates and tax limits.")} />
      <Tabs
        items={[
          { value: "general", label: t("General") },
          { value: "company", label: t("Company") },
          { value: "invoices", label: t("Invoices") },
          { value: "latex", label: "LaTeX" },
          { value: "limits", label: t("Limits & allowances") },
          { value: "data", label: t("Data") },
        ]}
        value={tab}
        onChange={setTab}
      />
      {tab === "general" && <GeneralTab />}
      {tab === "company" && <CompanyTab />}
      {tab === "invoices" && <InvoicesTab />}
      {tab === "latex" && <LatexTemplates />}
      {tab === "limits" && <LimitsTab />}
      {tab === "data" && <DataTab />}
    </div>
  );
}
