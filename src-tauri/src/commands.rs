use serde_json::{json, Value};

use crate::i18n::tr;
use crate::activities::Activity;
use crate::categories::{Category, CategoryInput};
use crate::customers::Customer;
use crate::db::{AppState, Settings};
use crate::error::AppResult;
use crate::expenses::{Expense, ExpenseInput, Receipt, ReceiptDraft};
use crate::invoices::{Invoice, InvoiceFilter, InvoiceInput, InvoiceList, RenderedPdf};
use crate::units::Unit;
use crate::jobs::JobReport;
use crate::ledger::{LedgerFilter, LedgerPage};
use crate::products::Product;
use crate::expense_recurring::{ExpenseRecurring, ExpenseRecurringCreated, ExpenseRecurringInput};
use crate::reports::{Analysis, MonthRow, ShareRow, Summary};
use crate::search::SearchHit;
use crate::tax::{Params, TaxInput, TaxReport};
use crate::templates::LatexFolder;
use crate::tracker::{Entry, EntryInput, Project, ProjectInput, TrackerInvoiceDraft, WeeklyHours};
use crate::vault::{Document, DocumentUpdate, TagInfo};
use crate::forecast::Forecast;
use crate::places::Place;
use crate::mcp_setup::McpSetup;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Access {
    Read,
    Write,
    Delete,
    AppOnly,
}

macro_rules! call {
    (stateless; $state:expr; $f:path; $($arg:expr),*) => {{
        let _ = $state;
        $f($($arg),*).await
    }};
    (; $state:expr; $f:path; $($arg:expr),*) => {
        $f($state, $($arg),*).await
    };
}

macro_rules! access {
    (read) => {
        Access::Read
    };
    (write) => {
        Access::Write
    };
    (delete) => {
        Access::Delete
    };
    (app) => {
        Access::AppOnly
    };
}

fn inline_schema<T: schemars::JsonSchema>() -> Value {
    let schema = schemars::generate::SchemaSettings::draft2020_12()
        .with(|s| s.inline_subschemas = true)
        .into_generator()
        .into_root_schema_for::<T>();
    serde_json::to_value(schema).unwrap_or_else(|_| json!({ "type": "object" }))
}

fn unstring_json_args(input: Value) -> Value {
    match input {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(k, v)| {
                    let v = match &v {
                        Value::String(s) if s.trim_start().starts_with(['{', '[']) => serde_json::from_str(s).unwrap_or(v),
                        _ => v,
                    };
                    (k, v)
                })
                .collect(),
        ),
        other => other,
    }
}

