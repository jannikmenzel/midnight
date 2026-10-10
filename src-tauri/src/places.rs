
use std::sync::OnceLock;

use serde::Serialize;

use crate::error::AppResult;

const DATA: &str = include_str!("../data/postal_codes.tsv");
const LIMIT: usize = 12;

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
}
