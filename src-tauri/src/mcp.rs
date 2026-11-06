use std::io::{BufRead, Write};
use std::path::PathBuf;

use rusqlite::{params, OptionalExtension};
use serde_json::{json, Map, Value};

use crate::commands::{dispatch, unstring_json_args, Access, ToolSpec, TOOLS};
use crate::db::{self, AppState};
use crate::error::{msg, AppResult};
use crate::i18n::{self, tr};
use crate::security::{load_mcp_access, McpAccess};

const PROTOCOL_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

const INSTRUCTIONS: &str = "Midnight is a local bookkeeping app for freelancers and the self-employed in Germany. \
Money amounts are always whole cents (€12.50 = 1250), dates are YYYY-MM-DD. \
save_* tools create a record without id and update it with id; before changing a record, always read it and write it back in full. \
Updates with missing fields are rejected. Only draft invoices can be changed or deleted. \
File paths must be absolute. Resetting all data is only possible inside the app.";

const READ_ONLY_NOTE: &str = " Access is read-only: the user turned off changes in Midnight's settings.";
const PRIVATE_NOTE: &str = " Customer data is hidden by the user's privacy settings: customer names, addresses and contact details are null, and tools that only return or export customer data are unavailable.";

const PRIVATE_TOOLS: &[&str] = &[
    "list_customers",
    "save_customer",
    "delete_customer",
    "list_activities",
    "export_ledger_csv",
    "export_timesheet",
    "export_tax_year",
    "render_invoice_pdf",
    "render_invoice_timesheet",
    "export_invoice_pdf",
    "export_invoice_xrechnung",
    "latex_write",
    "latex_import",
    "latex_reset",
    "latex_delete",
    "vault_upload",
    "add_receipts",
];

const CUSTOMER_FIELDS: &[&str] = &["customer", "customer_name", "customer_details", "customer_number", "largest_customer"];

struct FullWrite {
    tool: &'static str,
    arg: &'static str,
    key: Option<&'static str>,
    optional: &'static [&'static str],
}

const FULL_WRITES: &[FullWrite] = &[
    FullWrite { tool: "save_settings", arg: "settings", key: None, optional: &[] },
    FullWrite { tool: "save_tax_input", arg: "input", key: None, optional: &[] },
    FullWrite { tool: "update_document", arg: "update", key: None, optional: &[] },
    FullWrite { tool: "save_customer", arg: "customer", key: Some("id"), optional: &["customer_number", "invoice_count", "paid_cents", "open_cents", "tracked_seconds"] },
    FullWrite { tool: "save_project", arg: "project", key: Some("id"), optional: &[] },
    FullWrite { tool: "save_entry", arg: "entry", key: Some("id"), optional: &[] },
    FullWrite { tool: "save_expense", arg: "input", key: Some("id"), optional: &["deductible_percent"] },
    FullWrite { tool: "save_invoice", arg: "input", key: Some("id"), optional: &["invoice_number", "status", "tracker_entry_ids"] },
    FullWrite { tool: "save_category", arg: "input", key: Some("slug"), optional: &[] },
    FullWrite { tool: "save_product", arg: "product", key: Some("id"), optional: &["usage_count"] },
];

fn data_dir() -> AppResult<PathBuf> {
    if let Some(dir) = std::env::var_os("MIDNIGHT_DATA_DIR") {
        return Ok(PathBuf::from(dir));
    }
    let config: Value = serde_json::from_str(include_str!("../tauri.conf.json"))?;
    let identifier = config["identifier"].as_str().ok_or_else(|| msg(tr!("identifier missing in tauri.conf.json", "identifier fehlt in tauri.conf.json")))?;
    let base = if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
    } else if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
    };
    Ok(base.ok_or_else(|| msg(tr!("Cannot determine the data directory, set MIDNIGHT_DATA_DIR", "Datenverzeichnis nicht ermittelbar, MIDNIGHT_DATA_DIR setzen")))?.join(identifier))
}

fn open_state() -> AppResult<AppState> {
    let dir = data_dir()?;
    let db_path = dir.join("midnight.db");
    if !db_path.exists() {
        return Err(msg(tr!("No Midnight database found at {}. Please start the app once.", "Keine Midnight-Datenbank unter {} gefunden. Bitte die App einmal starten.", db_path.display())));
    }
    let conn = db::open(&db_path)?;
    i18n::set_language(&db::load_settings(&conn)?.language);
    db::localize_default_categories(&conn)?;
    crate::templates::init(&dir.join("files"), db::legacy_latex_dir(&conn)?)?;
    AppState::new(conn, dir.join("files"))
}

