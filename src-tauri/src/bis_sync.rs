/// BiS list sync module.
/// Downloads BiS data from the website and writes it as Lua SavedVariables.
/// Reads obtained status changes and syncs them back.

use crate::lua_parser;
use crate::lua_writer;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObtainedItemUpdate {
    pub bis_list_item_id: String,
    pub is_obtained: bool,
    pub obtained_at: Option<i64>,
}

/// Convert API response JSON into Lua SavedVariables format and write to disk.
pub fn write_bis_to_saved_variables(api_response: &Value, sv_path: &Path) -> Result<(), String> {
    let characters = api_response
        .get("characters")
        .ok_or("Missing 'characters' in API response")?;

    let version = api_response
        .get("version")
        .and_then(|v| v.as_i64())
        .unwrap_or(1);

    // Build the Lua table structure
    let mut lua_table = serde_json::Map::new();

    // _meta section
    lua_table.insert(
        "_meta".to_string(),
        json!({
            "version": version,
            "downloaded_at": chrono::Utc::now().timestamp(),
            "source": "goldschweine.de"
        }),
    );

    // Character sections
    if let Some(chars) = characters.as_object() {
        for (char_key, char_data) in chars {
            lua_table.insert(char_key.clone(), char_data.clone());
        }
    }

    let lua_content = lua_writer::write_saved_variable("CharTrackerBiS", &Value::Object(lua_table));

    std::fs::write(sv_path, &lua_content).map_err(|e| {
        format!(
            "Failed to write CharTrackerBiS.lua to {}: {}",
            sv_path.display(),
            e
        )
    })?;

    Ok(())
}

/// Read CharTrackerBiS.lua and find items where is_obtained changed to true.
pub fn read_obtained_items(sv_path: &Path) -> Result<Vec<ObtainedItemUpdate>, String> {
    if !sv_path.exists() {
        return Ok(vec![]);
    }

    let content = std::fs::read_to_string(sv_path)
        .map_err(|e| format!("Failed to read CharTrackerBiS.lua: {}", e))?;

    let bis_data = lua_parser::parse_saved_variable(&content, "CharTrackerBiS")
        .map_err(|e| format!("Failed to parse CharTrackerBiS.lua: {}", e))?;

    let mut obtained = Vec::new();

    if let Some(chars) = bis_data.as_object() {
        for (key, char_data) in chars {
            if key == "_meta" {
                continue;
            }

            if let Some(items) = char_data.get("items") {
                // lua_parser returns numeric-keyed tables as JSON arrays
                let item_iter: Vec<&Value> = if let Some(arr) = items.as_array() {
                    arr.iter().collect()
                } else if let Some(obj) = items.as_object() {
                    obj.values().collect()
                } else {
                    vec![]
                };

                for item in item_iter {
                    let is_obtained = item
                        .get("is_obtained")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);

                    if is_obtained {
                        if let Some(id) = item.get("bis_list_item_id").and_then(|v| v.as_str())
                        {
                            let obtained_at = item
                                .get("obtained_at")
                                .and_then(|v| v.as_i64());

                            obtained.push(ObtainedItemUpdate {
                                bis_list_item_id: id.to_string(),
                                is_obtained: true,
                                obtained_at,
                            });
                        }
                    }
                }
            }
        }
    }

    Ok(obtained)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_write_bis_to_saved_variables() {
        let api_response = json!({
            "characters": {
                "Arthas-Thekal": {
                    "list_name": "Main Spec",
                    "character_class": "DEATHKNIGHT",
                    "character_spec": "Frost",
                    "items": [
                        {
                            "item_id": 95814,
                            "item_name": "Unerring Vision of Lei Shen",
                            "slot_type": "Trinket",
                            "source": "Lei Shen",
                            "source_normalized": "lei shen",
                            "priority": 1,
                            "is_obtained": false,
                            "bis_list_item_id": "abc-123"
                        }
                    ]
                }
            },
            "version": 1,
            "timestamp": "2026-02-18T12:00:00Z"
        });

        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().to_path_buf();

        write_bis_to_saved_variables(&api_response, &path).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("CharTrackerBiS"));
        assert!(content.contains("Arthas-Thekal"));
        assert!(content.contains("Unerring Vision of Lei Shen"));
    }

    #[test]
    fn test_read_obtained_items() {
        let lua_content = r#"
CharTrackerBiS = {
    ["_meta"] = {
        ["version"] = 1,
    },
    ["Arthas-Thekal"] = {
        ["items"] = {
            [1] = {
                ["item_id"] = 95814,
                ["item_name"] = "Unerring Vision",
                ["is_obtained"] = true,
                ["obtained_at"] = 1708300000,
                ["bis_list_item_id"] = "abc-123",
            },
            [2] = {
                ["item_id"] = 95810,
                ["item_name"] = "Other Item",
                ["is_obtained"] = false,
                ["bis_list_item_id"] = "def-456",
            },
        },
    },
}
"#;

        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(lua_content.as_bytes()).unwrap();
        tmp.flush().unwrap();

        let obtained = read_obtained_items(tmp.path()).unwrap();
        assert_eq!(obtained.len(), 1);
        assert_eq!(obtained[0].bis_list_item_id, "abc-123");
        assert_eq!(obtained[0].is_obtained, true);
        assert_eq!(obtained[0].obtained_at, Some(1708300000));
    }
}
