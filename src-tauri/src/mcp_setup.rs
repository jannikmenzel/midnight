use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{json, Map, Value};

use crate::error::{msg, AppResult};
use crate::i18n::tr;

const SERVER_NAME: &str = "midnight";

#[derive(Debug, Serialize)]
pub struct McpSetup {
    pub server_path: String,
    pub server_exists: bool,
    pub claude_code_command: String,
    pub desktop_config_path: Option<String>,
    pub desktop_configured: bool,
    pub desktop_snippet: String,
}

fn server_path() -> AppResult<PathBuf> {
    let exe = std::env::current_exe()?;
    let dir = exe.parent().ok_or_else(|| msg(tr!("App directory not found", "App-Verzeichnis nicht gefunden")))?;
    Ok(dir.join(if cfg!(windows) { "midnight-mcp.exe" } else { "midnight-mcp" }))
}

fn desktop_config_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if cfg!(target_os = "macos") {
        home.map(|h| h.join("Library/Application Support/Claude/claude_desktop_config.json"))
    } else if cfg!(windows) {
        std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("Claude/claude_desktop_config.json"))
    } else {
        home.map(|h| h.join(".config/Claude/claude_desktop_config.json"))
    }
}

pub fn shell_quote(s: &str) -> String {
    if !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "/._-+:@".contains(c)) {
        return s.to_string();
    }
    format!("'{}'", s.replace('\'', r"'\''"))
}

pub fn claude_code_command(server: &str) -> String {
    format!("claude mcp add {SERVER_NAME} -- {}", shell_quote(server))
}

fn server_entry(server: &str) -> Value {
    json!({ "command": server })
}

pub fn desktop_snippet(server: &str) -> String {
    serde_json::to_string_pretty(&json!({ "mcpServers": { SERVER_NAME: server_entry(server) } })).unwrap_or_default()
}

pub fn has_server(config: &Value, server: &str) -> bool {
    config["mcpServers"][SERVER_NAME]["command"].as_str() == Some(server)
}

pub fn with_server(mut config: Value, server: &str) -> AppResult<Value> {
    let invalid = || msg(tr!("The Claude Desktop config is not a JSON object", "Die Claude-Desktop-Konfiguration ist kein JSON-Objekt"));
    let root = config.as_object_mut().ok_or_else(invalid)?;
    let servers = root.entry("mcpServers").or_insert_with(|| Value::Object(Map::new()));
    if servers.is_null() {
        *servers = Value::Object(Map::new());
    }
    servers.as_object_mut().ok_or_else(invalid)?.insert(SERVER_NAME.into(), server_entry(server));
    Ok(config)
}

fn read_config(path: &Path) -> AppResult<Value> {
    if !path.exists() {
        return Ok(json!({}));
    }
    let raw = std::fs::read_to_string(path)?;
    if raw.trim().is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_str(&raw).map_err(|e| {
        msg(tr!(
            "Cannot read {}: {e}. Please fix the file first.",
            "{} ist nicht lesbar: {e}. Bitte die Datei zuerst reparieren.",
            path.display()
        ))
    })
}

pub fn install_into(path: &Path, server: &str) -> AppResult<()> {
    let config = with_server(read_config(path)?, server)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.midnight-tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(&config)? + "\n")?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

pub async fn mcp_setup() -> AppResult<McpSetup> {
    let server = server_path()?;
    let server_str = server.to_string_lossy().to_string();
    let config_path = desktop_config_path();
    let desktop_configured = config_path.as_deref().and_then(|p| read_config(p).ok()).is_some_and(|c| has_server(&c, &server_str));
    Ok(McpSetup {
        server_exists: server.is_file(),
        claude_code_command: claude_code_command(&server_str),
        desktop_config_path: config_path.map(|p| p.to_string_lossy().to_string()),
        desktop_configured,
        desktop_snippet: desktop_snippet(&server_str),
        server_path: server_str,
    })
}

pub async fn mcp_install_claude_desktop() -> AppResult<()> {
    let server = server_path()?;
    if !server.is_file() {
        return Err(msg(tr!("midnight-mcp not found at {}", "midnight-mcp nicht gefunden unter {}", server.display())));
    }
    let path = desktop_config_path().ok_or_else(|| msg(tr!("Claude Desktop config not found", "Claude-Desktop-Konfiguration nicht gefunden")))?;
    install_into(&path, &server.to_string_lossy())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SERVER: &str = "/Applications/Midnight.app/Contents/MacOS/midnight-mcp";

    #[test]
    fn quotes_paths_for_the_shell_only_when_needed() {
        assert_eq!(shell_quote("/usr/bin/x"), "/usr/bin/x");
        assert_eq!(shell_quote("/My Apps/x"), "'/My Apps/x'");
        assert_eq!(shell_quote("/it's/x"), r"'/it'\''s/x'");
        assert_eq!(claude_code_command(SERVER), format!("claude mcp add midnight -- {SERVER}"));
    }

    #[test]
    fn merge_keeps_other_servers_and_settings() {
        let config = json!({ "theme": "dark", "mcpServers": { "other": { "command": "x" }, "midnight": { "command": "/old" } } });
        assert!(!has_server(&config, SERVER));
        let merged = with_server(config, SERVER).unwrap();
        assert_eq!(merged["theme"], "dark");
        assert_eq!(merged["mcpServers"]["other"]["command"], "x");
        assert!(has_server(&merged, SERVER), "stale path is repointed");
    }

    #[test]
    fn merge_creates_missing_sections_and_rejects_non_objects() {
        assert!(has_server(&with_server(json!({}), SERVER).unwrap(), SERVER));
        assert!(has_server(&with_server(json!({ "mcpServers": null }), SERVER).unwrap(), SERVER));
        assert!(with_server(json!([]), SERVER).is_err());
        assert!(with_server(json!({ "mcpServers": [] }), SERVER).is_err());
    }

    #[test]
    fn snippet_is_valid_config() {
        let snippet: Value = serde_json::from_str(&desktop_snippet(SERVER)).unwrap();
        assert!(has_server(&snippet, SERVER));
    }

    #[test]
    fn install_writes_new_file_and_refuses_to_clobber_broken_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Claude/claude_desktop_config.json");
        install_into(&path, SERVER).unwrap();
        let written: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(has_server(&written, SERVER));

        std::fs::write(&path, "{ broken").unwrap();
        assert!(install_into(&path, SERVER).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ broken", "broken file is left alone");
    }
}