fn access_of(state: &AppState) -> McpAccess {
    state.conn().ok().and_then(|conn| load_mcp_access(&conn).ok()).unwrap_or(McpAccess { read_only: true, private: true })
}

fn refusal(access: McpAccess, tool: &ToolSpec) -> Option<String> {
    if tool.access == Access::AppOnly {
        return None;
    }
    if access.read_only && tool.access != Access::Read {
        return Some(tr!(
            "Midnight only allows read-only access over MCP. The user can allow changes under Settings → General → MCP server.",
            "Midnight erlaubt über MCP nur Lesezugriff. Änderungen lassen sich unter Einstellungen → Allgemein → MCP-Server erlauben."
        ));
    }
    if access.private && PRIVATE_TOOLS.contains(&tool.name) {
        return Some(tr!(
            "Customer data is protected. The user can turn this off under Settings → General → MCP server.",
            "Kundendaten sind geschützt. Das lässt sich unter Einstellungen → Allgemein → MCP-Server ausschalten."
        ));
    }
    None
}

fn hide_customer_fields(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, field) in map.iter_mut() {
                if CUSTOMER_FIELDS.contains(&key.as_str()) {
                    *field = Value::Null;
                } else {
                    hide_customer_fields(field);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(hide_customer_fields),
        _ => {}
    }
}

fn private_args(name: &str, args: &mut Value) {
    match name {
        "list_invoices" | "list_ledger" => {
            if let Some(filter) = args.get_mut("filter").and_then(Value::as_object_mut) {
                filter.remove("search");
            }
        }
        "list_documents" => {
            if let Some(args) = args.as_object_mut() {
                args.insert("search".into(), Value::Null);
            }
        }
        _ => {}
    }
}

fn redact(name: &str, args: &Value, result: &mut Value) {
    match name {
        "list_ledger" => {
            for item in result["items"].as_array_mut().into_iter().flatten().filter(|i| i["kind"] == "income") {
                item["name"] = Value::Null;
            }
        }
        "report_analysis" => {
            for stat in result["customers"].as_array_mut().into_iter().flatten() {
                stat["key"] = stat["customer_id"].clone();
                stat["name"] = Value::Null;
            }
        }
        "global_search" => {
            let query = args["query"].as_str().unwrap_or_default().trim().to_lowercase();
            if let Some(hits) = result.as_array_mut() {
                hits.retain(|h| match h["kind"].as_str() {
                    Some("expense" | "project") => true,
                    Some("invoice") => h["title"].as_str().is_some_and(|number| number.to_lowercase().contains(&query)),
                    _ => false,
                });
                for hit in hits.iter_mut().filter(|h| h["kind"] != "expense") {
                    hit["subtitle"] = Value::Null;
                }
            }
        }
        "list_documents" => {
            if let Some(docs) = result.as_array_mut() {
                docs.retain(|d| d["customer_id"].is_null() && d["invoice_id"].is_null());
                for doc in docs.iter_mut() {
                    doc["snippet"] = Value::Null;
                }
            }
        }
        _ => {}
    }
    hide_customer_fields(result);
}

fn customer_terms(state: &AppState) -> AppResult<Vec<String>> {
    let conn = state.conn()?;
    let mut stmt = conn.prepare(
        "SELECT name FROM customers UNION SELECT contact FROM customers UNION SELECT email FROM customers
         UNION SELECT billing_email FROM customers UNION SELECT phone FROM customers UNION SELECT address_line1 FROM customers
         UNION SELECT address_line2 FROM customers UNION SELECT vat_number FROM customers
         UNION SELECT website FROM customers UNION SELECT leitweg_id FROM customers
         UNION SELECT name FROM customer_contacts UNION SELECT email FROM customer_contacts UNION SELECT phone FROM customer_contacts
         UNION SELECT customer_name FROM invoices",
    )?;
    let mut terms: Vec<String> = stmt
        .query_map([], |r| r.get::<_, Option<String>>(0))?
        .filter_map(|t| t.ok().flatten())
        .map(|t| t.trim().to_string())
        .filter(|t| t.chars().count() >= 3)
        .collect();
    terms.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    terms.dedup();
    Ok(terms)
}

fn mask_terms(value: &mut Value, pattern: &regex::Regex, placeholder: &str) {
    match value {
        Value::String(text) if pattern.is_match(text) => *text = pattern.replace_all(text, placeholder).into_owned(),
        Value::Array(items) => items.iter_mut().for_each(|v| mask_terms(v, pattern, placeholder)),
        Value::Object(map) => map
            .iter_mut()
            .filter(|(key, _)| *key != "id" && !key.ends_with("_id"))
            .for_each(|(_, v)| mask_terms(v, pattern, placeholder)),
        _ => {}
    }
}

