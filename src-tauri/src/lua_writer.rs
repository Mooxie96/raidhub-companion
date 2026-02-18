/// Converts serde_json::Value into WoW SavedVariables Lua format.
/// Produces valid Lua that WoW's restricted parser can read.

use serde_json::Value;

/// Write a top-level SavedVariables assignment: `VariableName = { ... }\n`
pub fn write_saved_variable(name: &str, value: &Value) -> String {
    let mut output = String::new();
    output.push_str(name);
    output.push_str(" = ");
    write_value(&mut output, value, 0);
    output.push('\n');
    output
}

fn write_value(out: &mut String, value: &Value, indent: usize) {
    match value {
        Value::Null => out.push_str("nil"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                out.push_str(&i.to_string());
            } else if let Some(f) = n.as_f64() {
                // Avoid trailing zeros for clean Lua
                if f == f.floor() && f.abs() < 1e15 {
                    out.push_str(&format!("{}", f as i64));
                } else {
                    out.push_str(&format!("{}", f));
                }
            }
        }
        Value::String(s) => write_lua_string(out, s),
        Value::Array(arr) => write_array(out, arr, indent),
        Value::Object(map) => write_table(out, map, indent),
    }
}

fn write_lua_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\0' => out.push_str("\\0"),
            _ => out.push(c),
        }
    }
    out.push('"');
}

fn write_array(out: &mut String, arr: &[Value], indent: usize) {
    if arr.is_empty() {
        out.push_str("{}");
        return;
    }

    let inner = indent + 1;
    out.push_str("{\n");

    for (i, val) in arr.iter().enumerate() {
        write_indent(out, inner);
        // Lua arrays are 1-indexed
        out.push_str(&format!("[{}] = ", i + 1));
        write_value(out, val, inner);
        out.push_str(",\n");
    }

    write_indent(out, indent);
    out.push('}');
}

fn write_table(out: &mut String, map: &serde_json::Map<String, Value>, indent: usize) {
    if map.is_empty() {
        out.push_str("{}");
        return;
    }

    let inner = indent + 1;
    out.push_str("{\n");

    for (key, val) in map {
        write_indent(out, inner);
        out.push_str("[\"");
        // Escape the key
        for c in key.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                _ => out.push(c),
            }
        }
        out.push_str("\"] = ");
        write_value(out, val, inner);
        out.push_str(",\n");
    }

    write_indent(out, indent);
    out.push('}');
}

fn write_indent(out: &mut String, level: usize) {
    for _ in 0..level {
        out.push('\t');
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lua_parser;
    use serde_json::json;

    #[test]
    fn test_simple_string() {
        let val = json!("hello world");
        let lua = write_saved_variable("TestVar", &val);
        assert_eq!(lua, "TestVar = \"hello world\"\n");
    }

    #[test]
    fn test_number() {
        let val = json!(42);
        let lua = write_saved_variable("TestVar", &val);
        assert_eq!(lua, "TestVar = 42\n");
    }

    #[test]
    fn test_boolean() {
        let val = json!(true);
        let lua = write_saved_variable("TestVar", &val);
        assert_eq!(lua, "TestVar = true\n");
    }

    #[test]
    fn test_simple_table() {
        let val = json!({"name": "Test", "value": 123});
        let lua = write_saved_variable("TestVar", &val);
        assert!(lua.contains("[\"name\"] = \"Test\""));
        assert!(lua.contains("[\"value\"] = 123"));
    }

    #[test]
    fn test_array() {
        let val = json!([1, 2, 3]);
        let lua = write_saved_variable("TestVar", &val);
        assert!(lua.contains("[1] = 1"));
        assert!(lua.contains("[2] = 2"));
        assert!(lua.contains("[3] = 3"));
    }

    #[test]
    fn test_string_escaping() {
        let val = json!("He said \"hello\"");
        let lua = write_saved_variable("TestVar", &val);
        assert!(lua.contains("\\\"hello\\\""));
    }

    #[test]
    fn test_roundtrip_bis_data() {
        let bis = json!({
            "_meta": {
                "version": 1,
                "downloaded_at": 1708300000
            },
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
        });

        let lua = write_saved_variable("CharTrackerBiS", &bis);

        // Parse it back with our Lua parser
        let ct_bis = lua_parser::parse_saved_variable(&lua, "CharTrackerBiS")
            .expect("Failed to parse generated Lua");

        // Verify structure
        let meta = ct_bis.get("_meta").expect("Missing _meta");
        assert_eq!(meta.get("version").unwrap().as_i64().unwrap(), 1);

        let arthas = ct_bis.get("Arthas-Thekal").expect("Missing Arthas-Thekal");
        assert_eq!(arthas.get("list_name").unwrap().as_str().unwrap(), "Main Spec");

        let items = arthas.get("items").expect("Missing items");
        let items_arr = items.as_array().expect("items should be an array");
        let first = &items_arr[0];
        assert_eq!(first.get("item_id").unwrap().as_i64().unwrap(), 95814);
        assert_eq!(first.get("item_name").unwrap().as_str().unwrap(), "Unerring Vision of Lei Shen");
        assert_eq!(first.get("is_obtained").unwrap().as_bool().unwrap(), false);
    }

    #[test]
    fn test_nil_value() {
        let val = json!(null);
        let lua = write_saved_variable("TestVar", &val);
        assert_eq!(lua, "TestVar = nil\n");
    }

    #[test]
    fn test_nested_tables() {
        let val = json!({"outer": {"inner": "value"}});
        let lua = write_saved_variable("TestVar", &val);
        // Should produce nested indentation
        assert!(lua.contains("[\"outer\"] = {"));
        assert!(lua.contains("[\"inner\"] = \"value\""));
    }
}
