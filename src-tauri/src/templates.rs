use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use rusqlite::OptionalExtension;
use serde::Serialize;

use crate::db::{load_settings, AppState, Settings};
use crate::error::{msg, AppResult, Context};
use crate::i18n::tr;
use crate::invoices::{compute_totals, Invoice, LineItem};
use crate::latex::{self, TimesheetEntry, INVOICE_PLACEHOLDERS, TIMESHEET_PLACEHOLDERS};
use crate::util::{iso, today_date};

pub const INVOICE: &str = "invoice.tex";
pub const TIMESHEET: &str = "timesheet.tex";

const DEFAULTS: &[(&str, &str)] = &[
    (INVOICE, include_str!("../templates/invoice.tex")),
    (TIMESHEET, include_str!("../templates/timesheet.tex")),
    ("Report.cls", include_str!("../templates/Report.cls")),
];

const DEFAULT_ASSETS: &[(&str, &[u8])] = &[
    ("Images/logo.pdf", include_bytes!("../templates/Images/logo.pdf")),
    ("Fonts/Inter/Inter-Regular.ttf", include_bytes!("../templates/Fonts/Inter/Inter-Regular.ttf")),
    ("Fonts/Inter/Inter-Bold.ttf", include_bytes!("../templates/Fonts/Inter/Inter-Bold.ttf")),
    ("Fonts/Inter/Inter-Italic.ttf", include_bytes!("../templates/Fonts/Inter/Inter-Italic.ttf")),
    ("Fonts/Inter/Inter-BoldItalic.ttf", include_bytes!("../templates/Fonts/Inter/Inter-BoldItalic.ttf")),
    ("Fonts/Inter/OFL.txt", include_bytes!("../templates/Fonts/Inter/OFL.txt")),
    ("Fonts/Poppins/Poppins-Light.ttf", include_bytes!("../templates/Fonts/Poppins/Poppins-Light.ttf")),
    ("Fonts/Poppins/Poppins-Regular.ttf", include_bytes!("../templates/Fonts/Poppins/Poppins-Regular.ttf")),
    ("Fonts/Poppins/Poppins-Medium.ttf", include_bytes!("../templates/Fonts/Poppins/Poppins-Medium.ttf")),
    ("Fonts/Poppins/Poppins-SemiBold.ttf", include_bytes!("../templates/Fonts/Poppins/Poppins-SemiBold.ttf")),
    ("Fonts/Poppins/Poppins-LightItalic.ttf", include_bytes!("../templates/Fonts/Poppins/Poppins-LightItalic.ttf")),
    ("Fonts/Poppins/Poppins-SemiBoldItalic.ttf", include_bytes!("../templates/Fonts/Poppins/Poppins-SemiBoldItalic.ttf")),
    ("Fonts/Poppins/OFL.txt", include_bytes!("../templates/Fonts/Poppins/OFL.txt")),
    ("Fonts/JetBrainsMono/JetBrainsMono-Light.ttf", include_bytes!("../templates/Fonts/JetBrainsMono/JetBrainsMono-Light.ttf")),
    ("Fonts/JetBrainsMono/JetBrainsMono-SemiBold.ttf", include_bytes!("../templates/Fonts/JetBrainsMono/JetBrainsMono-SemiBold.ttf")),
    ("Fonts/JetBrainsMono/JetBrainsMono-LightItalic.ttf", include_bytes!("../templates/Fonts/JetBrainsMono/JetBrainsMono-LightItalic.ttf")),
    ("Fonts/JetBrainsMono/JetBrainsMono-SemiBoldItalic.ttf", include_bytes!("../templates/Fonts/JetBrainsMono/JetBrainsMono-SemiBoldItalic.ttf")),
    ("Fonts/JetBrainsMono/OFL.txt", include_bytes!("../templates/Fonts/JetBrainsMono/OFL.txt")),
];

const TEXT_EXTENSIONS: &[&str] = &["tex", "cls", "sty", "cfg", "def", "bib"];
const FONT_EXTENSIONS: &[&str] = &["ttf", "otf"];
const IMAGE_EXTENSIONS: &[&str] = &["pdf", "png", "jpg", "jpeg", "eps"];
const MAX_TEXT_BYTES: u64 = 2 * 1024 * 1024;

pub fn default_content(name: &str) -> Option<&'static str> {
    DEFAULTS.iter().find(|(n, _)| *n == name).map(|(_, c)| *c)
}

