import { useMemo, useState } from "react";
import { api, type Place, type Unit } from "../lib/api";
import { useAsync } from "../lib/hooks";
import { lang, locale, t } from "../lib/i18n";
import { AutocompleteInput, Select } from "./ui";

const unitLists = new Map<string, Promise<Unit[]>>();
let countryList: Promise<string[]> | null = null;

function cached<T>(load: () => Promise<T>, forget: () => void): Promise<T> {
  const promise = load();
  promise.catch(forget);
  return promise;
}

export function useUnits(): Unit[] {
  const language = lang();
  const { data } = useAsync(() => {
    if (!unitLists.has(language)) unitLists.set(language, cached(api.listUnits, () => unitLists.delete(language)));
    return unitLists.get(language)!;
  }, [language]);
  return data ?? [];
}

export function UnitSelect({ value, onChange, disabled }: { value: string; onChange: (code: string) => void; disabled?: boolean }) {
  const units = useUnits();
  return (
    <Select value={value} onChange={(e) => onChange(e.target.value)} disabled={disabled} aria-label={t("Unit")}>
      <option value="">–</option>
      {units.map((u) => (
        <option key={u.code} value={u.code}>
          {u.label}
        </option>
      ))}
    </Select>
  );
}

export function CountrySelect({ value, onChange }: { value: string; onChange: (code: string) => void }) {
  const { data } = useAsync(() => (countryList ??= cached(api.listCountries, () => (countryList = null))), []);
  const language = lang();
  const options = useMemo(() => {
    const names = new Intl.DisplayNames([locale()], { type: "region" });
    return (data ?? []).map((code) => ({ code, name: names.of(code) ?? code })).sort((a, b) => a.name.localeCompare(b.name, locale()));
  }, [data, language]);
  return (
    <Select value={value} onChange={(e) => onChange(e.target.value)} aria-label={t("Country")}>
      {!options.some((o) => o.code === value) && <option value={value}>{value || "–"}</option>}
      {options.map((o) => (
        <option key={o.code} value={o.code}>
          {o.name}
        </option>
      ))}
    </Select>
  );
}

export function PlaceInput({ zip, city, onChange }: { zip: string; city: string; onChange: (place: { zip: string; city: string; country?: string }) => void }) {
  const [text, setText] = useState([zip, city].filter(Boolean).join(" "));
  const suggest = async (query: string) =>
    (await api.suggestPlaces(query)).map((p) => ({ key: `${p.country}-${p.zip}-${p.city}`, label: `${p.zip} ${p.city}`, sub: `${p.state} · ${p.country}`, data: p }));
  const pick = (p: Place) => {
    setText(`${p.zip} ${p.city}`);
    onChange({ zip: p.zip, city: p.city, country: p.country });
  };
  const type = (value: string) => {
    setText(value);
    const m = value.trim().match(/^(\d{4,5})\b\s*(.*)$/);
    onChange(m ? { zip: m[1], city: m[2] } : { zip: "", city: value.trim() });
  };
  return <AutocompleteInput value={text} onChange={type} suggest={suggest} onPick={pick} />;
}