fn mask_customer_terms(state: &AppState, value: &mut Value) -> Result<(), String> {
    let terms = customer_terms(state).map_err(|e| e.to_string())?;
    if terms.is_empty() {
        return Ok(());
    }
    let alternation = terms.iter().map(|t| regex::escape(t)).collect::<Vec<_>>().join("|");
    let pattern = regex::Regex::new(&format!("(?i){alternation}")).map_err(|e| e.to_string())?;
    mask_terms(value, &pattern, &tr!("[customer]", "[Kunde]"));
    Ok(())
}

fn hide_customer_data(state: &AppState, name: &str, args: &Value, result: &mut Value) -> Result<(), String> {
    redact(name, args, result);
    mask_customer_terms(state, result)
}

fn tool_list(access: McpAccess) -> Value {
    let tools: Vec<Value> = TOOLS
        .iter()
        .filter(|t| t.access != Access::AppOnly && refusal(access, t).is_none())
        .map(|t| {
            let mut schema = (t.input_schema)();
            if let Some(obj) = schema.as_object_mut() {
                obj.remove("$schema");
                obj.remove("title");
            }
            json!({
                "name": t.name,
                "description": t.description,
                "inputSchema": schema,
                "annotations": {
                    "readOnlyHint": t.access == Access::Read,
                    "destructiveHint": matches!(t.access, Access::Write | Access::Delete),
                    "openWorldHint": false,
                },
            })
        })
        .collect();
    json!({ "tools": tools })
}

fn locked_invoice(state: &AppState, name: &str, args: &Value) -> Option<String> {
    let id = match name {
        "delete_invoice" => args["id"].as_str()?,
        "save_invoice" => args["input"]["id"].as_str()?,
        _ => return None,
    };
    let conn = match state.conn() {
        Ok(conn) => conn,
        Err(e) => return Some(e.to_string()),
    };
    let row = conn
        .query_row("SELECT invoice_number, status FROM invoices WHERE id = ?1", params![id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .optional();
    let (number, status) = match row {
        Ok(Some(row)) => row,
        Ok(None) => return None,
        Err(e) => return Some(e.to_string()),
    };
    (status != "draft").then(|| {
        tr!(
            "Invoice {number} has the status {status}. Only drafts can be changed or deleted here. Ask the user, then set it back to draft with set_invoice_status.",
            "Rechnung {number} hat den Status {status}. Hier lassen sich nur Entwürfe ändern oder löschen. Frag den Nutzer und setze sie dann mit set_invoice_status zurück auf draft."
        )
    })
}

fn missing_fields(schema: &Value, record: &Map<String, Value>, optional: &[&str]) -> Vec<String> {
    let Some(fields) = schema["properties"].as_object() else { return Vec::new() };
    fields
        .iter()
        .filter(|(field, _)| !optional.contains(&field.as_str()))
        .flat_map(|(field, nested)| match record.get(field) {
            None => vec![field.clone()],
            Some(Value::Object(inner)) => missing_fields(nested, inner, &[]).into_iter().map(|f| format!("{field}.{f}")).collect(),
            Some(_) => Vec::new(),
        })
        .collect()
}

fn incomplete_update(name: &str, args: &Value) -> Option<String> {
    let rule = FULL_WRITES.iter().find(|r| r.tool == name)?;
    let record = args[rule.arg].as_object()?;
    if rule.key.is_some_and(|key| record.get(key).is_none_or(Value::is_null)) {
        return None;
    }
    let schema = (TOOLS.iter().find(|t| t.name == name)?.input_schema)();
    let missing = missing_fields(&schema["properties"][rule.arg], record, rule.optional).join(", ");
    (!missing.is_empty()).then(|| {
        tr!(
            "Incomplete update, missing fields: {missing}. Read the record first and send every field, null clears one.",
            "Unvollständiges Update, es fehlen: {missing}. Den Datensatz zuerst lesen und alle Felder mitschicken, null leert ein Feld."
        )
    })
}

async fn call_tool(state: &AppState, params: &Value) -> Value {
    let name = params["name"].as_str().unwrap_or_default();
    let args = match &params["arguments"] {
        Value::Null => json!({}),
        other => other.clone(),
    };
    let mut parsed = unstring_json_args(args.clone());
    let access = access_of(state);
    let args = if access.private {
        private_args(name, &mut parsed);
        parsed.clone()
    } else {
        args
    };
    let refused = TOOLS
        .iter()
        .find(|t| t.name == name)
        .and_then(|t| refusal(access, t))
        .or_else(|| locked_invoice(state, name, &parsed))
        .or_else(|| incomplete_update(name, &parsed));
    let outcome = match refused {
        Some(reason) => Err(reason),
        None => match dispatch(state, name, args).await {
            Ok(mut result) if access.private => hide_customer_data(state, name, &parsed, &mut result).map(|_| result),
            Err(reason) if access.private => {
                let mut text = Value::String(reason);
                Err(match mask_customer_terms(state, &mut text) {
                    Ok(()) => text.as_str().unwrap_or_default().to_string(),
                    Err(e) => e,
                })
            }
            other => other,
        },
    };
    match outcome {
        Ok(result) => json!({
            "content": [{ "type": "text", "text": serde_json::to_string(&result).unwrap_or_default() }],
            "isError": false,
        }),
        Err(e) => json!({
            "content": [{ "type": "text", "text": e }],
            "isError": true,
        }),
    }
}

fn instructions(access: McpAccess) -> String {
    let mut text = INSTRUCTIONS.to_string();
    if access.read_only {
        text.push_str(READ_ONLY_NOTE);
    }
    if access.private {
        text.push_str(PRIVATE_NOTE);
    }
    text
}

pub async fn handle(state: &AppState, request: &Value) -> Option<Value> {
    let id = request.get("id")?.clone();
    let params = &request["params"];
    let result = match request["method"].as_str().unwrap_or_default() {
        "initialize" => {
            let requested = params["protocolVersion"].as_str().unwrap_or_default();
            let version = PROTOCOL_VERSIONS.iter().find(|v| **v == requested).unwrap_or(&PROTOCOL_VERSIONS[0]);
            Ok(json!({
                "protocolVersion": version,
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "midnight", "version": env!("CARGO_PKG_VERSION") },
                "instructions": instructions(access_of(state)),
            }))
        }
        "ping" => Ok(json!({})),
        "tools/list" => Ok(tool_list(access_of(state))),
        "tools/call" => Ok(call_tool(state, params).await),
        other => Err(json!({ "code": -32601, "message": tr!("Method not supported: {other}", "Methode nicht unterstützt: {other}") })),
    };
    Some(match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err(error) => json!({ "jsonrpc": "2.0", "id": id, "error": error }),
    })
}

