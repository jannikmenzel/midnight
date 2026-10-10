import { useState } from "react";
import { api, errorMessage, type Settings } from "../lib/api";
import { Button, CurrencySelect, ErrorNote, Field, Input, Select, Textarea, Toggle } from "../components/ui";
import { isLang, lang, LANGUAGES, setLang, t } from "../lib/i18n";

const DEFAULT_NOTE = "Thank you for your business.";

export default function Onboarding({ settings, onDone }: { settings: Settings; onDone: () => void }) {
  const [form, setForm] = useState<Settings>(settings);
  const [useOwnName, setUseOwnName] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const set = <K extends keyof Settings>(k: K, v: Settings[K]) => setForm({ ...form, [k]: v });
  const companyName = useOwnName ? form.company_owner.trim() : form.company_name.trim();

  const finish = async () => {
    setBusy(true);
    setError(null);
    try {
      await api.saveSettings({
        ...form,
        company_name: companyName,
        onboarded: true,
        language: lang(),
        invoice_note: form.invoice_note === DEFAULT_NOTE ? t("Thank you for your business.") : form.invoice_note,
      });
      onDone();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="flex h-full items-center justify-center bg-bg p-6">
      <div className="w-full max-w-lg rounded-lg border border-line bg-panel p-8">
        <div className="mb-6 flex items-center gap-2">
          <svg viewBox="0 0 24 24" className="size-6" aria-hidden>
            <path d="M5 15a7 7 0 0 1 14 0z" fill="currentColor" />
            <rect x="3" y="16.5" width="18" height="1.6" rx="0.8" fill="currentColor" />
          </svg>
          <span className="text-[15px] font-semibold">Midnight</span>
          <Select value={lang()} onChange={(e) => isLang(e.target.value) && setLang(e.target.value)} className="ml-auto w-32" aria-label={t("Language")}>
            {LANGUAGES.map((l) => (
              <option key={l.value} value={l.value}>
                {l.label}
              </option>
            ))}
          </Select>
        </div>
        <div className="flex flex-col gap-4">
          <div>
            <h1 className="text-xl font-semibold tracking-tight">{t("Company details")}</h1>
            <p className="mt-1 text-[13px] text-muted">{t("These details appear on your invoices. You can change them later in the settings.")}</p>
          </div>
          <Field label={t("Your name")}>
            <Input value={form.company_owner} onChange={(e) => set("company_owner", e.target.value)} autoFocus />
          </Field>
          <Toggle checked={useOwnName} onChange={setUseOwnName} label={t("Use my name as the company name")} hint={t("If you don't have a separate company name")} />
          {!useOwnName && (
            <Field label={t("Company name")}>
              <Input value={form.company_name} onChange={(e) => set("company_name", e.target.value)} />
            </Field>
          )}
          <Field label={t("Address")}>
            <Textarea value={form.company_address} onChange={(e) => set("company_address", e.target.value)} placeholder={t("1 Example Street\n12345 Example City")} />
          </Field>
          <div className="grid grid-cols-2 gap-4">
            <Field label={t("Email")}>
              <Input value={form.company_email} onChange={(e) => set("company_email", e.target.value)} />
            </Field>
            <Field label={t("Currency")}>
              <CurrencySelect value={form.base_currency} onChange={(e) => set("base_currency", e.target.value)} />
            </Field>
          </div>
          <Toggle checked={form.small_business} onChange={(v) => set("small_business", v)} label={t("Small business under § 19 UStG")} hint={t("Invoices without VAT")} />
          {error && <ErrorNote message={error} />}
          <div className="flex justify-end">
            <Button variant="primary" onClick={finish} loading={busy} disabled={!companyName}>
              {t("Finish setup")}
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}
