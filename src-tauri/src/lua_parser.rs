/// Custom Lua SavedVariables parser for WoW addon data.
/// Handles the restricted Lua subset used in SavedVariables files:
/// global assignments, nested tables, strings, numbers, booleans, nil.

use serde_json::{json, Map, Value};
use std::iter::Peekable;
use std::str::Chars;

#[derive(Debug)]
pub struct ParseError {
    pub message: String,
    pub position: usize,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Parse error at position {}: {}", self.position, self.message)
    }
}

struct Parser<'a> {
    input: &'a str,
    chars: Peekable<Chars<'a>>,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input,
            chars: input.chars().peekable(),
            pos: 0,
        }
    }

    fn error(&self, msg: &str) -> ParseError {
        ParseError {
            message: msg.to_string(),
            position: self.pos,
        }
    }

    fn peek(&mut self) -> Option<char> {
        self.chars.peek().copied()
    }

    fn advance(&mut self) -> Option<char> {
        if let Some(c) = self.chars.next() {
            self.pos += c.len_utf8();
            Some(c)
        } else {
            None
        }
    }

    fn skip_whitespace_and_comments(&mut self) {
        loop {
            // Skip whitespace
            while let Some(c) = self.peek() {
                if c.is_whitespace() {
                    self.advance();
                } else {
                    break;
                }
            }
            // Skip single-line comments: -- ...
            if self.pos + 1 < self.input.len() && &self.input[self.pos..self.pos + 2] == "--" {
                // Check for block comment --[[ ... ]]
                if self.pos + 3 < self.input.len() && &self.input[self.pos..self.pos + 4] == "--[[" {
                    self.advance(); // -
                    self.advance(); // -
                    self.advance(); // [
                    self.advance(); // [
                    while self.pos + 1 < self.input.len() {
                        if &self.input[self.pos..self.pos + 2] == "]]" {
                            self.advance(); // ]
                            self.advance(); // ]
                            break;
                        }
                        self.advance();
                    }
                } else {
                    while let Some(c) = self.advance() {
                        if c == '\n' {
                            break;
                        }
                    }
                }
                continue;
            }
            break;
        }
    }

    fn expect(&mut self, expected: char) -> Result<(), ParseError> {
        self.skip_whitespace_and_comments();
        match self.advance() {
            Some(c) if c == expected => Ok(()),
            Some(c) => Err(self.error(&format!("expected '{}', found '{}'", expected, c))),
            None => Err(self.error(&format!("expected '{}', found end of input", expected))),
        }
    }

    fn parse_string(&mut self) -> Result<String, ParseError> {
        let quote = self.advance().ok_or_else(|| self.error("expected string"))?;
        if quote != '"' && quote != '\'' {
            return Err(self.error(&format!("expected quote, found '{}'", quote)));
        }

        let mut result = String::new();
        loop {
            match self.advance() {
                Some(c) if c == quote => return Ok(result),
                Some('\\') => {
                    match self.advance() {
                        Some('n') => result.push('\n'),
                        Some('t') => result.push('\t'),
                        Some('\\') => result.push('\\'),
                        Some('"') => result.push('"'),
                        Some('\'') => result.push('\''),
                        Some('r') => result.push('\r'),
                        Some(c) => {
                            result.push('\\');
                            result.push(c);
                        }
                        None => return Err(self.error("unexpected end of string escape")),
                    }
                }
                Some(c) => result.push(c),
                None => return Err(self.error("unterminated string")),
            }
        }
    }

    fn parse_number(&mut self) -> Result<Value, ParseError> {
        let start = self.pos;
        let mut has_dot = false;
        let mut has_digits = false;

        // Handle negative numbers
        if self.peek() == Some('-') {
            self.advance();
        }

        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                has_digits = true;
                self.advance();
            } else if c == '.' && !has_dot {
                has_dot = true;
                self.advance();
            } else {
                break;
            }
        }

        if !has_digits {
            return Err(self.error("expected number"));
        }

        let num_str = &self.input[start..self.pos];
        if has_dot {
            num_str
                .parse::<f64>()
                .map(|n| json!(n))
                .map_err(|_| self.error(&format!("invalid float: {}", num_str)))
        } else {
            num_str
                .parse::<i64>()
                .map(|n| json!(n))
                .map_err(|_| self.error(&format!("invalid integer: {}", num_str)))
        }
    }

    fn parse_identifier(&mut self) -> Result<String, ParseError> {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c.is_alphanumeric() || c == '_' {
                self.advance();
            } else {
                break;
            }
        }
        if self.pos == start {
            return Err(self.error("expected identifier"));
        }
        Ok(self.input[start..self.pos].to_string())
    }

    fn parse_value(&mut self) -> Result<Value, ParseError> {
        self.skip_whitespace_and_comments();

        match self.peek() {
            Some('{') => self.parse_table(),
            Some('"') | Some('\'') => self.parse_string().map(Value::String),
            Some(c) if c.is_ascii_digit() || c == '-' => self.parse_number(),
            Some(c) if c.is_alphabetic() || c == '_' => {
                let ident = self.parse_identifier()?;
                match ident.as_str() {
                    "true" => Ok(Value::Bool(true)),
                    "false" => Ok(Value::Bool(false)),
                    "nil" => Ok(Value::Null),
                    _ => Err(self.error(&format!("unexpected identifier: {}", ident))),
                }
            }
            Some(c) => Err(self.error(&format!("unexpected character: '{}'", c))),
            None => Err(self.error("unexpected end of input")),
        }
    }

    fn parse_table(&mut self) -> Result<Value, ParseError> {
        self.expect('{')?;

        let mut map: Map<String, Value> = Map::new();
        let mut max_int_key: i64 = 0;
        let mut is_pure_array = true;
        let mut array_entries: Vec<(i64, Value)> = Vec::new();
        let mut auto_index: i64 = 1; // Lua arrays are 1-based

        loop {
            self.skip_whitespace_and_comments();

            if self.peek() == Some('}') {
                self.advance();
                break;
            }

            // Parse entry: could be [key] = value, key = value, or just value (array-style)
            let (key, value) = self.parse_table_entry()?;

            match key {
                TableKey::String(s) => {
                    is_pure_array = false;
                    map.insert(s, value);
                }
                TableKey::Integer(i) => {
                    array_entries.push((i, value.clone()));
                    if i > max_int_key {
                        max_int_key = i;
                    }
                    if i >= auto_index {
                        auto_index = i + 1;
                    }
                    map.insert(i.to_string(), value);
                }
                TableKey::Auto => {
                    let i = auto_index;
                    auto_index += 1;
                    array_entries.push((i, value.clone()));
                    if i > max_int_key {
                        max_int_key = i;
                    }
                    map.insert(i.to_string(), value);
                }
            }

            // Skip optional comma/semicolon
            self.skip_whitespace_and_comments();
            if self.peek() == Some(',') || self.peek() == Some(';') {
                self.advance();
            }
        }

        // Determine if this is an array: all keys are sequential integers 1..n
        if is_pure_array && !array_entries.is_empty() {
            let count = array_entries.len() as i64;
            if max_int_key == count {
                // Check all keys are 1..n
                let mut sorted = array_entries;
                sorted.sort_by_key(|(k, _)| *k);
                let is_sequential = sorted.iter().enumerate().all(|(i, (k, _))| *k == (i as i64 + 1));
                if is_sequential {
                    let arr: Vec<Value> = sorted.into_iter().map(|(_, v)| v).collect();
                    return Ok(Value::Array(arr));
                }
            }
        }

        Ok(Value::Object(map))
    }

    fn parse_table_entry(&mut self) -> Result<(TableKey, Value), ParseError> {
        self.skip_whitespace_and_comments();

        if self.peek() == Some('[') {
            // Bracket key: ["string"] = value or [number] = value
            self.advance(); // consume [
            self.skip_whitespace_and_comments();

            let key = if self.peek() == Some('"') || self.peek() == Some('\'') {
                let s = self.parse_string()?;
                TableKey::String(s)
            } else {
                // Numeric key
                let num = self.parse_number()?;
                match num.as_i64() {
                    Some(i) => TableKey::Integer(i),
                    None => TableKey::String(num.to_string()),
                }
            };

            self.expect(']')?;
            self.skip_whitespace_and_comments();
            self.expect('=')?;
            let value = self.parse_value()?;
            Ok((key, value))
        } else if self.peek().map_or(false, |c| c.is_alphabetic() || c == '_') {
            // Could be bare key = value, or a boolean/nil value
            let start_pos = self.pos;
            let ident = self.parse_identifier()?;
            self.skip_whitespace_and_comments();

            if self.peek() == Some('=') {
                // It's a key = value
                self.advance(); // consume =
                let value = self.parse_value()?;
                Ok((TableKey::String(ident), value))
            } else {
                // It's a value (identifier like true/false/nil) — shouldn't normally happen
                // in WoW SavedVariables, but handle it for robustness
                let value = match ident.as_str() {
                    "true" => Value::Bool(true),
                    "false" => Value::Bool(false),
                    "nil" => Value::Null,
                    _ => return Err(ParseError {
                        message: format!("unexpected identifier in table: {}", ident),
                        position: start_pos,
                    }),
                };
                // This would be an array-style entry — but we don't track implicit indexing
                // In practice, WoW SavedVariables always use explicit keys
                Ok((TableKey::Integer(1), value))
            }
        } else if self.peek().map_or(false, |c| c == '{' || c == '"' || c == '\'' || c.is_ascii_digit() || c == '-') {
            // Implicit array entry: value without explicit key (e.g., { {["id"]=1}, {["id"]=2} })
            let value = self.parse_value()?;
            Ok((TableKey::Auto, value))
        } else {
            Err(self.error("expected table key or '}'"))
        }
    }
}