pub fn serve_stdio() -> AppResult<()> {
    let state = open_state()?;
    let runtime = tokio::runtime::Builder::new_current_thread().build()?;
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<Value>(&line) {
            Ok(request) => runtime.block_on(handle(&state, &request)),
            Err(e) => Some(json!({ "jsonrpc": "2.0", "id": null, "error": { "code": -32700, "message": e.to_string() } })),
        };
        if let Some(response) = response {
            writeln!(stdout, "{response}")?;
            stdout.flush()?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> (tempfile::TempDir, AppState) {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::new(db::open(&dir.path().join("midnight.db")).unwrap(), dir.path().join("files")).unwrap();
        (dir, state)
    }

    fn request(method: &str, params: Value) -> Value {
        json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params })
    }

    fn run(state: &AppState, req: Value) -> Option<Value> {
        tokio::runtime::Builder::new_current_thread().build().unwrap().block_on(handle(state, &req))
    }

    fn call(state: &AppState, name: &str, arguments: Value) -> Value {
        run(state, request("tools/call", json!({ "name": name, "arguments": arguments }))).unwrap()["result"].clone()
    }

    fn call_json(state: &AppState, name: &str, arguments: Value) -> Value {
        let r = call(state, name, arguments);
        assert_eq!(r["isError"], false, "{name}: {}", r["content"][0]["text"]);
        serde_json::from_str(r["content"][0]["text"].as_str().unwrap()).unwrap()
    }

    #[test]
    fn initialize_negotiates_version() {
        let (_dir, s) = state();
        let r = run(&s, request("initialize", json!({ "protocolVersion": "2025-03-26" }))).unwrap();
        assert_eq!(r["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(r["result"]["serverInfo"]["name"], "midnight");
        let r = run(&s, request("initialize", json!({ "protocolVersion": "1999-01-01" }))).unwrap();
        assert_eq!(r["result"]["protocolVersion"], PROTOCOL_VERSIONS[0]);
    }

    #[test]
    fn notifications_get_no_response() {
        let (_dir, s) = state();
        assert!(run(&s, json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).is_none());
    }

    #[test]
    fn lists_tools_with_object_schemas_and_hides_app_only() {
        let (_dir, s) = state();
        let tools = run(&s, request("tools/list", json!({}))).unwrap()["result"]["tools"].as_array().unwrap().clone();
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        for name in ["rename_document", "tax_forecast", "suggest_places"] {
            assert!(names.contains(&name), "{name} is exposed");
        }
        assert!(names.contains(&"list_customers") && names.contains(&"save_invoice"));
        assert!(!names.contains(&"reset_all_data"), "reset_all_data must not be exposed");
        for t in &tools {
            assert_eq!(t["inputSchema"]["type"], "object", "{}", t["name"]);
        }
        let delete = tools.iter().find(|t| t["name"] == "delete_customer").unwrap();
        assert_eq!(delete["annotations"]["destructiveHint"], true);
        let read = tools.iter().find(|t| t["name"] == "list_categories").unwrap();
        assert_eq!(read["annotations"]["readOnlyHint"], true);
        assert_eq!(read["annotations"]["destructiveHint"], false);
    }

    #[test]
    fn only_additive_tools_claim_to_be_non_destructive() {
        let (_dir, s) = state();
        let tools = run(&s, request("tools/list", json!({}))).unwrap()["result"]["tools"].as_array().unwrap().clone();
        let hint = |name: &str| tools.iter().find(|t| t["name"] == name).unwrap()["annotations"]["destructiveHint"].clone();
        for name in ["add_receipts", "vault_upload", "duplicate_invoice"] {
            assert_eq!(hint(name), false, "{name} only adds");
        }
        for name in ["save_invoice", "save_settings", "latex_write", "latex_import", "export_ledger_csv", "start_timer", "delete_customer"] {
            assert_eq!(hint(name), true, "{name} can overwrite or remove");
        }
    }

    #[test]
    fn tool_results_are_compact_json() {
        let (_dir, s) = state();
        let r = call(&s, "get_settings", json!({}));
        assert!(!r["content"][0]["text"].as_str().unwrap().contains('\n'));
    }

    fn line_item() -> Value {
        json!({ "name": "Beratung", "quantity": 1.0, "price_cents": 10000 })
    }

    fn draft_invoice(s: &AppState) -> Value {
        call_json(s, "save_invoice", json!({ "input": {
            "issue_date": "2026-10-01", "due_date": "2026-10-15", "currency": "EUR",
            "line_items": [line_item()], "vat_rate": 19.0, "note": "Danke",
        } }))
    }

    fn assert_rejected(s: &AppState, name: &str, arguments: Value, mentions: &str) {
        let r = call(s, name, arguments);
        assert_eq!(r["isError"], true, "{name} must be rejected");
        let text = r["content"][0]["text"].as_str().unwrap();
        assert!(text.contains(mentions), "{name}: {text}");
    }

    #[test]
    fn incomplete_updates_are_rejected_and_change_nothing() {
        let (_dir, s) = state();

        let customer = call_json(&s, "save_customer", json!({ "customer": { "name": "Test GmbH", "city": "Berlin" } }));
        assert_rejected(&s, "save_customer", json!({ "customer": { "id": customer["id"], "name": "Neu" } }), "city");
        let stringified = json!({ "id": customer["id"], "name": "Neu" }).to_string();
        assert_rejected(&s, "save_customer", json!({ "customer": stringified }), "city");
        let stored = call_json(&s, "list_customers", json!({}))[0].clone();
        assert_eq!(stored["name"], "Test GmbH");
        assert_eq!(stored["city"], "Berlin");

        let invoice = draft_invoice(&s);
        assert_rejected(&s, "save_invoice", json!({ "input": {
            "id": invoice["id"], "issue_date": "2026-10-02", "due_date": "2026-10-16", "line_items": [line_item()],
        } }), "vat_rate");
        let stored = call_json(&s, "get_invoice", json!({ "id": invoice["id"] }));
        assert_eq!(stored["vat_rate"], 19.0);
        assert_eq!(stored["note"], "Danke");

        let mut settings = call_json(&s, "get_settings", json!({}));
        settings["company_name"] = json!("Firma");
        call_json(&s, "save_settings", json!({ "settings": settings }));
        assert_rejected(&s, "save_settings", json!({ "settings": { "language": "de" } }), "company_name");
        assert_eq!(call_json(&s, "get_settings", json!({}))["company_name"], "Firma");

        let mut limits_changed = call_json(&s, "get_settings", json!({}));
        let limits = limits_changed["limits"].clone();
        limits_changed["limits"] = json!({});
        assert_rejected(&s, "save_settings", json!({ "settings": limits_changed }), "limits.");
        assert_eq!(call_json(&s, "get_settings", json!({}))["limits"], limits);
    }

    #[test]
    fn fields_that_keep_their_value_may_be_omitted() {
        let (_dir, s) = state();
        let expense = call_json(&s, "save_expense", json!({ "input": {
            "date": "2026-10-01", "name": "Essen", "amount_cents": 5000, "deductible_percent": 70,
        } }));
        let mut update = expense.clone();
        update.as_object_mut().unwrap().remove("deductible_percent");
        assert_eq!(call_json(&s, "save_expense", json!({ "input": update }))["deductible_percent"], 70);

        let invoice = draft_invoice(&s);
        let mut update = call_json(&s, "get_invoice", json!({ "id": invoice["id"] }));
        for field in ["invoice_number", "status"] {
            update.as_object_mut().unwrap().remove(field);
        }
        let saved = call_json(&s, "save_invoice", json!({ "input": update }));
        assert_eq!(saved["invoice_number"], invoice["invoice_number"]);
        assert_eq!(saved["status"], "draft");

        let customer = call_json(&s, "save_customer", json!({ "customer": { "name": "Test GmbH" } }));
        let mut update = customer.clone();
        update.as_object_mut().unwrap().remove("customer_number");
        assert_eq!(call_json(&s, "save_customer", json!({ "customer": update }))["customer_number"], customer["customer_number"]);
    }

    #[test]
    fn explicit_null_still_clears_a_field() {
        let (_dir, s) = state();
        let mut customer = call_json(&s, "save_customer", json!({ "customer": { "name": "Test GmbH", "city": "Berlin" } }));
        customer["city"] = Value::Null;
        call_json(&s, "save_customer", json!({ "customer": customer }));
        assert_eq!(call_json(&s, "list_customers", json!({}))[0]["city"], Value::Null);
    }

    #[test]
    fn new_records_do_not_need_every_field() {
        let (_dir, s) = state();
        call_json(&s, "save_customer", json!({ "customer": { "name": "Test GmbH" } }));
        call_json(&s, "save_category", json!({ "input": { "name": "Neu", "color": "#112233" } }));
        call_json(&s, "save_expense", json!({ "input": { "date": "2026-10-01", "name": "Figma", "amount_cents": 1500 } }));
    }

    #[test]
    fn records_can_be_read_and_written_back_unchanged() {
        let (dir, s) = state();

        call_json(&s, "save_customer", json!({ "customer": { "name": "Test GmbH" } }));
        let customer = call_json(&s, "list_customers", json!({}))[0].clone();
        call_json(&s, "save_customer", json!({ "customer": customer }));

        let project_id = call_json(&s, "save_project", json!({ "project": { "name": "Relaunch", "billable": true } }));
        let project = call_json(&s, "list_projects", json!({}))[0].clone();
        call_json(&s, "save_project", json!({ "project": project }));

        call_json(&s, "save_entry", json!({ "entry": {
            "project_id": project_id, "start": "2026-10-01T09:00:00Z", "stop": "2026-10-01T10:00:00Z",
        } }));
        let entry = call_json(&s, "list_entries", json!({}))[0].clone();
        call_json(&s, "save_entry", json!({ "entry": entry }));

        let expense = call_json(&s, "save_expense", json!({ "input": { "date": "2026-10-01", "name": "Figma", "amount_cents": 1500 } }));
        call_json(&s, "save_expense", json!({ "input": call_json(&s, "get_expense", json!({ "id": expense["id"] })) }));

        let invoice = draft_invoice(&s);
        call_json(&s, "save_invoice", json!({ "input": call_json(&s, "get_invoice", json!({ "id": invoice["id"] })) }));

        let category = call_json(&s, "list_categories", json!({}))[0].clone();
        call_json(&s, "save_category", json!({ "input": category }));

        call_json(&s, "save_product", json!({ "product": { "name": "Beratung", "price_cents": 10000, "currency": "EUR" } }));
        let product = call_json(&s, "list_products", json!({}))[0].clone();
        call_json(&s, "save_product", json!({ "product": product }));

        let report = call_json(&s, "get_tax_report", json!({ "year": 2026 }));
        call_json(&s, "save_tax_input", json!({ "year": 2026, "input": report["input"] }));

        let file = dir.path().join("notiz.txt");
        std::fs::write(&file, "Notiz").unwrap();
        call_json(&s, "vault_upload", json!({ "paths": [file] }));
        let document = call_json(&s, "list_documents", json!({}))[0].clone();
        call_json(&s, "update_document", json!({ "update": document }));
    }

    #[test]
    fn every_full_write_rule_matches_its_tool() {
        for rule in FULL_WRITES {
            let tool = TOOLS.iter().find(|t| t.name == rule.tool).unwrap_or_else(|| panic!("{} is a tool", rule.tool));
            let schema = (tool.input_schema)();
            let fields = schema["properties"][rule.arg]["properties"].as_object().unwrap_or_else(|| panic!("{} has fields", rule.tool));
            for field in rule.key.iter().chain(rule.optional) {
                assert!(fields.contains_key(*field), "{}: {field}", rule.tool);
            }
        }
        for tool in TOOLS.iter().filter(|t| t.name.starts_with("save_") && t.name != "save_expense_recurring" && t.access != Access::AppOnly) {
            assert!(FULL_WRITES.iter().any(|r| r.tool == tool.name), "{} needs a rule", tool.name);
        }
    }

    #[test]
    fn only_draft_invoices_can_be_changed_or_deleted() {
        let (_dir, s) = state();
        let invoice = draft_invoice(&s);
        let id = invoice["id"].clone();
        let number = invoice["invoice_number"].as_str().unwrap().to_string();
        let read = |s: &AppState| call_json(s, "get_invoice", json!({ "id": id }));
        call_json(&s, "save_invoice", json!({ "input": read(&s) }));

        for (status, paid_at) in [("unpaid", Value::Null), ("paid", json!("2026-10-05")), ("canceled", Value::Null)] {
            call_json(&s, "set_invoice_status", json!({ "id": id, "status": status, "paid_at": paid_at }));
            let mut changed = read(&s);
            changed["vat_rate"] = json!(0.0);
            assert_rejected(&s, "save_invoice", json!({ "input": changed }), &number);
            assert_rejected(&s, "delete_invoice", json!({ "id": id }), &number);
            assert_eq!(read(&s)["vat_rate"], 19.0, "{status}");
        }

        call_json(&s, "set_invoice_status", json!({ "id": id, "status": "draft" }));
        call_json(&s, "save_invoice", json!({ "input": read(&s) }));
        call_json(&s, "delete_invoice", json!({ "id": id }));
    }

    #[test]
    fn unknown_invoices_report_not_found_through_the_tool() {
        let (_dir, s) = state();
        assert_eq!(call(&s, "delete_invoice", json!({ "id": "missing" }))["isError"], true);
    }

    #[test]
    fn app_only_tools_cannot_be_called() {
        let (_dir, s) = state();
        assert_eq!(call(&s, "reset_all_data", json!({}))["isError"], true);
        assert_eq!(call(&s, "list_categories", json!({}))["isError"], false);
    }

    #[test]
    fn reads_and_writes_through_tools() {
        let (_dir, s) = state();
        assert_eq!(call_json(&s, "list_categories", json!({})).as_array().unwrap().len(), 13);

        let mut settings = call_json(&s, "get_settings", json!(null));
        settings["company_name"] = json!("Test Company");
        call_json(&s, "save_settings", json!({ "settings": settings }));
        assert_eq!(call_json(&s, "get_settings", json!({}))["company_name"], "Test Company");
    }

    #[test]
    fn rejects_unknown_arguments_and_tools() {
        let (_dir, s) = state();
        let r = call(&s, "list_categories", json!({ "bogus": 1 }));
        assert_eq!(r["isError"], true);
        assert!(r["content"][0]["text"].as_str().unwrap().contains("Invalid arguments"));
        assert_eq!(call(&s, "does_not_exist", json!({}))["isError"], true);
    }

    fn set_access(s: &AppState, read_only: bool, private: bool) {
        let access = crate::security::McpAccess { read_only, private };
        tokio::runtime::Builder::new_current_thread().build().unwrap().block_on(crate::security::save_mcp_access(s, access)).unwrap();
    }

    fn tool_names(s: &AppState) -> Vec<String> {
        let list = run(s, request("tools/list", json!({}))).unwrap();
        list["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap().to_string()).collect()
    }

    #[test]
    fn read_only_access_offers_and_runs_only_reads() {
        let (_dir, s) = state();
        set_access(&s, true, false);
        let names = tool_names(&s);
        assert!(names.contains(&"list_invoices".to_string()));
        for write in ["save_invoice", "delete_category", "add_receipts", "save_settings"] {
            assert!(!names.contains(&write.to_string()), "{write} is not offered");
        }
        assert_rejected(&s, "delete_category", json!({ "slug": "software" }), "read-only");
        assert_eq!(call_json(&s, "list_categories", json!({})).as_array().unwrap().len(), 13, "nothing was deleted");
        let init = run(&s, request("initialize", json!({}))).unwrap();
        assert!(init["result"]["instructions"].as_str().unwrap().contains("read-only"));
    }

    #[test]
    fn private_access_hides_customer_data() {
        let (_dir, s) = state();
        let customer = call_json(&s, "save_customer", json!({ "customer": {
            "name": "Acme GmbH", "email": "kontakt@acme.test", "address_line1": "Hauptstraße 1", "zip": "10115", "city": "Berlin",
        } }));
        let invoice = call_json(&s, "save_invoice", json!({ "input": {
            "customer_id": customer["id"], "issue_date": "2026-10-01", "due_date": "2026-10-15", "currency": "EUR",
            "line_items": [line_item()], "vat_rate": 19.0,
        } }));
        call_json(&s, "set_invoice_status", json!({ "id": invoice["id"], "status": "paid", "paid_at": "2026-10-05" }));
        call_json(&s, "save_project", json!({ "project": { "name": "Website", "customer_id": customer["id"], "billable": true } }));

        set_access(&s, false, true);
        let names = tool_names(&s);
        for hidden in ["list_customers", "save_customer", "list_activities", "export_invoice_pdf", "export_tax_year"] {
            assert!(!names.contains(&hidden.to_string()), "{hidden} is not offered");
        }
        assert_rejected(&s, "list_customers", json!({}), "Customer data");
        for (name, arguments) in [
            ("latex_write", json!({ "path": "Report.cls", "content": "x" })),
            ("vault_upload", json!({ "paths": ["/tmp/x.pdf"] })),
            ("add_receipts", json!({ "expense_id": "e", "paths": ["/tmp/x.pdf"] })),
        ] {
            assert_rejected(&s, name, arguments, "Customer data");
        }
        let probe = call_json(&s, "list_invoices", json!({ "filter": { "search": "Zzz" } }));
        assert_eq!(probe["items"].as_array().map(Vec::len), Some(1), "search is ignored, it cannot probe names: {probe}");
        let hits = call_json(&s, "global_search", json!({ "query": "Acme" }));
        assert_eq!(hits, json!([]), "a customer name finds nothing");

        set_access(&s, false, false);
        let id = invoice["id"].as_str().unwrap().to_string();
        call_json(&s, "save_customer", json!({ "customer": { "name": &id[..8] } }));
        set_access(&s, false, true);
        assert_eq!(call_json(&s, "get_invoice", json!({ "id": id }))["id"], json!(id), "ids are never masked");

        let reads = [
            ("get_invoice", json!({ "id": invoice["id"] })),
            ("list_invoices", json!({ "filter": {} })),
            ("list_ledger", json!({ "filter": {} })),
            ("list_projects", json!({ "include_completed": true })),
            ("global_search", json!({ "query": "Acme" })),
            ("report_analysis", json!({ "from": "2026-01-01", "to": "2026-12-31" })),
            ("get_tax_report", json!({ "year": 2026 })),
            ("tax_forecast", json!({ "year": 2026 })),
            ("list_documents", json!({ "search": null, "tag": null })),
            ("list_documents", json!({ "search": "Acme", "tag": null })),
        ];
        for (name, arguments) in reads {
            let r = call(&s, name, arguments);
            let text = r["content"][0]["text"].as_str().unwrap();
            assert_eq!(r["isError"], false, "{name}: {text}");
            for secret in ["Acme", "acme.test", "Hauptstraße", "Berlin"] {
                assert!(!text.contains(secret), "{name} leaks {secret}: {text}");
            }
        }
        let read = call_json(&s, "get_invoice", json!({ "id": invoice["id"] }));
        assert_eq!((read["customer_id"].clone(), read["amount_cents"].clone()), (customer["id"].clone(), json!(11900)), "ids and amounts stay");
        assert!(run(&s, request("initialize", json!({}))).unwrap()["result"]["instructions"].as_str().unwrap().contains("Customer data"));

        set_access(&s, false, false);
        assert_eq!(call(&s, "list_customers", json!({}))["isError"], false);
    }

    #[test]
    fn unknown_method_is_a_protocol_error() {
        let (_dir, s) = state();
        assert_eq!(run(&s, request("resources/list", json!({}))).unwrap()["error"]["code"], -32601);
    }
}
