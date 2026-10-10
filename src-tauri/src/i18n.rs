use std::sync::atomic::{AtomicBool, Ordering};

static GERMAN: AtomicBool = AtomicBool::new(false);

pub fn set_language(lang: &str) {
    GERMAN.store(lang == "de", Ordering::Relaxed);
}

pub fn is_de() -> bool {
    GERMAN.load(Ordering::Relaxed)
}

macro_rules! tr {
    ($en:literal, $de:literal) => {
        if $crate::i18n::is_de() { format!($de) } else { format!($en) }
    };
    ($en:literal, $de:literal, $($arg:tt)*) => {
        if $crate::i18n::is_de() { format!($de, $($arg)*) } else { format!($en, $($arg)*) }
    };
}
pub(crate) use tr;

pub fn pick(en: &'static str, de: &'static str) -> &'static str {
    if is_de() { de } else { en }
}

pub fn fmt_date(iso_date: &str) -> String {
    match crate::util::parse_date(iso_date) {
        Some(d) if is_de() => d.format("%d.%m.%Y").to_string(),
        Some(d) => d.format("%d/%m/%Y").to_string(),
        None => iso_date.to_string(),
    }
}
