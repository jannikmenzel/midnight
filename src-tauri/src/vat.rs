use crate::customers::{country_of, Customer};
use crate::db::Settings;
use crate::i18n::pick;
use crate::invoices::Invoice;
use crate::util::clean;

const EU: &[&str] = &[
    "AT", "BE", "BG", "CY", "CZ", "DE", "DK", "EE", "ES", "FI", "FR", "GR", "HR", "HU", "IE", "IT", "LT", "LU", "LV", "MT", "NL", "PL", "PT", "RO", "SE", "SI", "SK",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaxCase {
    Standard,
    SmallBusiness,
    ReverseCharge,
    NotTaxable,
    Exempt,
}

impl TaxCase {
    pub fn of(inv: &Invoice, customer: Option<&Customer>, s: &Settings) -> Self {
        if inv.vat_rate > 0.0 {
            return Self::Standard;
        }
        if s.small_business {
            return Self::SmallBusiness;
        }
        let Some(c) = customer else { return Self::Exempt };
        let country = country_of(c, s);
        match country.as_str() {
            home if home == s.company_country => Self::Exempt,
            eu if EU.contains(&eu) && clean(&c.vat_number).is_some() => Self::ReverseCharge,
            eu if EU.contains(&eu) => Self::Exempt,
            _ => Self::NotTaxable,
        }
    }

    pub fn category_code(self) -> &'static str {
        match self {
            Self::Standard => "S",
            Self::SmallBusiness | Self::Exempt => "E",
            Self::ReverseCharge => "AE",
            Self::NotTaxable => "O",
        }
    }

    pub fn exemption(self) -> Option<(&'static str, Option<&'static str>)> {
        match self {
            Self::Standard | Self::Exempt => None,
            Self::SmallBusiness => Some((pick("Small business according to § 19 UStG", "Kleinunternehmer gemäß § 19 UStG"), None)),
            Self::ReverseCharge => Some((pick("Reverse charge", "Steuerschuldnerschaft des Leistungsempfängers"), Some("VATEX-EU-AE"))),
            Self::NotTaxable => Some((pick("Not taxable in Germany", "Nicht im Inland steuerbare Leistung"), Some("VATEX-EU-O"))),
        }
    }

    pub fn note(self) -> &'static str {
        match self {
            Self::SmallBusiness => pick("No VAT is shown in accordance with § 19 UStG.", "Gemäß § 19 UStG wird keine Umsatzsteuer ausgewiesen."),
            Self::ReverseCharge => pick("Reverse charge: the recipient of the service is liable for VAT.", "Steuerschuldnerschaft des Leistungsempfängers (Reverse Charge)."),
            Self::NotTaxable => pick("Not taxable in Germany (place of supply abroad).", "Nicht im Inland steuerbare Leistung (Leistungsort im Ausland)."),
            Self::Standard | Self::Exempt => "",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn customer(country: Option<&str>, vat_number: Option<&str>) -> Customer {
        Customer { name: "Acme".into(), country: country.map(String::from), vat_number: vat_number.map(String::from), ..Customer::default() }
    }

    #[test]
    fn picks_the_vat_category() {
        let s = Settings::default();
        let zero = Invoice { vat_rate: 0.0, ..Invoice::default() };
        assert_eq!(TaxCase::of(&Invoice { vat_rate: 19.0, ..Invoice::default() }, None, &s), TaxCase::Standard);
        assert_eq!(TaxCase::of(&zero, None, &Settings { small_business: true, ..Settings::default() }), TaxCase::SmallBusiness);
        assert_eq!(TaxCase::of(&zero, Some(&customer(Some("FR"), Some("FR12345678901"))), &s), TaxCase::ReverseCharge);
        assert_eq!(TaxCase::of(&zero, Some(&customer(Some("FR"), None)), &s), TaxCase::Exempt);
        assert_eq!(TaxCase::of(&zero, Some(&customer(Some("CH"), None)), &s), TaxCase::NotTaxable);
        assert_eq!(TaxCase::of(&zero, Some(&customer(None, None)), &s), TaxCase::Exempt, "no country means the company's country");
        let austrian = Settings { company_country: "AT".into(), ..Settings::default() };
        assert_eq!(TaxCase::of(&zero, Some(&customer(Some("DE"), Some("DE123456789"))), &austrian), TaxCase::ReverseCharge, "domestic follows the company country");
    }
}