enum TableKey {
    String(String),
    Integer(i64),
    Auto, // Implicit numeric key (array-style entry without [n] = ...)
}

/// Parse a WoW SavedVariables file and extract the named global variable.
/// Returns the parsed table as a serde_json::Value.
pub fn parse_saved_variable(input: &str, variable_name: &str) -> Result<Value, ParseError> {
    // Find "VariableName = " in the file
    let search = format!("{} = ", variable_name);
    let start_idx = input.find(&search).ok_or_else(|| ParseError {
        message: format!("variable '{}' not found in file", variable_name),
        position: 0,
    })?;

    let offset = start_idx + search.len();
    let remaining = &input[offset..];

    let mut parser = Parser::new(remaining);
    parser.parse_value()
}

/// Convenience: parse CharTrackerDB from a SavedVariables file.
pub fn parse_chartracker_db(input: &str) -> Result<Value, ParseError> {
    parse_saved_variable(input, "CharTrackerDB")
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_string_table() {
        let lua = r#"TestDB = {
            ["name"] = "Hello",
            ["value"] = 42,
        }"#;
        let result = parse_saved_variable(lua, "TestDB").unwrap();
        assert_eq!(result["name"], "Hello");
        assert_eq!(result["value"], 42);
    }

    #[test]
    fn test_nested_table() {
        let lua = r#"TestDB = {
            ["basic"] = {
                ["name"] = "Arthas",
                ["level"] = 90,
            },
        }"#;
        let result = parse_saved_variable(lua, "TestDB").unwrap();
        assert_eq!(result["basic"]["name"], "Arthas");
        assert_eq!(result["basic"]["level"], 90);
    }

    #[test]
    fn test_array_table() {
        let lua = r#"TestDB = {
            ["items"] = {
                [1] = "sword",
                [2] = "shield",
                [3] = "potion",
            },
        }"#;
        let result = parse_saved_variable(lua, "TestDB").unwrap();
        let items = result["items"].as_array().unwrap();
        assert_eq!(items.len(), 3);
        assert_eq!(items[0], "sword");
        assert_eq!(items[1], "shield");
        assert_eq!(items[2], "potion");
    }

    #[test]
    fn test_boolean_and_nil() {
        let lua = r#"TestDB = {
            ["active"] = true,
            ["deleted"] = false,
            ["optional"] = nil,
        }"#;
        let result = parse_saved_variable(lua, "TestDB").unwrap();
        assert_eq!(result["active"], true);
        assert_eq!(result["deleted"], false);
        assert!(result["optional"].is_null());
    }

    #[test]
    fn test_negative_number() {
        let lua = r#"TestDB = {
            ["offset"] = -42,
            ["ratio"] = 3.14,
        }"#;
        let result = parse_saved_variable(lua, "TestDB").unwrap();
        assert_eq!(result["offset"], -42);
        assert_eq!(result["ratio"], 3.14);
    }

    #[test]
    fn test_comments_ignored() {
        let lua = r#"-- This is a comment
        TestDB = {
            -- another comment
            ["name"] = "test", -- inline comment
        }"#;
        let result = parse_saved_variable(lua, "TestDB").unwrap();
        assert_eq!(result["name"], "test");
    }

    #[test]
    fn test_bare_keys() {
        let lua = r#"TestDB = {
            name = "test",
            value = 42,
        }"#;
        let result = parse_saved_variable(lua, "TestDB").unwrap();
        assert_eq!(result["name"], "test");
        assert_eq!(result["value"], 42);
    }

    #[test]
    fn test_single_quoted_strings() {
        let lua = r#"TestDB = {
            ['name'] = 'hello world',
        }"#;
        let result = parse_saved_variable(lua, "TestDB").unwrap();
        assert_eq!(result["name"], "hello world");
    }

    #[test]
    fn test_escaped_strings() {
        let lua = r#"TestDB = {
            ["path"] = "Interface\\Icons\\Spell_Holy",
        }"#;
        let result = parse_saved_variable(lua, "TestDB").unwrap();
        assert_eq!(result["path"], "Interface\\Icons\\Spell_Holy");
    }

    #[test]
    fn test_chartracker_structure() {
        let lua = r#"CharTrackerDB = {
            ["version"] = "1.0.0",
            ["lastUpdate"] = 1705147800,
            ["characters"] = {
                ["Beispielchar-Eredar"] = {
                    ["basic"] = {
                        ["name"] = "Beispielchar",
                        ["realm"] = "Eredar",
                        ["class"] = "PALADIN",
                        ["race"] = "Human",
                        ["level"] = 90,
                        ["faction"] = "Alliance",
                        ["guild"] = "Meine Gilde",
                    },
                    ["gold"] = 1234567,
                    ["professions"] = {
                        [1] = {
                            ["type"] = "primary",
                            ["name"] = "Mining",
                            ["skill"] = 600,
                            ["max"] = 600,
                        },
                        [2] = {
                            ["type"] = "primary",
                            ["name"] = "Blacksmithing",
                            ["skill"] = 598,
                            ["max"] = 600,
                        },
                    },
                    ["timestamp"] = 1705147800,
                },
            },
        }"#;

        let result = parse_chartracker_db(lua).unwrap();
        assert_eq!(result["version"], "1.0.0");
        assert_eq!(result["lastUpdate"], 1705147800);

        let char = &result["characters"]["Beispielchar-Eredar"];
        assert_eq!(char["basic"]["name"], "Beispielchar");
        assert_eq!(char["basic"]["class"], "PALADIN");
        assert_eq!(char["basic"]["level"], 90);
        assert_eq!(char["gold"], 1234567);

        let professions = char["professions"].as_array().unwrap();
        assert_eq!(professions.len(), 2);
        assert_eq!(professions[0]["name"], "Mining");
        assert_eq!(professions[1]["name"], "Blacksmithing");
    }

    #[test]
    fn test_multiple_variables() {
        let lua = r#"CharTrackerDB = {
            ["version"] = "1.0.0",
        }
        CharTrackerSettings = {
            ["autoSaveInCities"] = true,
        }"#;

        let db = parse_saved_variable(lua, "CharTrackerDB").unwrap();
        assert_eq!(db["version"], "1.0.0");

        let settings = parse_saved_variable(lua, "CharTrackerSettings").unwrap();
        assert_eq!(settings["autoSaveInCities"], true);
    }

    #[test]
    fn test_variable_not_found() {
        let lua = r#"SomeOtherDB = {}"#;
        let result = parse_saved_variable(lua, "CharTrackerDB");
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_table() {
        let lua = r#"TestDB = {}"#;
        let result = parse_saved_variable(lua, "TestDB").unwrap();
        assert!(result.is_object());
        assert_eq!(result.as_object().unwrap().len(), 0);
    }

    #[test]
    fn test_implicit_array_entries() {
        let lua = r#"TestDB = {
            ["currencies"] = {
                {
                    ["id"] = 463446,
                    ["name"] = "Justice Points",
                    ["count"] = 475,
                },
                {
                    ["id"] = 134912,
                    ["name"] = "Ironpaw Token",
                    ["count"] = 1,
                },
            },
        }"#;
        let result = parse_saved_variable(lua, "TestDB").unwrap();
        let currencies = result["currencies"].as_array().unwrap();
        assert_eq!(currencies.len(), 2);
        assert_eq!(currencies[0]["id"], 463446);
        assert_eq!(currencies[0]["name"], "Justice Points");
        assert_eq!(currencies[1]["name"], "Ironpaw Token");
    }

    #[test]
    fn test_implicit_array_with_mixed_types() {
        let lua = r#"TestDB = {
            ["specs"] = {
                {
                    ["id"] = 1,
                    ["name"] = "Spec 0",
                },
                {
                    ["id"] = 2,
                    ["name"] = "Spec 0",
                },
            },
            ["version"] = "1.0.0",
        }"#;
        let result = parse_saved_variable(lua, "TestDB").unwrap();
        let specs = result["specs"].as_array().unwrap();
        assert_eq!(specs.len(), 2);
        assert_eq!(specs[0]["id"], 1);
        assert_eq!(result["version"], "1.0.0");
    }

    #[test]
    fn test_mixed_array_with_nested_objects() {
        let lua = r#"TestDB = {
            ["currencies"] = {
                [1] = {
                    ["id"] = 395,
                    ["name"] = "Justice Points",
                    ["count"] = 3500,
                },
                [2] = {
                    ["id"] = 396,
                    ["name"] = "Valor Points",
                    ["count"] = 980,
                },
            },
        }"#;
        let result = parse_saved_variable(lua, "TestDB").unwrap();
        let currencies = result["currencies"].as_array().unwrap();
        assert_eq!(currencies.len(), 2);
        assert_eq!(currencies[0]["name"], "Justice Points");
        assert_eq!(currencies[1]["name"], "Valor Points");
    }
}
