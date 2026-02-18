/// Parser for Gargul addon SavedVariables (Gargul.lua).
/// Extracts GDKP session data from GargulDB["GDKP"]["Ledger"].
/// Uses the existing lua_parser for Lua → JSON conversion, then
/// extracts the structured GDKP data from the JSON tree.

use crate::lua_parser;
use serde::{Deserialize, Serialize};
use serde_json::Value;

// ============================================================
// Data Structures (sent to the website API)
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GargulSyncPayload {
    pub sessions: Vec<GdkpSession>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GdkpSession {
    pub id: String,
    pub title: String,
    pub created_at: i64,
    pub locked_at: Option<i64>,
    pub management_cut: Option<f64>,
    pub total_pot: i64,
    pub auctions: Vec<GdkpAuction>,
    pub cuts: Vec<GdkpCut>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GdkpAuction {
    pub id: String,
    pub item_id: i64,
    pub item_link: Option<String>,
    pub item_name: String,
    pub price: i64,
    pub winner_name: String,
    pub winner_realm: String,
    pub winner_class: String,
    pub created_at: i64,
    pub deleted_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GdkpCut {
    pub player_name: String,
    pub player_realm: String,
    pub player_class: String,
    pub amount: f64,
}

// ============================================================
// Parsing
// ============================================================

/// Parse GargulDB from a Gargul.lua SavedVariables file
/// and extract all GDKP sessions from the Ledger.
pub fn parse_gargul_gdkp(file_content: &str) -> Result<GargulSyncPayload, String> {
    // Parse the Lua file into JSON
    let gargul_db = lua_parser::parse_saved_variable(file_content, "GargulDB")
        .map_err(|e| format!("Failed to parse GargulDB: {}", e))?;

    // Navigate to GDKP.Ledger
    let ledger = match gargul_db.get("GDKP").and_then(|g| g.get("Ledger")) {
        Some(l) if l.is_object() => l,
        _ => {
            return Ok(GargulSyncPayload {
                sessions: Vec::new(),
            })
        }
    };

    let ledger_obj = ledger.as_object().unwrap();
    let mut sessions = Vec::new();

    for (session_id, session_val) in ledger_obj {
        match parse_session(session_id, session_val) {
            Ok(session) => sessions.push(session),
            Err(e) => {
                eprintln!("Warning: Failed to parse session {}: {}", session_id, e);
            }
        }
    }

    // Sort by createdAt ascending
    sessions.sort_by_key(|s| s.created_at);

    Ok(GargulSyncPayload { sessions })
}

fn parse_session(session_id: &str, val: &Value) -> Result<GdkpSession, String> {
    let title = val
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("Untitled Session")
        .to_string();

    let created_at = val
        .get("createdAt")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    let locked_at = val.get("lockedAt").and_then(|v| v.as_i64());

    let deleted_at = val.get("deletedAt").and_then(|v| v.as_i64());
    if deleted_at.is_some() {
        return Err("Session is deleted".to_string());
    }

    let management_cut = val.get("managementCut").and_then(|v| v.as_f64());

    // Parse auctions
    let auctions = parse_auctions(val.get("Auctions"));

    // Calculate total pot from non-deleted auctions
    let total_pot: i64 = auctions
        .iter()
        .filter(|a| a.deleted_at.is_none() && a.price > 0)
        .map(|a| a.price)
        .sum();

    // Parse cuts from Pot.Cuts
    let cuts = parse_cuts(val.get("Pot"), val.get("Auctions"));

    Ok(GdkpSession {
        id: val
            .get("ID")
            .and_then(|v| v.as_str())
            .unwrap_or(session_id)
            .to_string(),
        title,
        created_at,
        locked_at,
        management_cut,
        total_pot,
        auctions,
        cuts,
    })
}

fn parse_auctions(auctions_val: Option<&Value>) -> Vec<GdkpAuction> {
    let auctions_obj = match auctions_val.and_then(|v| v.as_object()) {
        Some(obj) => obj,
        None => return Vec::new(),
    };

    let mut result = Vec::new();

    for (auction_id, auction_val) in auctions_obj {
        let item_id = auction_val
            .get("itemID")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        let item_link = auction_val
            .get("itemLink")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let item_name = extract_item_name(item_link.as_deref(), item_id);

        let price = auction_val
            .get("price")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        let created_at = auction_val
            .get("createdAt")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        let deleted_at = auction_val.get("deletedAt").and_then(|v| v.as_i64());

        // Winner info
        let winner = auction_val.get("Winner");
        let winner_name = winner
            .and_then(|w| w.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let winner_realm = winner
            .and_then(|w| w.get("realm"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let winner_class = winner
            .and_then(|w| w.get("class"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        result.push(GdkpAuction {
            id: auction_val
                .get("ID")
                .and_then(|v| v.as_str())
                .unwrap_or(auction_id)
                .to_string(),
            item_id,
            item_link,
            item_name,
            price,
            winner_name,
            winner_realm,
            winner_class,
            created_at,
            deleted_at,
        });
    }

    // Sort by createdAt
    result.sort_by_key(|a| a.created_at);
    result
}

fn parse_cuts(pot_val: Option<&Value>, auctions_val: Option<&Value>) -> Vec<GdkpCut> {
    let cuts_obj = match pot_val
        .and_then(|p| p.get("Cuts"))
        .and_then(|v| v.as_object())
    {
        Some(obj) => obj,
        None => return Vec::new(),
    };

    // Build a map of player-guid -> class from auction data
    let mut player_classes: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();

    if let Some(auctions) = auctions_val.and_then(|v| v.as_object()) {
        for (_, auction) in auctions {
            if let Some(winner) = auction.get("Winner") {
                let guid = winner
                    .get("guid")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let class = winner
                    .get("class")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if !guid.is_empty() && !class.is_empty() {
                    player_classes.insert(guid.to_string(), class.to_string());
                }
            }
            // Also check bidders
            if let Some(bids) = auction.get("Bids").and_then(|v| v.as_object()) {
                for (_, bid_val) in bids {
                    if let Some(bidder) = bid_val.get("Bidder") {
                        let guid = bid_val
                            .get("bidder")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        let class = bidder
                            .get("class")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        if !guid.is_empty() && !class.is_empty() {
                            player_classes.insert(guid.to_string(), class.to_string());
                        }
                    }
                }
            }
        }
    }

    let mut result = Vec::new();

    for (player_guid, amount_val) in cuts_obj {
        let amount = match amount_val.as_f64() {
            Some(a) => a,
            None => continue,
        };

        // player_guid format is "name-realm" (lowercased)
        let (name, realm) = split_player_guid(player_guid);
        let player_class = player_classes
            .get(player_guid)
            .cloned()
            .unwrap_or_default();

        result.push(GdkpCut {
            player_name: capitalize_first(&name),
            player_realm: capitalize_first(&realm),
            player_class,
            amount,
        });
    }

    result.sort_by(|a, b| a.player_name.cmp(&b.player_name));
    result
}

/// Split "name-realm" into (name, realm)
fn split_player_guid(guid: &str) -> (String, String) {
    match guid.split_once('-') {
        Some((name, realm)) => (name.to_string(), realm.to_string()),
        None => (guid.to_string(), String::new()),
    }
}

/// Capitalize first letter: "arthas" -> "Arthas"
fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
        None => String::new(),
    }
}

/// Extract item name from WoW item link: "|cffa335ee|Hitem:19019...|h[Thunderfury]|h|r"
fn extract_item_name(item_link: Option<&str>, item_id: i64) -> String {
    if let Some(link) = item_link {
        // Match the text between [brackets] in the item link
        if let Some(start) = link.find("|h[") {
            if let Some(end) = link[start + 3..].find("]|h") {
                return link[start + 3..start + 3 + end].to_string();
            }
        }
    }
    format!("Item #{}", item_id)
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_item_name() {
        assert_eq!(
            extract_item_name(
                Some("|cffa335ee|Hitem:19019::::::::80:::::|h[Thunderfury, Blessed Blade of the Windseeker]|h|r"),
                19019
            ),
            "Thunderfury, Blessed Blade of the Windseeker"
        );
    }

    #[test]
    fn test_extract_item_name_fallback() {
        assert_eq!(extract_item_name(None, 12345), "Item #12345");
    }

    #[test]
    fn test_split_player_guid() {
        let (name, realm) = split_player_guid("arthas-frostmourne");
        assert_eq!(name, "arthas");
        assert_eq!(realm, "frostmourne");
    }

    #[test]
    fn test_capitalize_first() {
        assert_eq!(capitalize_first("arthas"), "Arthas");
        assert_eq!(capitalize_first(""), "");
        assert_eq!(capitalize_first("a"), "A");
    }

    #[test]
    fn test_parse_empty_gargul() {
        let lua = r#"GargulDB = {
            ["GDKP"] = {
                ["Queue"] = {},
            },
        }"#;
        let result = parse_gargul_gdkp(lua).unwrap();
        assert_eq!(result.sessions.len(), 0);
    }

    #[test]
    fn test_parse_session_with_auctions() {
        let lua = r#"GargulDB = {
            ["GDKP"] = {
                ["Ledger"] = {
                    ["1706792400abc123"] = {
                        ["ID"] = "1706792400abc123",
                        ["title"] = "Test GDKP Run",
                        ["createdAt"] = 1706792400,
                        ["lockedAt"] = 1706799600,
                        ["managementCut"] = 10,
                        ["Auctions"] = {
                            ["1706793000def456"] = {
                                ["ID"] = "1706793000def456",
                                ["itemID"] = 19019,
                                ["itemLink"] = "|cffa335ee|Hitem:19019::::::::80:::::|h[Thunderfury]|h|r",
                                ["price"] = 25000,
                                ["createdAt"] = 1706793000,
                                ["Winner"] = {
                                    ["name"] = "Stabby",
                                    ["realm"] = "Everlook",
                                    ["class"] = "ROGUE",
                                    ["guid"] = "stabby-everlook",
                                },
                                ["CreatedBy"] = {
                                    ["name"] = "Lootmaster",
                                    ["realm"] = "Everlook",
                                    ["class"] = "PALADIN",
                                },
                            },
                        },
                        ["Pot"] = {
                            ["Cuts"] = {
                                ["stabby-everlook"] = 1125.5,
                                ["lootmaster-everlook"] = 1125.5,
                            },
                        },
                    },
                },
                ["Queue"] = {},
            },
        }"#;

        let result = parse_gargul_gdkp(lua).unwrap();
        assert_eq!(result.sessions.len(), 1);

        let session = &result.sessions[0];
        assert_eq!(session.id, "1706792400abc123");
        assert_eq!(session.title, "Test GDKP Run");
        assert_eq!(session.locked_at, Some(1706799600));
        assert_eq!(session.total_pot, 25000);
        assert_eq!(session.auctions.len(), 1);
        assert_eq!(session.cuts.len(), 2);

        let auction = &session.auctions[0];
        assert_eq!(auction.item_name, "Thunderfury");
        assert_eq!(auction.price, 25000);
        assert_eq!(auction.winner_name, "Stabby");
        assert_eq!(auction.winner_class, "ROGUE");

        // Check cuts (sorted by name)
        assert_eq!(session.cuts[0].player_name, "Lootmaster");
        assert_eq!(session.cuts[1].player_name, "Stabby");
    }

    #[test]
    fn test_deleted_sessions_skipped() {
        let lua = r#"GargulDB = {
            ["GDKP"] = {
                ["Ledger"] = {
                    ["deleted_session"] = {
                        ["ID"] = "deleted_session",
                        ["title"] = "Deleted Run",
                        ["createdAt"] = 1706792400,
                        ["deletedAt"] = 1706800000,
                        ["Auctions"] = {},
                    },
                },
                ["Queue"] = {},
            },
        }"#;

        let result = parse_gargul_gdkp(lua).unwrap();
        assert_eq!(result.sessions.len(), 0);
    }

    #[test]
    fn test_deleted_auctions_not_in_pot() {
        let lua = r#"GargulDB = {
            ["GDKP"] = {
                ["Ledger"] = {
                    ["sess1"] = {
                        ["ID"] = "sess1",
                        ["title"] = "Test",
                        ["createdAt"] = 1706792400,
                        ["lockedAt"] = 1706799600,
                        ["Auctions"] = {
                            ["auc1"] = {
                                ["ID"] = "auc1",
                                ["itemID"] = 100,
                                ["price"] = 5000,
                                ["createdAt"] = 1706793000,
                                ["Winner"] = {
                                    ["name"] = "Buyer",
                                    ["realm"] = "Realm",
                                    ["class"] = "WARRIOR",
                                },
                            },
                            ["auc2"] = {
                                ["ID"] = "auc2",
                                ["itemID"] = 200,
                                ["price"] = 3000,
                                ["deletedAt"] = 1706794000,
                                ["createdAt"] = 1706793500,
                                ["Winner"] = {
                                    ["name"] = "Other",
                                    ["realm"] = "Realm",
                                    ["class"] = "MAGE",
                                },
                            },
                        },
                        ["Pot"] = {
                            ["Cuts"] = {},
                        },
                    },
                },
                ["Queue"] = {},
            },
        }"#;

        let result = parse_gargul_gdkp(lua).unwrap();
        let session = &result.sessions[0];
        // Only non-deleted auction counts toward pot
        assert_eq!(session.total_pot, 5000);
        // Both auctions are present (API filters deleted ones)
        assert_eq!(session.auctions.len(), 2);
    }
}
