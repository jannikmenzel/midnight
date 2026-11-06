use std::sync::Mutex;
use std::time::{Duration, Instant};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::db::AppState;
use crate::error::{msg, AppResult};
use crate::i18n::tr;

const PIN_KEY: &str = "app_pin";
const MCP_KEY: &str = "mcp_access";
const ROUNDS: u32 = 100_000;
const MAX_ATTEMPTS: u32 = 5;
const COOLDOWN: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct McpAccess {
    pub read_only: bool,
    pub private: bool,
}

#[derive(Serialize, Deserialize)]
struct StoredPin {
    salt: String,
    hash: String,
}

#[derive(Default)]
struct Throttle {
    failures: u32,
    until: Option<Instant>,
}

impl Throttle {
    fn wait(&self, now: Instant) -> Option<u64> {
        self.until.filter(|until| *until > now).map(|until| (until - now).as_secs().max(1))
    }

    fn record(&mut self, now: Instant, correct: bool) {
        if correct {
            *self = Throttle::default();
            return;
        }
        self.failures += 1;
        if self.failures >= MAX_ATTEMPTS {
            self.failures = 0;
            self.until = Some(now + COOLDOWN);
        }
    }
}

static THROTTLE: Mutex<Throttle> = Mutex::new(Throttle { failures: 0, until: None });

fn read_key<T: serde::de::DeserializeOwned>(conn: &Connection, key: &str) -> AppResult<Option<T>> {
    let raw: Option<String> = conn.query_row("SELECT value FROM settings WHERE key = ?1", params![key], |r| r.get(0)).optional()?;
    Ok(raw.and_then(|json| serde_json::from_str(&json).ok()))
}

fn write_key<T: Serialize>(conn: &Connection, key: &str, value: &T) -> AppResult<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, serde_json::to_string(value)?],
    )?;
    Ok(())
}

pub fn load_mcp_access(conn: &Connection) -> AppResult<McpAccess> {
    let raw: Option<String> = conn.query_row("SELECT value FROM settings WHERE key = ?1", params![MCP_KEY], |r| r.get(0)).optional()?;
    Ok(match raw {
        None => McpAccess::default(),
        Some(json) => serde_json::from_str(&json).unwrap_or(McpAccess { read_only: true, private: true }),
    })
}

pub async fn get_mcp_access(state: &AppState) -> AppResult<McpAccess> {
    load_mcp_access(&*state.conn()?)
}

pub async fn save_mcp_access(state: &AppState, access: McpAccess) -> AppResult<McpAccess> {
    write_key(&*state.conn()?, MCP_KEY, &access)?;
    Ok(access)
}

