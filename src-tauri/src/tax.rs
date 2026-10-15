use std::collections::BTreeMap;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::i18n::tr;
use crate::db::{load_settings, AppState, Settings};
use crate::error::AppResult;
use crate::util::{fmt_money, now_ts};

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Params {
    pub year: i32,
    pub basic_allowance: i64,
    zone2_end: f64,
    zone3_end: f64,
    zone4_end: f64,
    z2_a: f64,
    z3_a: f64,
    z3_c: f64,
    z4_c: f64,
    z5_c: f64,
    pub soli_threshold: i64,
    pub home_office_per_day: i64,
    pub home_office_max_days: i64,
    pub employee_allowance: i64,
    pub special_allowance: i64,
    pub(crate) commute_first_20: i64,
    pub(crate) commute_beyond: i64,
    pub trainer_allowance: i64,
    pub volunteer_allowance: i64,
    pub family_insurance_limit: i64,
    pub study_cost_max: i64,
}

const PARAMS: [Params; 3] = [
    Params {
        year: 2024,
        basic_allowance: 1_178_400,
        zone2_end: 17_005.0,
        zone3_end: 66_760.0,
        zone4_end: 277_825.0,
        z2_a: 954.80,
        z3_a: 181.19,
        z3_c: 991.21,
        z4_c: 10_636.31,
        z5_c: 18_971.06,
        soli_threshold: 1_813_000,
        home_office_per_day: 600,
        home_office_max_days: 210,
        employee_allowance: 123_000,
        special_allowance: 3_600,
        commute_first_20: 30,
        commute_beyond: 38,
        trainer_allowance: 300_000,
        volunteer_allowance: 84_000,
        family_insurance_limit: 50_500,
        study_cost_max: 600_000,
    },
    Params {
        year: 2025,
        basic_allowance: 1_209_600,
        zone2_end: 17_443.0,
        zone3_end: 68_480.0,
        zone4_end: 277_825.0,
        z2_a: 932.30,
        z3_a: 176.64,
        z3_c: 1_015.13,
        z4_c: 10_911.92,
        z5_c: 19_246.67,
        soli_threshold: 1_995_000,
        home_office_per_day: 600,
        home_office_max_days: 210,
        employee_allowance: 123_000,
        special_allowance: 3_600,
        commute_first_20: 30,
        commute_beyond: 38,
        trainer_allowance: 300_000,
        volunteer_allowance: 84_000,
        family_insurance_limit: 53_500,
        study_cost_max: 600_000,
    },
    Params {
        year: 2026,
        basic_allowance: 1_234_800,
        zone2_end: 17_799.0,
        zone3_end: 69_878.0,
        zone4_end: 277_825.0,
        z2_a: 914.51,
        z3_a: 173.10,
        z3_c: 1_034.87,
        z4_c: 11_135.63,
        z5_c: 19_470.38,
        soli_threshold: 2_035_000,
        home_office_per_day: 600,
        home_office_max_days: 210,
        employee_allowance: 123_000,
        special_allowance: 3_600,
        commute_first_20: 38,
        commute_beyond: 38,
        trainer_allowance: 330_000,
        volunteer_allowance: 96_000,
        family_insurance_limit: 56_500,
        study_cost_max: 600_000,
    },
];