pub fn dir(files_dir: &Path) -> PathBuf {
    files_dir.join("latex")
}

pub fn ensure(files_dir: &Path) -> AppResult<PathBuf> {
    let dir = dir(files_dir);
    for sub in ["Fonts", "Images"] {
        std::fs::create_dir_all(dir.join(sub))?;
    }
    for (name, content) in DEFAULTS {
        let path = dir.join(name);
        if !path.exists() {
            std::fs::write(path, content)?;
        }
    }
    Ok(dir)
}

pub fn init(files_dir: &Path, legacy_dir: Option<String>) -> AppResult<()> {
    let dir = dir(files_dir);
    let fresh = !dir.exists();
    if fresh {
        if let Some(src) = legacy_dir.map(|d| expand_home(&d)).filter(|p| p.is_dir()) {
            if let Err(e) = import_legacy(&src, &dir) {
                eprintln!("[latex] Could not copy the template folder {}: {e}", src.display());
            }
        }
    }
    ensure(files_dir)?;
    if fresh {
        for (rel, bytes) in DEFAULT_ASSETS {
            let path = dir.join(rel);
            if !path.exists() {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(path, bytes)?;
            }
        }
    }
    Ok(())
}

fn expand_home(path: &str) -> PathBuf {
    match path.trim().strip_prefix('~') {
        Some(rest) => PathBuf::from(format!("{}{rest}", std::env::var("HOME").unwrap_or_default())),
        None => PathBuf::from(path.trim()),
    }
}

fn import_legacy(src: &Path, dir: &Path) -> AppResult<()> {
    std::fs::create_dir_all(dir)?;
    if let Ok(cls) = std::fs::read_to_string(src.join("Template/Report.cls")) {
        std::fs::write(dir.join("Report.cls"), cls.replace("../Template/Fonts/", "Fonts/").replace("../Template/Images/", "Images/"))?;
    }
    copy_tree(&src.join("Template/Images"), &dir.join("Images"))?;
    for fonts in ["Template/Fonts", "Dokumente/Fonts"] {
        copy_tree(&src.join(fonts), &dir.join("Fonts"))?;
    }
    Ok(())
}