fn hash_pin(salt: &str, pin: &str) -> String {
    let mut digest = Sha256::digest(format!("{salt}:{pin}").as_bytes());
    for _ in 0..ROUNDS {
        digest = Sha256::digest(digest);
    }
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

fn valid_pin(pin: &str) -> bool {
    pin.len() == 5 && pin.bytes().all(|b| b.is_ascii_digit())
}

fn stored_pin(conn: &Connection) -> AppResult<Option<StoredPin>> {
    read_key(conn, PIN_KEY)
}

fn matches(stored: &StoredPin, pin: &str) -> bool {
    valid_pin(pin) && hash_pin(&stored.salt, pin) == stored.hash
}

fn attempt(stored: &StoredPin, pin: &str) -> AppResult<bool> {
    let now = Instant::now();
    let mut throttle = THROTTLE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(seconds) = throttle.wait(now) {
        return Err(msg(tr!("Too many attempts. Try again in {seconds} s.", "Zu viele Versuche. In {seconds} s erneut versuchen.")));
    }
    let correct = matches(stored, pin);
    throttle.record(now, correct);
    Ok(correct)
}

pub async fn pin_enabled(state: &AppState) -> AppResult<bool> {
    Ok(stored_pin(&*state.conn()?)?.is_some())
}

pub async fn set_pin(state: &AppState, current: Option<String>, pin: Option<String>) -> AppResult<bool> {
    let conn = state.conn()?;
    if let Some(stored) = stored_pin(&conn)? {
        if !attempt(&stored, current.as_deref().unwrap_or_default())? {
            return Err(msg(tr!("The current PIN is wrong.", "Die aktuelle PIN ist falsch.")));
        }
    }
    match pin {
        Some(pin) => {
            if !valid_pin(&pin) {
                return Err(msg(tr!("The PIN must have exactly 5 digits.", "Die PIN muss genau 5 Ziffern haben.")));
            }
            let salt = uuid::Uuid::new_v4().simple().to_string();
            write_key(&conn, PIN_KEY, &StoredPin { hash: hash_pin(&salt, &pin), salt })?;
            Ok(true)
        }
        None => {
            conn.execute("DELETE FROM settings WHERE key = ?1", params![PIN_KEY])?;
            Ok(false)
        }
    }
}

pub async fn pin_cooldown() -> AppResult<Option<u64>> {
    Ok(THROTTLE.lock().unwrap_or_else(|e| e.into_inner()).wait(Instant::now()))
}

pub async fn unlock(state: &AppState, pin: String) -> AppResult<bool> {
    let stored = stored_pin(&*state.conn()?)?;
    match stored {
        Some(stored) => attempt(&stored, &pin),
        None => Ok(true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> AppState {
        let dir = std::env::temp_dir().join(format!("midnight-security-{}", crate::util::new_id()));
        AppState::new(crate::db::open_in_memory().unwrap(), dir).unwrap()
    }

    fn run<T>(f: impl std::future::Future<Output = T>) -> T {
        tauri::async_runtime::block_on(f)
    }

    #[test]
    fn pin_is_stored_hashed_and_needs_the_current_one_to_change() {
        let s = state();
        assert!(!run(pin_enabled(&s)).unwrap());
        assert!(run(unlock(&s, String::new())).unwrap(), "no PIN, nothing to unlock");

        assert!(run(set_pin(&s, None, Some("1234".into()))).is_err(), "four digits");
        assert!(run(set_pin(&s, None, Some("12a45".into()))).is_err(), "digits only");
        assert!(run(set_pin(&s, None, Some("04711".into()))).unwrap());
        let raw: String = s.conn().unwrap().query_row("SELECT value FROM settings WHERE key = 'app_pin'", [], |r| r.get(0)).unwrap();
        assert!(!raw.contains("04711"), "never in plain text");

        assert!(run(unlock(&s, "04711".into())).unwrap());
        assert!(run(set_pin(&s, Some("11111".into()), None)).is_err(), "wrong current PIN");
        assert!(run(set_pin(&s, Some("04711".into()), Some("99999".into()))).unwrap());
        assert!(run(unlock(&s, "99999".into())).unwrap());
        assert!(!run(set_pin(&s, Some("99999".into()), None)).unwrap());
        assert!(!run(pin_enabled(&s)).unwrap());
    }

    #[test]
    fn five_wrong_attempts_pause_the_input() {
        let start = Instant::now();
        let mut t = Throttle::default();
        for _ in 0..MAX_ATTEMPTS - 1 {
            t.record(start, false);
            assert_eq!(t.wait(start), None);
        }
        t.record(start, false);
        assert_eq!(t.wait(start), Some(30));
        assert_eq!(t.wait(start + COOLDOWN), None, "free again after the pause");
        t.record(start + COOLDOWN, true);
        assert_eq!((t.failures, t.until), (0, None));
    }

    #[test]
    fn mcp_access_defaults_to_full_and_round_trips() {
        let s = state();
        assert_eq!(run(get_mcp_access(&s)).unwrap(), McpAccess::default());
        let access = McpAccess { read_only: true, private: true };
        run(save_mcp_access(&s, access)).unwrap();
        assert_eq!(run(get_mcp_access(&s)).unwrap(), access);
        s.conn().unwrap().execute("UPDATE settings SET value = 'broken' WHERE key = 'mcp_access'", []).unwrap();
        assert_eq!(run(get_mcp_access(&s)).unwrap(), McpAccess { read_only: true, private: true }, "a broken entry locks down");
    }
}