pub fn params_for(year: i32) -> Params {
    *PARAMS.iter().min_by_key(|p| (p.year - year).abs()).unwrap()
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct ParamOverrides {
    pub home_office_per_day: Option<i64>,
    pub home_office_max_days: Option<i64>,
    pub employee_allowance: Option<i64>,
    pub special_allowance: Option<i64>,
    pub commute_first_20: Option<i64>,
    pub commute_beyond: Option<i64>,
    pub trainer_allowance: Option<i64>,
    pub volunteer_allowance: Option<i64>,
    pub family_insurance_limit: Option<i64>,
    pub study_cost_max: Option<i64>,
}

impl Params {
    fn with(mut self, o: Option<&ParamOverrides>) -> Self {
        let Some(o) = o else { return self };
        let set = |field: &mut i64, value: Option<i64>| {
            if let Some(v) = value.filter(|v| *v >= 0) {
                *field = v;
            }
        };
        set(&mut self.home_office_per_day, o.home_office_per_day);
        set(&mut self.home_office_max_days, o.home_office_max_days);
        set(&mut self.employee_allowance, o.employee_allowance);
        set(&mut self.special_allowance, o.special_allowance);
        set(&mut self.commute_first_20, o.commute_first_20);
        set(&mut self.commute_beyond, o.commute_beyond);
        set(&mut self.trainer_allowance, o.trainer_allowance);
        set(&mut self.volunteer_allowance, o.volunteer_allowance);
        set(&mut self.family_insurance_limit, o.family_insurance_limit);
        set(&mut self.study_cost_max, o.study_cost_max);
        self
    }
}

pub async fn default_tax_params(year: i32) -> AppResult<Params> {
    Ok(params_for(year))
}

pub async fn tax_years() -> AppResult<Vec<i32>> {
    Ok(PARAMS.iter().rev().map(|p| p.year).collect())
}

pub fn income_tax(p: &Params, zve_cents: i64) -> i64 {
    let x = (zve_cents.max(0) / 100) as f64;
    let base = p.basic_allowance as f64 / 100.0;
    let tax = if x <= base {
        0.0
    } else if x <= p.zone2_end {
        let y = (x - base) / 10_000.0;
        (p.z2_a * y + 1_400.0) * y
    } else if x <= p.zone3_end {
        let z = (x - p.zone2_end) / 10_000.0;
        (p.z3_a * z + 2_397.0) * z + p.z3_c
    } else if x <= p.zone4_end {
        0.42 * x - p.z4_c
    } else {
        0.45 * x - p.z5_c
    };
    (tax.floor() as i64).max(0) * 100
}

pub fn solidarity(p: &Params, tax_cents: i64) -> i64 {
    if tax_cents <= p.soli_threshold {
        return 0;
    }
    let full = tax_cents as f64 * 0.055;
    let eased = (tax_cents - p.soli_threshold) as f64 * 0.119;
    full.min(eased).floor() as i64
}

pub fn reasonable_burden(total_income: i64) -> i64 {
    let tiers = [(1_534_000, 0.05), (5_113_000, 0.06), (i64::MAX, 0.07)];
    let (mut rest, mut lower, mut sum) = (total_income.max(0), 0i64, 0f64);
    for (upper, rate) in tiers {
        let part = rest.min(upper - lower);
        sum += part as f64 * rate;
        rest -= part;
        lower = upper;
        if rest == 0 {
            break;
        }
    }
    sum.round() as i64
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
#[serde(default)]
pub struct Job {
    pub employer: String,
    pub gross_cents: i64,
    pub wage_tax_cents: i64,
    pub soli_cents: i64,
    pub church_tax_cents: i64,
    pub pension_cents: i64,
    pub commute_days: i64,
    pub commute_km: i64,
    pub equipment_cents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
#[serde(default)]
pub struct Volunteer {
    pub organisation: String,
    pub kind: String,
    pub income_cents: i64,
    pub costs_cents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
#[serde(default)]
pub struct TaxInput {
    pub tax_id: String,
    pub tax_number: String,
    pub economic_id: String,
    pub tax_office: String,
    pub birth_date: String,
    pub activity: String,
    pub activity_start: String,
    pub refund_iban: String,
    pub church_tax_rate: f64,
    pub health_status: String,
    pub health_insurer: String,
    pub health_cents: i64,
    pub care_cents: i64,
    pub health_refund_cents: i64,
    pub sick_pay: bool,
    pub pension_voluntary_cents: i64,
    pub ruerup_cents: i64,
    pub other_insurance_cents: i64,
    pub jobs: Vec<Job>,
    pub volunteering: Vec<Volunteer>,
    pub study_cents: i64,
    pub donations_cents: i64,
    pub medical_cents: i64,
    pub household_services_cents: i64,
    pub craftsmen_cents: i64,
    pub prepayments_cents: i64,
    pub checklist: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IncomeRow {
    pub number: String,
    pub issue_date: String,
    pub customer: String,
    pub paid_at: String,
    pub amount_cents: i64,
}

#[derive(Debug, Serialize)]
pub struct ExpenseRow {
    pub name: String,
    pub date: String,
    pub category: String,
    pub euer_line: String,
    pub amount_cents: i64,
    pub deductible_cents: i64,
}

#[derive(Debug, Serialize)]
pub struct Warning {
    pub level: &'static str,
    pub text: String,
}

#[derive(Debug, Serialize)]
pub struct TaxReport {
    pub year: i32,
    pub params: Params,
    pub input: TaxInput,
    pub income: Vec<IncomeRow>,
    pub income_cents: i64,
    pub expenses: Vec<ExpenseRow>,
    pub expenses_by_line: Vec<(String, i64)>,
    pub expenses_cents: i64,
    pub home_office_days: i64,
    pub home_office_business_days: i64,
    pub home_office_business_cents: i64,
    pub profit_cents: i64,
    pub employment_gross_cents: i64,
    pub employment_costs_cents: i64,
    pub employment_costs_used_cents: i64,
    pub employment_net_cents: i64,
    pub volunteer_income_cents: i64,
    pub volunteer_tax_free_cents: i64,
    pub volunteer_taxable_cents: i64,
    pub total_income_cents: i64,
    pub basic_insurance_cents: i64,
    pub retirement_cents: i64,
    pub other_insurance_used_cents: i64,
    pub provision_cents: i64,
    pub special_cents: i64,
    pub burden_cents: i64,
    pub medical_deductible_cents: i64,
    pub taxable_income_cents: i64,
    pub income_tax_cents: i64,
    pub reduction_35a_cents: i64,
    pub solidarity_cents: i64,
    pub church_tax_cents: i64,
    pub tax_total_cents: i64,
    pub withheld_cents: i64,
    pub prepayments_cents: i64,
    pub balance_cents: i64,
    pub largest_customer: Option<(String, f64)>,
    pub deadline: String,
    pub warnings: Vec<Warning>,
}

pub fn euer_line(slug: Option<&str>) -> &'static str {
    let (en, de) = match slug.unwrap_or("") {
        "software" => ("Ongoing IT costs", "Laufende EDV-Kosten"),
        "hardware" => ("Depreciation / low-value assets (IT hardware)", "Abschreibungen / GWG (EDV-Hardware)"),
        "office" => ("Work equipment and office supplies", "Arbeitsmittel und Büromaterial"),
        "telecom" => ("Telecommunications", "Telekommunikation"),
        "training" => ("Training costs", "Fortbildungskosten"),
        "travel" => ("Travel expenses", "Reisekosten"),
        "meals" => ("Business meals", "Bewirtungsaufwendungen"),
        "marketing" => ("Advertising costs", "Werbekosten"),
        "contractors" => ("Contractors", "Fremdleistungen"),
        "insurance" => ("Contributions, fees, insurance", "Beiträge, Gebühren, Versicherungen"),
        "workspace" => ("Premises costs", "Raumkosten"),
        _ => ("Other business expenses", "Übrige Betriebsausgaben"),
    };
    crate::i18n::pick(en, de)
}

fn load_input(conn: &Connection, year: i32, settings: &Settings) -> AppResult<TaxInput> {
    let raw: Option<String> = conn.query_row("SELECT data FROM tax_years WHERE year = ?1", params![year], |r| r.get(0)).optional()?;
    let mut input: TaxInput = raw.and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default();
    if input.tax_number.is_empty() {
        input.tax_number = settings.company_tax_number.clone();
    }
    if input.health_status.is_empty() {
        input.health_status = "student".into();
    }
    Ok(input)
}

#[derive(Default)]
pub struct Adjust {
    pub extra_income: Vec<IncomeRow>,
    pub extra_expense_cents: i64,
}

pub fn compute(conn: &Connection, year: i32) -> AppResult<TaxReport> {
    compute_with(conn, year, &Adjust::default())
}

pub fn compute_with(conn: &Connection, year: i32, adj: &Adjust) -> AppResult<TaxReport> {
    let settings = load_settings(conn)?;
    let input = load_input(conn, year, &settings)?;
    let p = params_for(year).with(settings.tax_overrides.get(&year));
    let limits = &settings.limits;
    let (from, to) = (format!("{year}-01-01"), format!("{year}-12-31"));

    let mut stmt = conn.prepare(
        "SELECT invoice_number, issue_date, customer_name, substr(paid_at, 1, 10), amount_cents
         FROM invoices WHERE status = 'paid' AND substr(paid_at, 1, 10) BETWEEN ?1 AND ?2 ORDER BY paid_at, invoice_number",
    )?;
    let mut income: Vec<IncomeRow> = stmt
        .query_map(params![from, to], |r| Ok(IncomeRow { number: r.get(0)?, issue_date: r.get(1)?, customer: r.get::<_, Option<String>>(2)?.unwrap_or_else(|| tr!("No customer", "Ohne Kunde")), paid_at: r.get(3)?, amount_cents: r.get(4)? }))?
        .collect::<Result<_, _>>()?;
    income.extend(adj.extra_income.iter().cloned());
    let income_cents: i64 = income.iter().map(|i| i.amount_cents).sum();

    let mut stmt = conn.prepare(
        "SELECT e.name, e.date, c.name, e.category_slug, e.amount_cents
         FROM expenses e LEFT JOIN categories c ON c.slug = e.category_slug WHERE e.date BETWEEN ?1 AND ?2 ORDER BY e.date",
    )?;
    let expenses: Vec<ExpenseRow> = stmt
        .query_map(params![from, to], |r| {
            let slug: Option<String> = r.get(3)?;
            let amount: i64 = r.get(4)?;
            let deductible = if slug.as_deref() == Some("meals") { (amount as f64 * limits.meals_deductible_percent as f64 / 100.0).round() as i64 } else { amount };
            Ok(ExpenseRow { name: r.get(0)?, date: r.get(1)?, category: r.get::<_, Option<String>>(2)?.unwrap_or_else(|| tr!("Uncategorised", "Ohne Kategorie")), euer_line: euer_line(slug.as_deref()).into(), amount_cents: amount, deductible_cents: deductible })
        })?
        .collect::<Result<_, _>>()?;
    let mut by_line: BTreeMap<String, i64> = BTreeMap::new();
    for e in &expenses {
        *by_line.entry(e.euer_line.clone()).or_default() += e.deductible_cents;
    }
    let expenses_cents: i64 = expenses.iter().map(|e| e.deductible_cents).sum::<i64>() + adj.extra_expense_cents;

    let home_office_days = crate::tracker::home_office_days(conn, &from, &to)?;
    let ho_business = home_office_days.min(p.home_office_max_days);
    let ho_business_cents = ho_business * p.home_office_per_day;

    let profit_cents = income_cents - expenses_cents - ho_business_cents;

    let gross: i64 = input.jobs.iter().map(|j| j.gross_cents).sum();
    let commute: i64 = input
        .jobs
        .iter()
        .map(|j| j.commute_days * (j.commute_km.min(20) * p.commute_first_20 + (j.commute_km - 20).max(0) * p.commute_beyond))
        .sum();
    let equipment: i64 = input.jobs.iter().map(|j| j.equipment_cents).sum();
    let costs = commute + equipment;
    let costs_used = if input.jobs.is_empty() { 0 } else if costs > p.employee_allowance { costs } else { p.employee_allowance.min(gross) };
    let employment_net = gross - costs_used;

    let (mut vol_income, mut vol_free, mut vol_taxable) = (0, 0, 0);
    for trainer in [true, false] {
        let list: Vec<&Volunteer> = input.volunteering.iter().filter(|v| (v.kind == "trainer") == trainer).collect();
        let inc: i64 = list.iter().map(|v| v.income_cents).sum();
        let cost: i64 = list.iter().map(|v| v.costs_cents).sum();
        let allowance = if trainer { p.trainer_allowance } else { p.volunteer_allowance };
        vol_income += inc;
        vol_free += inc.min(allowance);
        vol_taxable += (inc - allowance.max(cost)).max(0);
    }

    let total_income = profit_cents + employment_net + vol_taxable;

    let health = ((input.health_cents - input.health_refund_cents).max(0) as f64 * if input.sick_pay { 0.96 } else { 1.0 }).round() as i64;
    let basic = health + input.care_cents;
    let retirement = input.jobs.iter().map(|j| j.pension_cents).sum::<i64>() + input.pension_voluntary_cents + input.ruerup_cents;
    let other_limit = if input.health_status == "family" { 190_000 } else { 280_000 };
    let other_used = input.other_insurance_cents.min((other_limit - basic).max(0));
    let provision = basic + retirement + other_used;

    let special = (input.study_cents.min(p.study_cost_max) + input.donations_cents).max(p.special_allowance);
    let burden = reasonable_burden(total_income);
    let medical = (input.medical_cents - burden).max(0);
    let taxable = (total_income - provision - special - medical).max(0);

    let est = income_tax(&p, taxable);
    let reduction = (((input.household_services_cents as f64 * 0.2).round() as i64).min(400_000) + ((input.craftsmen_cents as f64 * 0.2).round() as i64).min(120_000)).min(est);
    let est_after = est - reduction;
    let soli = solidarity(&p, est_after);
    let church = (est_after as f64 * input.church_tax_rate / 100.0).round() as i64;
    let tax_total = est_after + soli + church;
    let withheld: i64 = input.jobs.iter().map(|j| j.wage_tax_cents + j.soli_cents + j.church_tax_cents).sum();

    let mut per_customer: BTreeMap<&str, i64> = BTreeMap::new();
    for i in &income {
        *per_customer.entry(i.customer.as_str()).or_default() += i.amount_cents;
    }
    let largest = per_customer
        .iter()
        .max_by_key(|(_, v)| **v)
        .filter(|_| income_cents > 0)
        .map(|(k, v)| (k.to_string(), *v as f64 / income_cents as f64));

    let mut warnings = Vec::new();
    if input.health_status == "family" && total_income / 12 > p.family_insurance_limit {
        warnings.push(Warning {
            level: "warn",
            text: tr!(
                "Family insurance: total income of about {} per month is above the limit of {} – family insurance ends. Inform your health insurer.",
                "Familienversicherung: Das Gesamteinkommen von ca. {} pro Monat liegt über der Grenze von {} – die Familienversicherung endet. Krankenkasse informieren.",
                fmt_money(total_income / 12, "EUR"),
                fmt_money(p.family_insurance_limit, "EUR")
            ),
        });
    }
    if let Some((name, share)) = &largest {
        if *share >= 5.0 / 6.0 {
            warnings.push(Warning { level: "warn", text: tr!("Mandatory pension insurance: {:.0} % of revenue comes from {name} (from 5/6, mandatory insurance under § 2 no. 9 SGB VI may apply).", "Rentenversicherungspflicht: {:.0} % des Umsatzes kommen von {name} (ab 5/6 droht Pflichtversicherung nach § 2 Nr. 9 SGB VI).", share * 100.0) });
        } else {
            warnings.push(Warning { level: "info", text: tr!("Largest customer: {name} with {:.0} % of revenue – below 5/6, no mandatory pension insurance.", "Größter Kunde: {name} mit {:.0} % des Umsatzes – unter 5/6, keine Rentenversicherungspflicht.", share * 100.0) });
        }
    }
    if settings.small_business && income_cents > limits.small_business_upper_cents {
        warnings.push(Warning {
            level: "warn",
            text: tr!("Revenue over {}: the small business scheme no longer applies, effective immediately.", "Umsatz über {}: Die Kleinunternehmerregelung entfällt sofort.", fmt_money(limits.small_business_upper_cents, "EUR")),
        });
    } else if settings.small_business && income_cents > limits.small_business_cents {
        warnings.push(Warning {
            level: "warn",
            text: tr!("Revenue over {}: no small business scheme from {}.", "Umsatz über {}: Ab {} keine Kleinunternehmerregelung mehr.", fmt_money(limits.small_business_cents, "EUR"), year + 1),
        });
    }
    if taxable > p.basic_allowance {
        warnings.push(Warning { level: "info", text: tr!("Taxable income is above the basic allowance – quarterly prepayments may be due from {} (10 Mar, 10 Jun, 10 Sep, 10 Dec). Set money aside.", "Das zu versteuernde Einkommen liegt über dem Grundfreibetrag – ab {} sind vierteljährliche Vorauszahlungen möglich (10.03., 10.06., 10.09., 10.12.). Rücklage bilden.", year + 1) });
    }
    if vol_income > vol_free {
        warnings.push(Warning { level: "info", text: tr!("Volunteering: income exceeds the allowances, the remainder is taxable (form S or N).", "Ehrenamt: Die Einnahmen übersteigen die Freibeträge, der Rest ist steuerpflichtig (Anlage S bzw. N).") });
    }
    for e in expenses.iter().filter(|e| e.amount_cents > limits.gwg_gross_cents && (e.euer_line == euer_line(Some("hardware")) || e.euer_line == euer_line(Some("office")))) {
        warnings.push(Warning { level: "info", text: tr!("\"{}\" ({}) is above the low-value asset limit: IT hardware is immediately deductible (form AVEÜR), furniture only via depreciation.", "„{}“ ({}) liegt über der GWG-Grenze: EDV-Hardware ist sofort absetzbar (Anlage AVEÜR), Möbel nur per AfA.", e.name, fmt_money(e.amount_cents, "EUR")) });
    }
    if !input.jobs.is_empty() || profit_cents > 0 {
        warnings.push(Warning { level: "info", text: tr!("Filing is usually mandatory (self-employed profit, or wages with side income over €410). Deadline: 31 Jul {}.", "Abgabe in der Regel Pflicht (Gewinn aus selbständiger Arbeit bzw. Arbeitslohn mit Nebeneinkünften über 410 €). Frist: 31.07.{}.", year + 1) });
    }

    Ok(TaxReport {
        year,
        params: p,
        income,
        income_cents,
        expenses,
        expenses_by_line: by_line.into_iter().collect(),
        expenses_cents,
        home_office_days,
        home_office_business_days: ho_business,
        home_office_business_cents: ho_business_cents,
        profit_cents,
        employment_gross_cents: gross,
        employment_costs_cents: costs,
        employment_costs_used_cents: costs_used,
        employment_net_cents: employment_net,
        volunteer_income_cents: vol_income,
        volunteer_tax_free_cents: vol_free,
        volunteer_taxable_cents: vol_taxable,
        total_income_cents: total_income,
        basic_insurance_cents: basic,
        retirement_cents: retirement,
        other_insurance_used_cents: other_used,
        provision_cents: provision,
        special_cents: special,
        burden_cents: burden,
        medical_deductible_cents: medical,
        taxable_income_cents: taxable,
        income_tax_cents: est,
        reduction_35a_cents: reduction,
        solidarity_cents: soli,
        church_tax_cents: church,
        tax_total_cents: tax_total,
        withheld_cents: withheld,
        prepayments_cents: input.prepayments_cents,
        balance_cents: tax_total - withheld - input.prepayments_cents,
        largest_customer: largest,
        deadline: crate::i18n::fmt_date(&format!("{}-07-31", year + 1)),
        warnings,
        input,
    })
}

pub async fn get_tax_report(state: &AppState, year: i32) -> AppResult<TaxReport> {
    let conn = state.conn()?;
    compute(&conn, year)
}

pub async fn save_tax_input(state: &AppState, year: i32, input: TaxInput) -> AppResult<TaxReport> {
    let conn = state.conn()?;
    conn.execute(
        "INSERT INTO tax_years (year, data, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(year) DO UPDATE SET data = excluded.data, updated_at = excluded.updated_at",
        params![year, serde_json::to_string(&input)?, now_ts()],
    )?;
    compute(&conn, year)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tariff_matches_the_published_formula() {
        let p = params_for(2026);
        assert_eq!(income_tax(&p, 1_234_800), 0, "nothing up to the basic allowance");
        assert_eq!(income_tax(&p, 1_500_000), 43_500, "zone 2: (914.51·0.2652 + 1400)·0.2652 ≈ 435 €");
        assert_eq!(params_for(2031).year, 2026, "later years fall back to the newest known parameters");
        assert_eq!(solidarity(&p, 1_000_000), 0);
        let at = |e: i64| income_tax(&p, e * 100) / 100;
        assert!((at(17_799) - at(17_800)).abs() <= 1);
        assert!((at(69_878) - at(69_879)).abs() <= 1);
    }

    #[test]
    fn tariff_2024_is_continuous() {
        let p = params_for(2024);
        assert_eq!(p.year, 2024);
        assert_eq!(income_tax(&p, 1_178_400), 0, "nothing up to the basic allowance");
        let at = |e: i64| income_tax(&p, e * 100) / 100;
        assert!((at(17_005) - at(17_006)).abs() <= 1);
        assert!((at(66_760) - at(66_761)).abs() <= 1);
        assert!((at(277_825) - at(277_826)).abs() <= 1);
    }

    #[test]
    fn euer_lines_follow_the_language() {
        assert_eq!(euer_line(Some("software")), "Ongoing IT costs");
        assert_eq!(euer_line(None), "Other business expenses");
    }

    #[test]
    fn reasonable_burden_is_stepwise() {
        assert_eq!(reasonable_burden(1_000_000), 50_000, "5 % of 10.000 €");
        assert_eq!(reasonable_burden(2_000_000), 76_700 + 27_960, "5 % of 15.340 € + 6 % of 4.660 €");
    }

    #[test]
    fn full_year_with_job_volunteering_and_insurance() {
        let conn = crate::db::open_in_memory().unwrap();
        conn.execute_batch(
            "INSERT INTO invoices (id, invoice_number, customer_name, status, issue_date, due_date, amount_cents, paid_at) VALUES
               ('a', 'INV-1', 'Acme GmbH', 'paid', '2026-03-01', '2026-03-15', 600000, '2026-03-10'),
               ('b', 'INV-2', 'Beispiel e.V.', 'paid', '2026-05-01', '2026-05-15', 400000, '2026-05-10'),
               ('c', 'INV-3', 'Beispiel e.V.', 'paid', '2025-12-01', '2025-12-15', 99900, '2025-12-20');
             INSERT INTO expenses (id, date, name, amount_cents, category_slug) VALUES
               ('e1', '2026-04-01', 'JetBrains', 10000, 'software'),
               ('e2', '2026-04-02', 'Kundenessen', 10000, 'meals');
             INSERT INTO tracker_projects (id, name) VALUES ('p', 'Website');
             WITH RECURSIVE days(d) AS (SELECT date('2026-01-01') UNION ALL SELECT date(d, '+1 day') FROM days WHERE d < '2026-09-30')
             INSERT INTO tracker_entries (id, project_id, date, start, stop, duration) SELECT d, 'p', d, d || 'T09:00:00Z', d || 'T10:00:00Z', 3600 FROM days;",
        )
        .unwrap();
        let input = TaxInput {
            health_status: "student".into(),
            health_cents: 100_000,
            care_cents: 30_000,
            other_insurance_cents: 200_000,
            jobs: vec![Job { employer: "Beispiel AG".into(), gross_cents: 600_000, wage_tax_cents: 20_000, pension_cents: 55_800, ..Job::default() }],
            volunteering: vec![Volunteer { organisation: "Sportverein".into(), kind: "trainer".into(), income_cents: 400_000, ..Volunteer::default() }],
            ..TaxInput::default()
        };
        conn.execute("INSERT INTO tax_years (year, data) VALUES (2026, ?1)", [serde_json::to_string(&input).unwrap()]).unwrap();

        let r = compute(&conn, 2026).unwrap();
        assert_eq!(r.income_cents, 1_000_000, "only payments received in 2026 count");
        assert_eq!(r.expenses_cents, 17_000, "Bewirtung only at 70 %");
        assert_eq!((r.home_office_days, r.home_office_business_days), (273, 210), "every tracked day counts, capped at 210 days");
        assert_eq!(r.profit_cents, 1_000_000 - 17_000 - 126_000);
        assert_eq!(r.employment_costs_used_cents, 123_000, "employee allowance when actual costs are lower");
        assert_eq!(r.volunteer_taxable_cents, 70_000, "4.000 € minus the 3.300 € trainer allowance");
        assert_eq!(r.other_insurance_used_cents, 150_000, "2.800 € frame minus 1.300 € basic insurance");
        assert_eq!(r.provision_cents, 130_000 + 55_800 + 150_000);
        assert_eq!(r.special_cents, 3_600, "special expenses allowance");
        assert_eq!(r.withheld_cents, 20_000);
        assert_eq!(r.balance_cents, r.tax_total_cents - 20_000);
        assert!(r.warnings.iter().any(|w| w.text.contains("Volunteering")));
    }

    #[test]
    fn settings_override_params_and_limits() {
        let conn = crate::db::open_in_memory().unwrap();
        conn.execute_batch(
            "INSERT INTO invoices (id, invoice_number, customer_name, status, issue_date, due_date, amount_cents, paid_at) VALUES
               ('a', 'INV-1', 'Acme GmbH', 'paid', '2026-03-01', '2026-03-15', 3_000_000, '2026-03-10');
             INSERT INTO expenses (id, date, name, amount_cents, category_slug) VALUES ('e', '2026-04-02', 'Kundenessen', 10000, 'meals');",
        )
        .unwrap();
        let mut settings = load_settings(&conn).unwrap();
        settings.small_business = true;
        settings.limits.small_business_cents = 5_000_000;
        settings.limits.meals_deductible_percent = 50;
        settings.tax_overrides.insert(2026, ParamOverrides { home_office_per_day: Some(800), ..ParamOverrides::default() });
        crate::db::store_settings(&conn, &settings).unwrap();
        conn.execute_batch(
            "INSERT INTO tracker_projects (id, name) VALUES ('p', 'Website');
             INSERT INTO tracker_entries (id, project_id, date, start, stop, duration) VALUES
               ('t1', 'p', '2026-02-02', '2026-02-02T09:00:00Z', '2026-02-02T12:00:00Z', 10800),
               ('t2', 'p', '2026-02-02', '2026-02-02T13:00:00Z', '2026-02-02T15:00:00Z', 7200),
               ('t3', 'p', '2026-02-03', '2026-02-03T09:00:00Z', '2026-02-03T12:00:00Z', 10800),
               ('t4', 'p', '2026-02-04', '2026-02-04T09:00:00Z', NULL, 0);
             INSERT INTO home_office_exclusions (date) VALUES ('2026-02-03');",
        )
        .unwrap();
        let input = TaxInput::default();
        conn.execute("INSERT INTO tax_years (year, data) VALUES (2026, ?1)", [serde_json::to_string(&input).unwrap()]).unwrap();

        let r = compute(&conn, 2026).unwrap();
        assert_eq!(r.params.home_office_per_day, 800);
        assert_eq!(r.home_office_days, 1, "one day per date, excluded days and running timers do not count");
        assert_eq!(r.home_office_business_cents, 800);
        assert_eq!(r.expenses_cents, 5_000, "meals at the configured 50 %");
        let small_business_warning = |r: &TaxReport| r.warnings.iter().any(|w| w.text.contains("small business scheme"));
        assert!(!small_business_warning(&r), "30.000 € stays below the configured 50.000 €");
        assert_eq!(compute(&conn, 2025).unwrap().params.home_office_per_day, 600, "overrides only apply to their year");

        settings.limits.small_business_cents = 2_500_000;
        crate::db::store_settings(&conn, &settings).unwrap();
        assert!(small_business_warning(&compute(&conn, 2026).unwrap()), "30.000 € is over the default 25.000 €");
        settings.small_business = false;
        crate::db::store_settings(&conn, &settings).unwrap();
        assert!(!small_business_warning(&compute(&conn, 2026).unwrap()), "irrelevant without the small business scheme");
    }
}
