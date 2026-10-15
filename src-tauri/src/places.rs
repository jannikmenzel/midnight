
use std::sync::OnceLock;

use serde::Serialize;

use crate::error::{msg, AppResult};
use crate::i18n::{pick, tr};

const DATA: &str = include_str!("../data/postal_codes.tsv");
const LIMIT: usize = 12;

const COUNTRY_DATA: &str = include_str!("../data/countries.tsv");

struct Country {
    code: &'static str,
    en: &'static str,
    de: &'static str,
}

fn countries() -> &'static [Country] {
    static LIST: OnceLock<Vec<Country>> = OnceLock::new();
    LIST.get_or_init(|| {
        COUNTRY_DATA
            .lines()
            .filter_map(|line| {
                let mut f = line.split('\t');
                Some(Country { code: f.next()?, en: f.next()?, de: f.next()? })
            })
            .collect()
    })
}

pub const HOME_COUNTRY: &str = "DE";

pub fn is_country(code: &str) -> bool {
    countries().binary_search_by(|c| c.code.cmp(code)).is_ok()
}

pub fn country_name(code: &str) -> String {
    let code = code.trim();
    countries().iter().find(|c| c.code == code).map_or_else(|| code.to_string(), |c| pick(c.en, c.de).to_string())
}

pub fn country_from_text(raw: &str) -> Option<String> {
    let text = raw.trim();
    let upper = text.to_uppercase();
    if is_country(&upper) {
        return Some(upper);
    }
    let lower = text.to_lowercase();
    countries().iter().find(|c| c.en.to_lowercase() == lower || c.de.to_lowercase() == lower).map(|c| c.code.to_string())
}

pub fn parse_country(raw: Option<&str>) -> AppResult<Option<String>> {
    let Some(code) = raw.map(|c| c.trim().to_ascii_uppercase()).filter(|c| !c.is_empty()) else { return Ok(None) };
    if is_country(&code) {
        Ok(Some(code))
    } else {
        Err(msg(tr!("Unknown country code {code} (ISO 3166, e.g. DE)", "Unbekannter Ländercode {code} (ISO 3166, z. B. DE)")))
    }
}

pub async fn list_countries() -> AppResult<Vec<&'static str>> {
    Ok(countries().iter().map(|c| c.code).collect())
}

#[derive(Debug, Clone, Serialize)]
pub struct Place {
    pub country: String,
    pub zip: String,
    pub city: String,
    pub state: String,
}

struct Entry {
    place: Place,
    key: String,
}

fn fold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars().flat_map(char::to_lowercase) {
        match c {
            'ä' | 'á' | 'à' | 'â' => out.push('a'),
            'ö' | 'ó' | 'ò' | 'ô' => out.push('o'),
            'ü' | 'ú' | 'ù' | 'û' => out.push('u'),
            'é' | 'è' | 'ê' | 'ë' => out.push('e'),
            'í' | 'ì' | 'î' | 'ï' => out.push('i'),
            'ç' => out.push('c'),
            'ß' => out.push_str("ss"),
            _ => out.push(c),
        }
    }
    out
}

fn entries() -> &'static [Entry] {
    static ENTRIES: OnceLock<Vec<Entry>> = OnceLock::new();
    ENTRIES.get_or_init(|| {
        DATA.lines()
            .filter_map(|line| {
                let mut f = line.split('\t');
                let place = Place { country: f.next()?.into(), zip: f.next()?.into(), city: f.next()?.into(), state: f.next().unwrap_or("").into() };
                Some(Entry { key: fold(&place.city), place })
            })
            .collect()
    })
}

pub fn search(query: &str, country: Option<&str>) -> Vec<Place> {
    let q = query.trim();
    if q.is_empty() {
        return Vec::new();
    }
    let country = country.map(str::to_uppercase).filter(|c| matches!(c.as_str(), "DE" | "AT" | "CH"));
    let in_country = |e: &&Entry| country.as_deref().is_none_or(|c| e.place.country == c);
    if q.chars().all(|c| c.is_ascii_digit()) {
        return entries().iter().filter(in_country).filter(|e| e.place.zip.starts_with(q)).take(LIMIT).map(|e| e.place.clone()).collect();
    }
    let k = fold(q);
    let mut hits: Vec<&Entry> = entries().iter().filter(in_country).filter(|e| e.key.contains(&k)).collect();
    hits.sort_by_key(|e| (!e.key.starts_with(&k), e.key.len(), e.place.country != "DE", e.place.zip.clone()));
    hits.into_iter().take(LIMIT).map(|e| e.place.clone()).collect()
}

pub async fn suggest_places(query: String, country: Option<String>) -> AppResult<Vec<Place>> {
    Ok(search(&query, country.as_deref()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_by_postal_code_and_town() {
        let by_zip = search("01069", None);
        assert_eq!((by_zip[0].city.as_str(), by_zip[0].country.as_str()), ("Dresden", "DE"));
        assert!(search("0106", Some("DE")).iter().all(|p| p.zip.starts_with("0106")));
        assert!(search("munchen", None).iter().any(|p| p.city == "München"));
        assert!(search("Zürich", None).iter().any(|p| p.country == "CH" && p.zip == "8001"));
        assert!(search("1010", Some("AT")).iter().all(|p| p.city == "Wien"));
        assert!(search("Deutsche Telekom", None).is_empty(), "company postcodes are filtered out");
    }

    #[test]
    fn countries_are_sorted_iso_codes_with_names() {
        assert!(countries().windows(2).all(|w| w[0].code < w[1].code), "binary search needs a sorted list");
        assert!(is_country("DE") && is_country("SS") && !is_country("EU") && !is_country("UK"), "only assigned ISO 3166-1 codes");
        assert_eq!(country_from_text("Frankreich").as_deref(), Some("FR"));
        assert_eq!(country_from_text("united kingdom").as_deref(), Some("GB"));
        assert_eq!(country_from_text("ÖSTERREICH").as_deref(), Some("AT"));
        assert_eq!(country_from_text("Atlantis"), None);
        assert_eq!(country_name("US"), "United States");
        assert_eq!(parse_country(Some(" de ")).unwrap().as_deref(), Some("DE"));
        assert_eq!(parse_country(Some("")).unwrap(), None);
        assert!(parse_country(Some("UK")).is_err() && parse_country(Some("Deutschland")).is_err());
    }
}