macro_rules! commands {
    ($( $(#[$flag:ident])? $access:ident $module:ident::$name:ident($($arg:ident: $ty:ty),*) -> $ret:ty, $desc:literal; )*) => {
        $(
            #[tauri::command]
            pub async fn $name(state: tauri::State<'_, AppState>, $($arg: $ty),*) -> AppResult<$ret> {
                call!($($flag)?; &*state; crate::$module::$name; $($arg),*)
            }
        )*

        mod args {
            use super::*;
            $(
                #[allow(non_camel_case_types)]
                #[derive(serde::Deserialize, schemars::JsonSchema)]
                #[serde(deny_unknown_fields)]
                pub struct $name {
                    $(pub $arg: $ty,)*
                }
            )*
        }

        pub struct ToolSpec {
            pub name: &'static str,
            pub description: &'static str,
            pub access: Access,
            pub input_schema: fn() -> Value,
        }

        pub const TOOLS: &[ToolSpec] = &[
            $(ToolSpec {
                name: stringify!($name),
                description: $desc,
                access: access!($access),
                input_schema: || inline_schema::<args::$name>(),
            },)*
        ];

        pub async fn dispatch(state: &AppState, name: &str, input: Value) -> Result<Value, String> {
            match name {
                $(stringify!($name) if access!($access) != Access::AppOnly => {
                    let parsed: args::$name = match serde_json::from_value(input.clone()) {
                        Ok(parsed) => parsed,
                        Err(e) => serde_json::from_value(unstring_json_args(input)).map_err(|_| tr!("Invalid arguments: {e}", "Ungültige Argumente: {e}"))?,
                    };
                    let args::$name { $($arg),* } = parsed;
                    let result = call!($($flag)?; state; crate::$module::$name; $($arg),*).map_err(|e| e.to_string())?;
                    serde_json::to_value(result).map_err(|e| e.to_string())
                })*
                _ => Err(tr!("Unknown tool: {name}", "Unbekanntes Tool: {name}")),
            }
        }
    };
}

commands! {
    read settings::get_settings() -> Settings, "Reads all settings (company, invoices, limits and allowances, notifications, language).";
    write settings::save_settings(settings: Settings) -> Settings, "Saves the complete settings. Read get_settings first and change only the fields you need.";
    read settings::data_location() -> String, "Returns the app's data directory.";
    app settings::reset_all_data() -> (), "Deletes all data.";
    #[stateless] app mcp_setup::mcp_setup() -> McpSetup, "Shows how to connect the MCP server to Claude.";
    #[stateless] app mcp_setup::mcp_install_claude_desktop() -> (), "Registers the MCP server in Claude Desktop.";

    read ledger::list_ledger(filter: LedgerFilter) -> LedgerPage, "Lists transactions (income from paid invoices and expenses) with filters and totals.";
    write ledger::export_ledger_csv(filter: LedgerFilter, path: String) -> usize, "Exports filtered transactions as CSV to an absolute file path. Returns the number of rows.";

    read expenses::get_expense(id: String) -> Expense, "Reads an expense.";
    write expenses::save_expense(input: ExpenseInput) -> Expense, "Creates an expense (without id) or updates it (with id). Amounts in cents, dates as YYYY-MM-DD.";
    delete expenses::delete_expense(id: String) -> (), "Deletes an expense including its receipts.";
    read expenses::list_receipts(expense_id: String) -> Vec<Receipt>, "Lists the receipts of an expense.";
    write expenses::add_receipts(expense_id: String, paths: Vec<String>) -> usize, "Attaches local files (absolute paths) to an expense as receipts.";
    delete expenses::delete_receipt(id: String) -> (), "Deletes a receipt.";
    #[stateless] read expenses::extract_receipt(path: String) -> ReceiptDraft, "Extracts name, amount and date from a local receipt file (PDF/text) without saving anything.";

    read categories::list_categories() -> Vec<Category>, "Lists all expense categories.";
    write categories::save_category(input: CategoryInput) -> String, "Creates or updates a category. Returns the slug.";
    delete categories::delete_category(slug: String) -> (), "Deletes a category.";

    read customers::list_customers(search: Option<String>, include_archived: Option<bool>) -> Vec<Customer>, "Lists customers, optionally filtered.";
    write customers::save_customer(customer: Customer) -> Customer, "Creates a customer (without id) or updates it (with id). country is an ISO 3166 code from list_countries; empty means the company's own country.";
    delete customers::delete_customer(id: String) -> (), "Deletes a customer.";

    read tracker::list_projects(include_completed: Option<bool>) -> Vec<Project>, "Lists time tracking projects.";
    write tracker::save_project(project: ProjectInput) -> String, "Creates or updates a project. start_date/end_date (YYYY-MM-DD) are the approximate duration used by the tax forecast; once the project is completed they are replaced by the first and last tracked day. Returns the id.";
    delete tracker::delete_project(id: String) -> (), "Deletes a project including its time entries.";
    read tracker::list_entries(project_id: Option<String>, from: Option<String>, to: Option<String>) -> Vec<Entry>, "Lists time entries, optionally by project and period (YYYY-MM-DD).";
    read tracker::running_timer() -> Option<Entry>, "Returns the running timer, if any.";
    read tracker::weekly_hours() -> WeeklyHours, "Returns the hours tracked in the current week (closed entries, in seconds) and the weekly limit from the settings (limit_hours, 0 = no limit), plus all past weeks that exceeded the limit (past_exceeded, newest first).";
    write tracker::start_timer(project_id: String, description: Option<String>) -> Entry, "Starts a timer for a project.";
    write tracker::stop_timer() -> Option<String>, "Stops the running timer.";
    write tracker::save_entry(entry: EntryInput) -> String, "Creates or updates a time entry.";
    write tracker::set_home_office(date: String, applies: bool) -> (), "Sets whether the home office allowance applies on a day with tracked time (YYYY-MM-DD, default: it applies). The tax return counts these days automatically.";
    delete tracker::delete_entry(id: String) -> (), "Deletes a time entry.";
    read tracker::invoice_lines_from_tracker(project_id: String) -> TrackerInvoiceDraft, "Builds invoice line items from a project's unbilled time (saves nothing).";
    write tracker::invoice_unbilled_for_customer(customer_id: String) -> Invoice, "Creates a draft invoice from all of a customer's unbilled time.";
    write tracker::export_timesheet(from: String, to: String, customer_id: Option<String>, target: String) -> usize, "Exports a timesheet to an absolute file path.";

    read tax::get_tax_report(year: i32) -> TaxReport, "Calculates the tax return (income tax estimate) for a year, including saved inputs.";
    #[stateless] read tax::default_tax_params(year: i32) -> Params, "Returns the built-in tax allowances for a year (without overrides from the settings).";
    #[stateless] read tax::tax_years() -> Vec<i32>, "Lists the years with built-in tax parameters, newest first.";
    write tax::save_tax_input(year: i32, input: TaxInput) -> TaxReport, "Saves the complete manual tax inputs for a year. Read get_tax_report first and adjust its input.";

    read invoices::list_invoices(filter: InvoiceFilter) -> InvoiceList, "Lists invoices with filters and totals per status.";
    read invoices::get_invoice(id: String) -> Invoice, "Reads an invoice.";
    read invoices::next_invoice_number(issue_date: Option<String>) -> String, "Returns the next free invoice number.";
    write invoices::save_invoice(input: InvoiceInput) -> Invoice, "Creates an invoice (without id) or updates it (with id). Amounts in cents; line item units are codes from list_units.";
    write invoices::set_invoice_status(id: String, status: String, paid_at: Option<String>) -> Invoice, "Sets an invoice's status (draft, unpaid, paid, canceled); paid_at as YYYY-MM-DD.";
    delete invoices::delete_invoice(id: String) -> (), "Deletes an invoice.";
    write invoices::duplicate_invoice(id: String) -> Invoice, "Duplicates an invoice as a new draft.";
    #[stateless] read units::list_units() -> Vec<Unit>, "Lists the allowed line item units (UN/ECE Recommendation 20 codes such as HUR for hours) with their labels.";
    write invoices::render_invoice_pdf(id: String) -> RenderedPdf, "Renders an invoice's PDF with LaTeX (requires xelatex) and returns the file path, plus a warning if the e-invoice could not be embedded.";
    write invoices::render_invoice_timesheet(id: String) -> Option<String>, "Renders the timesheet for an invoice if time entries are linked.";
    write invoices::export_invoice_pdf(id: String, target: String) -> Option<String>, "Renders the invoice PDF with LaTeX (requires xelatex) and saves it to an absolute file path. With e-invoicing enabled in the settings, the PDF is a ZUGFeRD/Factur-X hybrid (EN 16931); returns a warning if the e-invoice could not be embedded.";
    write invoices::export_invoice_xrechnung(id: String, target: String) -> (), "Saves the invoice as an XRechnung 3.0 e-invoice (CII XML) to an absolute file path.";

    read templates::latex_files() -> LatexFolder, "Lists the LaTeX template folder (invoice.tex, timesheet.tex, Report.cls, fonts, images) with the placeholders each template supports.";
    read templates::latex_read(path: String) -> String, "Reads a text file (.tex/.cls/.sty) from the LaTeX template folder; path is relative, e.g. invoice.tex.";
    write templates::latex_write(path: String, content: String) -> (), "Writes a text file (.tex/.cls/.sty) in the LaTeX template folder. Read it first and write it back in full.";
    write templates::latex_reset(path: String) -> String, "Restores a built-in template (invoice.tex, timesheet.tex, Report.cls) and returns its content.";
    write templates::latex_import(paths: Vec<String>) -> usize, "Copies local files (absolute paths) into the LaTeX template folder: fonts to Fonts/<family>/, images to Images/, .tex/.cls/.sty to the top level.";
    delete templates::latex_delete(path: String) -> (), "Deletes a file or folder (e.g. a font family) from the LaTeX template folder.";
    write templates::latex_preview(template: String) -> String, "Compiles invoice.tex or timesheet.tex with sample data (Max Mustermann as sender, an example customer) and returns the PDF path.";

    read products::list_products(search: Option<String>) -> Vec<Product>, "Lists saved products and services.";
    write products::save_product(product: Product) -> String, "Creates or updates a product. unit is a code from list_units.";
    write products::touch_product(id: String) -> (), "Increments a product's usage counter.";
    delete products::delete_product(id: String) -> (), "Deletes a product.";

    read expense_recurring::list_expense_recurring() -> Vec<ExpenseRecurring>, "Lists recurring expenses (series).";
    write expense_recurring::save_expense_recurring(input: ExpenseRecurringInput) -> ExpenseRecurringCreated, "Turns an existing expense into a recurring series; it is the first expense and the template. Following expenses are booked automatically on their dates; past dates are booked right away (returned as caught_up).";
    write expense_recurring::set_expense_recurring_status(id: String, status: String) -> (), "Sets the status of a recurring expense (active, paused, canceled).";
    delete expense_recurring::delete_expense_recurring(id: String) -> (), "Deletes a recurring expense series; expenses already booked are kept.";


    write vault::vault_upload(paths: Vec<String>) -> usize, "Adds local files (absolute paths) to the document vault.";
    read vault::list_documents(search: Option<String>, tag: Option<String>) -> Vec<Document>, "Searches the document vault (full text).";
    write vault::update_document(update: DocumentUpdate) -> (), "Changes a document's title, tags, type or customer.";
    write vault::rename_document(id: String, name: String) -> String, "Renames a document's stored file on disk (the extension is kept if omitted) and returns the new file name.";
    delete vault::delete_document(id: String) -> (), "Deletes a document.";
    read vault::list_tags() -> Vec<TagInfo>, "Lists all document tags with counts.";

    read forecast::tax_forecast(year: i32) -> Forecast, "Tax forecast for a year: expected payments (open invoices, unbilled time, project budgets), recurring expenses, expected tax at year end and threshold warnings.";
    #[stateless] read places::list_countries() -> Vec<&'static str>, "Lists the allowed ISO 3166-1 country codes for customers and the company address.";
    #[stateless] read places::suggest_places(query: String, country: Option<String>) -> Vec<Place>, "Offline lookup of postal codes and towns in Germany, Austria and Switzerland (postal code prefix or town name).";

    read reports::report_summary(from: String, to: String) -> Summary, "Income, expenses and profit for a period (YYYY-MM-DD).";
    read reports::report_monthly(from: String, to: String) -> Vec<MonthRow>, "Monthly figures for a period (YYYY-MM-DD).";
    read reports::report_categories(from: String, to: String, kind: Option<String>) -> Vec<ShareRow>, "Shares by category for a period.";
    read reports::report_analysis(from: String, to: String) -> Analysis, "Analysis for a period (YYYY-MM-DD): revenue, tracked hours, payment days and open amounts per customer, expenses per category against the previous period, hours per project and monthly fixed costs from active recurring expenses.";

    read search::global_search(query: String) -> Vec<SearchHit>, "Searches expenses, invoices, customers, projects and documents.";

    read activities::list_activities() -> Vec<Activity>, "Lists the most recent notifications.";
    write activities::mark_activities_read() -> (), "Marks all notifications as read.";
    delete activities::clear_activities() -> (), "Deletes all notifications.";

    write jobs::run_jobs_now() -> JobReport, "Runs the background jobs now (marks overdue invoices and books due recurring expenses).";
}
