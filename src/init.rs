use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

const HOOK_COMMAND: &str = "poke hook-notify";

/// Get the path to Claude Code's settings.json.
fn claude_settings_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".claude").join("settings.json"))
}

/// Read existing settings or return an empty object.
fn read_settings(path: &Path) -> io::Result<Value> {
    if path.exists() {
        let contents = fs::read_to_string(path)?;
        serde_json::from_str(&contents).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    } else {
        Ok(json!({}))
    }
}

/// Check if the poke hook-notify command is already present in the Notification hooks.
///
/// Claude Code settings use PascalCase event names and a nested structure:
/// ```json
/// "hooks": { "Notification": [{ "hooks": [{ "type": "command", "command": "..." }] }] }
/// ```
fn has_poke_hook(settings: &Value) -> bool {
    settings
        .get("hooks")
        .and_then(|h| h.get("Notification"))
        .and_then(|n| n.as_array())
        .map(|groups| {
            groups.iter().any(|group| {
                group
                    .get("hooks")
                    .and_then(|h| h.as_array())
                    .map(|hooks| {
                        hooks.iter().any(|entry| {
                            entry.get("command").and_then(|c| c.as_str()) == Some(HOOK_COMMAND)
                        })
                    })
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

/// Add the poke hook-notify entry to settings, preserving all existing config.
///
/// Creates the correct Claude Code hook structure:
/// ```json
/// "hooks": { "Notification": [{ "hooks": [{ "type": "command", "command": "poke hook-notify" }] }] }
/// ```
fn add_poke_hook(settings: &mut Value) -> Result<(), String> {
    let hook_group = json!({
        "hooks": [
            {
                "type": "command",
                "command": HOOK_COMMAND
            }
        ]
    });

    let obj = settings
        .as_object_mut()
        .ok_or("settings is not a JSON object")?;
    if !obj.contains_key("hooks") {
        obj.insert("hooks".to_string(), json!({}));
    }

    let hooks = obj
        .get_mut("hooks")
        .and_then(|h| h.as_object_mut())
        .ok_or("\"hooks\" is not a JSON object")?;
    if !hooks.contains_key("Notification") {
        hooks.insert("Notification".to_string(), json!([]));
    }

    let notification = hooks
        .get_mut("Notification")
        .and_then(|n| n.as_array_mut())
        .ok_or("\"Notification\" is not a JSON array")?;

    notification.push(hook_group);
    Ok(())
}

/// Write settings back to disk, creating parent directories as needed.
fn write_settings(path: &Path, settings: &Value) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(settings)
        .map_err(io::Error::other)?;
    fs::write(path, json)
}

/// Run the init command using a specific settings path (for testing).
pub fn run_with_path(path: &Path) -> Result<String, String> {
    let mut settings =
        read_settings(path).map_err(|e| format!("failed to read settings: {}", e))?;

    if has_poke_hook(&settings) {
        return Ok("Claude Code hook already configured.".to_string());
    }

    add_poke_hook(&mut settings).map_err(|e| format!("malformed settings: {}", e))?;

    write_settings(path, &settings).map_err(|e| format!("failed to write settings: {}", e))?;

    // Also create the events directory
    if let Some(home) = dirs::home_dir() {
        let _ = fs::create_dir_all(home.join(".poke").join("events"));
    }

    Ok("Added poke hook-notify to Claude Code settings.".to_string())
}

/// Run the init command using the default settings path.
pub fn run() -> Result<String, String> {
    let path =
        claude_settings_path().ok_or_else(|| "could not determine home directory".to_string())?;
    run_with_path(&path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn adds_hook_to_empty_settings() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("settings.json");

        let msg = run_with_path(&path).unwrap();
        assert!(msg.contains("Added"));

        let settings: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert!(has_poke_hook(&settings));
    }

    #[test]
    fn adds_hook_to_existing_settings_preserves_config() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("settings.json");
        let existing = json!({
            "apiKey": "sk-test",
            "theme": "dark"
        });
        fs::write(&path, serde_json::to_string_pretty(&existing).unwrap()).unwrap();

        run_with_path(&path).unwrap();

        let settings: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert!(has_poke_hook(&settings));
        assert_eq!(settings["apiKey"], "sk-test");
        assert_eq!(settings["theme"], "dark");
    }

    #[test]
    fn preserves_existing_hooks() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("settings.json");
        let existing = json!({
            "hooks": {
                "Notification": [
                    {"hooks": [{"type": "command", "command": "other-tool notify"}]}
                ],
                "PreToolUse": [
                    {"hooks": [{"type": "command", "command": "logger"}]}
                ]
            }
        });
        fs::write(&path, serde_json::to_string_pretty(&existing).unwrap()).unwrap();

        run_with_path(&path).unwrap();

        let settings: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        let notifs = settings["hooks"]["Notification"].as_array().unwrap();
        assert_eq!(notifs.len(), 2);
        assert_eq!(notifs[0]["hooks"][0]["command"], "other-tool notify");
        assert_eq!(notifs[1]["hooks"][0]["command"], HOOK_COMMAND);
        // PreToolUse preserved
        assert!(settings["hooks"]["PreToolUse"].is_array());
    }

    #[test]
    fn idempotent_does_not_duplicate() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("settings.json");

        let msg1 = run_with_path(&path).unwrap();
        assert!(msg1.contains("Added"));

        let msg2 = run_with_path(&path).unwrap();
        assert!(msg2.contains("already configured"));

        let settings: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        let notifs = settings["hooks"]["Notification"].as_array().unwrap();
        assert_eq!(notifs.len(), 1);
    }

    #[test]
    fn creates_parent_directories() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nested").join("deep").join("settings.json");

        run_with_path(&path).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn has_poke_hook_returns_false_for_empty() {
        assert!(!has_poke_hook(&json!({})));
        assert!(!has_poke_hook(&json!({"hooks": {}})));
        assert!(!has_poke_hook(&json!({"hooks": {"Notification": []}})));
        // Old lowercase format should not match
        assert!(!has_poke_hook(&json!({
            "hooks": {
                "notification": [
                    {"type": "command", "command": "poke hook-notify"}
                ]
            }
        })));
    }

    #[test]
    fn has_poke_hook_returns_true_when_present() {
        let settings = json!({
            "hooks": {
                "Notification": [
                    {
                        "hooks": [
                            {"type": "command", "command": "poke hook-notify"}
                        ]
                    }
                ]
            }
        });
        assert!(has_poke_hook(&settings));
    }

    #[test]
    fn has_poke_hook_with_matcher() {
        let settings = json!({
            "hooks": {
                "Notification": [
                    {
                        "matcher": "some-pattern",
                        "hooks": [
                            {"type": "command", "command": "poke hook-notify"}
                        ]
                    }
                ]
            }
        });
        assert!(has_poke_hook(&settings));
    }
}
