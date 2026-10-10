use std::io::{BufRead, Write};
use std::path::PathBuf;

use serde_json::{json, Value};

use crate::commands::{dispatch, Access, TOOLS};
use crate::db::{self, AppState};
use crate::error::{msg, AppResult};
use crate::i18n::{self, tr};

const PROTOCOL_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

const INSTRUCTIONS: &str = "Midnight is a local bookkeeping app for freelancers and the self-employed in Germany. \
Money amounts are always whole cents (€12.50 = 1250), dates are YYYY-MM-DD. \
save_* tools create a record without id and update it with id; before changing a record, always read it and write it back in full. \
File paths must be absolute. Resetting all data is only possible inside the app.";

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

fn tool_list() -> Value {
    let tools: Vec<Value> = TOOLS
        .iter()
        .filter(|t| t.access != Access::AppOnly)
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
                    "destructiveHint": t.access == Access::Delete,
                    "openWorldHint": false,
                },
            })
        })
        .collect();
    json!({ "tools": tools })
}

async fn call_tool(state: &AppState, params: &Value) -> Value {
    let name = params["name"].as_str().unwrap_or_default();
    let args = match &params["arguments"] {
        Value::Null => json!({}),
        other => other.clone(),
    };
    match dispatch(state, name, args).await {
        Ok(result) => json!({
            "content": [{ "type": "text", "text": serde_json::to_string_pretty(&result).unwrap_or_default() }],
            "isError": false,
        }),
        Err(e) => json!({
            "content": [{ "type": "text", "text": e }],
            "isError": true,
        }),
    }
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
                "instructions": INSTRUCTIONS,
            }))
        }
        "ping" => Ok(json!({})),
        "tools/list" => Ok(tool_list()),
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

    #[test]
    fn unknown_method_is_a_protocol_error() {
        let (_dir, s) = state();
        assert_eq!(run(&s, request("resources/list", json!({}))).unwrap()["error"]["code"], -32601);
    }
}