fn copy_tree(src: &Path, dst: &Path) -> AppResult<()> {
    if !src.is_dir() {
        return Ok(());
    }
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let target = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn extension(path: &str) -> String {
    Path::new(path).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

fn kind_of(path: &str) -> &'static str {
    let ext = extension(path);
    if TEXT_EXTENSIONS.contains(&ext.as_str()) {
        "text"
    } else if FONT_EXTENSIONS.contains(&ext.as_str()) {
        "font"
    } else if IMAGE_EXTENSIONS.contains(&ext.as_str()) {
        "image"
    } else {
        "other"
    }
}

fn resolve(dir: &Path, rel: &str) -> AppResult<PathBuf> {
    let rel_path = Path::new(rel);
    let valid = !rel.trim().is_empty()
        && rel_path.components().all(|c| matches!(c, Component::Normal(part) if !part.to_string_lossy().starts_with('.')));
    if !valid {
        return Err(msg(tr!("Invalid path: {rel}", "Ungültiger Pfad: {rel}")));
    }
    Ok(dir.join(rel_path))
}

#[derive(Debug, Serialize)]
pub struct LatexFile {
    pub path: String,
    pub size: u64,
    pub kind: &'static str,
    pub has_default: bool,
}

#[derive(Debug, Serialize)]
pub struct LatexFolder {
    pub dir: String,
    pub xelatex: Option<String>,
    pub files: Vec<LatexFile>,
    pub placeholders: BTreeMap<&'static str, &'static [&'static str]>,
}

fn walk(root: &Path, current: &Path, out: &mut Vec<LatexFile>) -> AppResult<()> {
    for entry in std::fs::read_dir(current)? {
        let entry = entry?;
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            walk(root, &path, out)?;
        } else {
            let rel = path.strip_prefix(root).unwrap_or(&path).components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
            out.push(LatexFile { kind: kind_of(&rel), has_default: default_content(&rel).is_some(), size: entry.metadata()?.len(), path: rel });
        }
    }
    Ok(())
}

pub async fn latex_files(state: &AppState) -> AppResult<LatexFolder> {
    let dir = ensure(&state.files_dir)?;
    let mut files = Vec::new();
    walk(&dir, &dir, &mut files)?;
    files.sort_by_key(|f| f.path.to_lowercase());
    Ok(LatexFolder {
        dir: dir.to_string_lossy().to_string(),
        xelatex: latex::find_xelatex().map(|p| p.to_string_lossy().to_string()),
        files,
        placeholders: BTreeMap::from([(INVOICE, INVOICE_PLACEHOLDERS), (TIMESHEET, TIMESHEET_PLACEHOLDERS)]),
    })
}

pub async fn latex_read(state: &AppState, path: String) -> AppResult<String> {
    let full = resolve(&ensure(&state.files_dir)?, &path)?;
    if kind_of(&path) != "text" {
        return Err(msg(tr!("{path} is not a text file", "{path} ist keine Textdatei")));
    }
    if std::fs::metadata(&full)?.len() > MAX_TEXT_BYTES {
        return Err(msg(tr!("{path} is too large to edit here", "{path} ist zu groß zum Bearbeiten")));
    }
    std::fs::read_to_string(&full).ctx(&tr!("Reading {path}", "{path} lesen"))
}

pub async fn latex_write(state: &AppState, path: String, content: String) -> AppResult<()> {
    let full = resolve(&ensure(&state.files_dir)?, &path)?;
    if kind_of(&path) != "text" {
        return Err(msg(tr!("Only .tex, .cls and .sty files can be edited", "Nur .tex-, .cls- und .sty-Dateien sind bearbeitbar")));
    }
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(std::fs::write(full, content)?)
}

pub async fn latex_reset(state: &AppState, path: String) -> AppResult<String> {
    let content = default_content(&path).ok_or_else(|| msg(tr!("{path} has no built-in version", "Für {path} gibt es keine eingebaute Version")))?;
    std::fs::write(resolve(&ensure(&state.files_dir)?, &path)?, content)?;
    Ok(content.to_string())
}

pub async fn latex_import(state: &AppState, paths: Vec<String>) -> AppResult<usize> {
    let dir = ensure(&state.files_dir)?;
    for src in &paths {
        let src = PathBuf::from(src);
        let name = src.file_name().map(|n| n.to_string_lossy().to_string()).ok_or_else(|| msg(tr!("Invalid file path", "Ungültiger Dateipfad")))?;
        let dest = match kind_of(&name) {
            "font" => {
                let stem = Path::new(&name).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                let family = stem.split('-').next().filter(|f| !f.is_empty()).unwrap_or(&stem).to_string();
                resolve(&dir, &format!("Fonts/{family}/{name}"))?
            }
            "image" => resolve(&dir, &format!("Images/{name}"))?,
            "text" => resolve(&dir, &name)?,
            _ => return Err(msg(tr!("Unsupported file type: {name}", "Nicht unterstützter Dateityp: {name}"))),
        };
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(&src, &dest).ctx(&tr!("Copying {name}", "{name} kopieren"))?;
    }
    Ok(paths.len())
}

pub async fn latex_delete(state: &AppState, path: String) -> AppResult<()> {
    if default_content(&path).is_some() {
        return Err(msg(tr!("{path} is required; reset it instead", "{path} wird benötigt; bitte stattdessen zurücksetzen")));
    }
    let dir = ensure(&state.files_dir)?;
    let full = resolve(&dir, &path)?;
    if full == dir.join("Fonts") || full == dir.join("Images") {
        return Err(msg(tr!("This folder is required", "Dieser Ordner wird benötigt")));
    }
    if full.is_dir() {
        std::fs::remove_dir_all(full)?;
    } else {
        std::fs::remove_file(full)?;
    }
    Ok(())
}

pub async fn latex_preview(state: &AppState, template: String) -> AppResult<String> {
    enum Data {
        Invoice(Box<(Invoice, Option<crate::customers::Customer>)>),
        Timesheet(Vec<TimesheetEntry>),
    }
    let (data, settings) = {
        let conn = state.conn()?;
        let settings = load_settings(&conn)?;
        let data = match template.as_str() {
            INVOICE => {
                let latest: Option<String> = conn
                    .query_row("SELECT id FROM invoices ORDER BY issue_date DESC, created_at DESC LIMIT 1", [], |r| r.get(0))
                    .optional()?;
                match latest {
                    Some(id) => {
                        let inv = crate::invoices::load_invoice(&conn, &id)?;
                        let customer = inv.customer_id.as_deref().map(|c| crate::customers::get_customer(&conn, c)).transpose()?;
                        Data::Invoice(Box::new((inv, customer)))
                    }
                    None => Data::Invoice(Box::new((sample_invoice(&settings), None))),
                }
            }
            TIMESHEET | "Report.cls" => {
                let to = today_date();
                let entries = crate::tracker::timesheet_in_range(&conn, &iso(to - chrono::Duration::days(60)), &iso(to), None)?;
                Data::Timesheet(if entries.is_empty() { sample_entries() } else { entries })
            }
            _ => return Err(msg(tr!("There is no preview for {template}", "Für {template} gibt es keine Vorschau"))),
        };
        (data, settings)
    };
    let tc = latex::toolchain(&state.files_dir).map_err(msg)?;
    let stem = Path::new(&template).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let out = state.files_dir.join("previews").join(format!("{stem}.pdf"));
    match data {
        Data::Invoice(data) => {
            let (inv, customer) = *data;
            latex::render_invoice(&tc, &inv, customer.as_ref(), &settings, &out)?
        }
        Data::Timesheet(entries) => {
            let first = entries.iter().filter_map(|e| crate::util::parse_date(&e.date)).min().unwrap_or_else(today_date);
            latex::render_timesheet(&tc, &entries, &tr!("Preview", "Vorschau"), first, &settings, &out)?
        }
    }
    Ok(out.to_string_lossy().to_string())
}

fn sample_invoice(s: &Settings) -> Invoice {
    let today = today_date();
    let line_items = vec![
        LineItem { name: tr!("Website – UI design ({})", "Website – UI-Design ({})", today.format("%d.%m.%y")), quantity: 12.5, price_cents: 8500, unit: Some(tr!("hrs", "Std.")) },
        LineItem { name: tr!("Hosting (monthly)", "Hosting (monatlich)"), quantity: 1.0, price_cents: 2900, unit: None },
    ];
    let vat_rate = if s.small_business { 0.0 } else { s.default_vat_rate };
    let totals = compute_totals(&line_items, vat_rate, 0);
    Invoice {
        invoice_number: tr!("INV-{}-0001", "RE-{}-0001", today.format("%Y")),
        customer_name: Some("Example Ltd".into()),
        customer_details: Some(tr!("Example Ltd\n1 Sample Road\n12345 Sample City", "Beispiel GmbH\nMusterweg 1\n12345 Musterstadt")),
        status: "draft".into(),
        issue_date: iso(today),
        due_date: iso(today + chrono::Duration::days(s.default_payment_terms)),
        currency: s.base_currency.clone(),
        line_items,
        vat_rate,
        subtotal_cents: totals.subtotal_cents,
        vat_cents: totals.vat_cents,
        amount_cents: totals.amount_cents,
        note: Some(s.invoice_note.clone()).filter(|n| !n.trim().is_empty()),
        ..Default::default()
    }
}

fn sample_entries() -> Vec<TimesheetEntry> {
    let today = today_date();
    [(9, "Kick-off & briefing", 5400), (8, "Wireframes", 10800), (6, "UI design", 14400), (2, "Feedback round", 3600), (1, "Revisions", 7200)]
        .into_iter()
        .map(|(days_ago, description, duration)| {
            let date = iso(today - chrono::Duration::days(days_ago));
            TimesheetEntry { start: format!("{date}T09:00:00Z"), date, duration, description: description.into(), customer: "Example Ltd".into() }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOGO: &str = "Images/logo.pdf";

    #[test]
    fn rejects_paths_outside_the_folder() {
        let dir = Path::new("/tmp/latex");
        assert!(resolve(dir, "Fonts/Inter/Inter-Bold.ttf").is_ok());
        for bad in ["", "../secret", "/etc/passwd", "Fonts/../../x", ".hidden"] {
            assert!(resolve(dir, bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn migrates_a_legacy_folder_once() {
        let legacy = tempfile::tempdir().unwrap();
        let files = tempfile::tempdir().unwrap();
        let l = legacy.path();
        std::fs::create_dir_all(l.join("Template/Images")).unwrap();
        std::fs::create_dir_all(l.join("Template/Fonts/Poppins")).unwrap();
        std::fs::create_dir_all(l.join("Dokumente/Fonts/Inter")).unwrap();
        std::fs::write(l.join("Template/Report.cls"), "Path = ../Template/Fonts/Poppins/ {../Template/Images/}").unwrap();
        std::fs::write(l.join("Template/Images/logo.pdf"), "pdf").unwrap();
        std::fs::write(l.join("Template/Fonts/Poppins/Poppins-Light.ttf"), "ttf").unwrap();
        std::fs::write(l.join("Dokumente/Fonts/Inter/Inter-Regular.ttf"), "ttf").unwrap();

        init(files.path(), Some(l.to_string_lossy().to_string())).unwrap();
        let dir = dir(files.path());
        assert_eq!(std::fs::read_to_string(dir.join("Report.cls")).unwrap(), "Path = Fonts/Poppins/ {Images/}");
        assert!(dir.join("Images/logo.pdf").is_file());
        assert!(dir.join("Fonts/Poppins/Poppins-Light.ttf").is_file());
        assert!(dir.join("Fonts/Inter/Inter-Regular.ttf").is_file());
        assert_eq!(std::fs::read_to_string(dir.join(INVOICE)).unwrap(), default_content(INVOICE).unwrap());

        std::fs::write(dir.join("Report.cls"), "edited").unwrap();
        init(files.path(), Some(l.to_string_lossy().to_string())).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("Report.cls")).unwrap(), "edited", "an existing folder is never overwritten");
    }

    #[test]
    fn a_fresh_folder_gets_the_logo_and_fonts_once() {
        let files = tempfile::tempdir().unwrap();
        init(files.path(), None).unwrap();
        for (rel, bytes) in DEFAULT_ASSETS {
            assert_eq!(std::fs::read(dir(files.path()).join(rel)).unwrap(), *bytes, "{rel}");
        }
        for font in ["Fonts/Inter/Inter-Regular.ttf", "Fonts/Poppins/Poppins-Light.ttf", "Fonts/JetBrainsMono/JetBrainsMono-Light.ttf"] {
            assert!(DEFAULT_ASSETS.iter().any(|(rel, _)| *rel == font), "{font} is shipped");
        }
        let logo = dir(files.path()).join(LOGO);
        assert!(std::fs::read(&logo).unwrap().starts_with(b"%PDF"));

        std::fs::remove_file(&logo).unwrap();
        init(files.path(), None).unwrap();
        assert!(!logo.exists(), "a deleted logo stays deleted");
    }

    #[test]
    fn a_legacy_logo_is_not_replaced() {
        let legacy = tempfile::tempdir().unwrap();
        let files = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(legacy.path().join("Template/Images")).unwrap();
        std::fs::write(legacy.path().join("Template/Images/logo.pdf"), "own").unwrap();
        init(files.path(), Some(legacy.path().to_string_lossy().to_string())).unwrap();
        assert_eq!(std::fs::read_to_string(dir(files.path()).join(LOGO)).unwrap(), "own");
    }

    #[test]
    fn classifies_files() {
        assert_eq!(kind_of("invoice.tex"), "text");
        assert_eq!(kind_of("Fonts/Inter/Inter-Bold.TTF"), "font");
        assert_eq!(kind_of("Images/logo.pdf"), "image");
        assert_eq!(kind_of("notes.docx"), "other");
    }

    #[test]
    #[ignore = "needs xelatex"]
    fn compiles_the_default_templates() {
        let files = tempfile::tempdir().unwrap();
        init(files.path(), std::env::var("MIDNIGHT_LEGACY_LATEX_DIR").ok()).unwrap();
        let tc = latex::toolchain(files.path()).unwrap();
        let s = Settings { company_name: "Example Studio".into(), company_owner: "Jane Doe".into(), company_iban: "DE00 0000".into(), ..Settings::default() };
        let out = files.path().join("out");
        latex::render_invoice(&tc, &sample_invoice(&s), None, &s, &out.join("invoice.pdf")).unwrap();
        let entries = sample_entries();
        latex::render_timesheet(&tc, &entries, "Preview", crate::util::parse_date(&entries[0].date).unwrap(), &s, &out.join("timesheet.pdf")).unwrap();
        let keep = std::env::var("MIDNIGHT_KEEP_PDFS").ok();
        if let Some(target) = keep {
            for f in ["invoice.pdf", "timesheet.pdf"] {
                std::fs::copy(out.join(f), Path::new(&target).join(f)).unwrap();
            }
        }
        assert!(pdf_extract::extract_text(out.join("invoice.pdf")).unwrap().contains("Example Ltd"));
    }
}
