//! MCP server (US-MCP-01): expose the hub to AI agents over stdio.
//!
//! `tui-op-hub mcp` speaks newline-delimited JSON-RPC 2.0 (the MCP stdio
//! transport) and offers the hub's core surfaces as MCP tools:
//! `query_entities` (FTS5 search), `get_secret` (decrypt by name — the
//! localhost-trust contract, never logged), and workflow/lookup helpers as
//! they land. Hand-rolled on serde_json: no extra dependencies, and the
//! binary stays a single static TUI+service+mcp build.

use std::io::{BufRead, Write};
use std::sync::Arc;

use serde_json::{json, Value};
use sqlx::SqlitePool;

/// Serve MCP over stdin/stdout until EOF.
pub async fn run(pool: Arc<SqlitePool>) -> anyhow::Result<()> {
    let stdin = std::io::stdin();
    let mut out = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let msg: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => {
                // unparseable line — respond with a parse error so clients hang up gracefully
                let resp = json!({
                    "jsonrpc": "2.0", "id": Value::Null,
                    "error": {"code": -32700, "message": "parse error"}
                });
                writeln!(out, "{}", resp)?;
                out.flush()?;
                continue;
            }
        };
        if let Some(resp) = dispatch_request(&pool, &msg).await {
            writeln!(out, "{}", resp)?;
            out.flush()?;
        }
    }
    Ok(())
}

/// Handle one JSON-RPC message. Notifications (no id) return None.
async fn dispatch_request(pool: &Arc<SqlitePool>, msg: &Value) -> Option<Value> {
    let id = msg.get("id").cloned();
    let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let params = msg.get("params").cloned().unwrap_or(Value::Null);

    // notifications carry no id — no response frame
    let id = id?;

    let result = match method {
        "initialize" => Ok(json!({
            "protocolVersion": "2024-11-05",
            "serverInfo": {"name": "tui-op-hub", "version": env!("CARGO_PKG_VERSION")},
            "capabilities": {"tools": {}}
        })),
        "notifications/initialized" | "initialized" => return None,
        "tools/list" => Ok(tools_list()),
        "tools/call" => tools_call(pool, &params).await,
        other => Err(json!({
            "code": -32601, "message": format!("method not found: {other}")
        })),
    };

    Some(match result {
        Ok(v) => json!({"jsonrpc": "2.0", "id": id, "result": v}),
        Err(err) => json!({"jsonrpc": "2.0", "id": id, "error": err}),
    })
}

fn tools_list() -> Value {
    json!({
        "tools": [
            {
                "name": "query_entities",
                "description": "Full-text search the hub's command and knowledge entities",
                "inputSchema": {
                    "type": "object",
                    "properties": {"query": {"type": "string", "description": "search terms"}},
                    "required": ["query"]
                }
            },
            {
                "name": "get_secret",
                "description": "Fetch a secret by exact name and return its decrypted value (localhost trust — do not relay secrets)",
                "inputSchema": {
                    "type": "object",
                    "properties": {"name": {"type": "string", "description": "exact secret name, e.g. ssh-pass:mykey"}},
                    "required": ["name"]
                }
            }
        ]
    })
}

async fn tools_call(pool: &Arc<SqlitePool>, params: &Value) -> Result<Value, Value> {
    let name = params
        .get("name")
        .and_then(|n| n.as_str())
        .unwrap_or_default();
    let args = params.get("arguments").cloned().unwrap_or(Value::Null);
    let text = match name {
        "query_entities" => {
            let query = args.get("query").and_then(|q| q.as_str()).unwrap_or("");
            match crate::repository::search_entities(pool, query).await {
                Ok(rows) => serde_json::to_string(&rows)
                    .map_err(|e| json!({"code": -32603, "message": e.to_string()}))?,
                Err(e) => return Err(json!({"code": -32603, "message": e.to_string()})),
            }
        }
        "get_secret" => {
            let secret_name = args.get("name").and_then(|n| n.as_str()).unwrap_or("");
            match resolve_secret(pool, secret_name).await {
                Ok(Some(value)) => value,
                Ok(None) => {
                    return Err(json!({"code": -32000, "message": format!("secret not found: {secret_name}")}))
                }
                Err(e) => return Err(json!({"code": -32603, "message": e.to_string()})),
            }
        }
        other => return Err(json!({"code": -32601, "message": format!("unknown tool: {other}")})),
    };
    Ok(json!({"content": [{"type": "text", "text": text}]}))
}

/// Resolve a secret by name for the default user and decrypt it.
async fn resolve_secret(pool: &Arc<SqlitePool>, name: &str) -> Result<Option<String>, anyhow::Error> {
    let uid = crate::auth::first_user_id(pool)
        .await?
        .ok_or_else(|| anyhow::anyhow!("no users in the vault"))?;
    match crate::repository::get_secret_by_name(pool, &uid, name).await? {
        Some(s) => {
            let val = crate::secrets::decrypt_for_user(pool, &s.user_id, &s.value_enc).await?;
            Ok(Some(val))
        }
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // encryption needs a process key — mirror workflow.rs tests
    // Base64 of 32 'a' bytes — same test key workflow.rs uses
    const TEST_KEY: &str = "YWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWE=";

    async fn test_pool() -> Arc<SqlitePool> {
        std::env::set_var("TUI_OP_HUB_SECRETS_KEY", TEST_KEY);
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        crate::db::run_migrations(&pool).await.unwrap();
        Arc::new(pool)
    }

    fn rpc(id: i64, method: &str, params: Value) -> Value {
        json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
    }

    #[tokio::test]
    async fn given_initialize_when_dispatched_then_capabilities_present() {
        let pool = test_pool().await;
        let resp = dispatch_request(&pool, &rpc(1, "initialize", json!({})))
            .await
            .unwrap();
        assert_eq!(resp["id"], 1);
        assert_eq!(resp["result"]["serverInfo"]["name"], "tui-op-hub");
        assert!(resp["result"]["capabilities"]["tools"].is_object());
    }

    #[tokio::test]
    async fn given_notification_when_dispatched_then_no_response() {
        let pool = test_pool().await;
        let msg = json!({"jsonrpc": "2.0", "method": "notifications/initialized"});
        assert!(dispatch_request(&pool, &msg).await.is_none());
    }

    #[tokio::test]
    async fn given_tools_list_when_dispatched_then_core_tools_listed() {
        let pool = test_pool().await;
        let resp = dispatch_request(&pool, &rpc(2, "tools/list", json!({})))
            .await
            .unwrap();
        let names: Vec<&str> = resp["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"query_entities"));
        assert!(names.contains(&"get_secret"));
    }

    #[tokio::test]
    async fn given_unknown_tool_when_called_then_error_result() {
        let pool = test_pool().await;
        let resp = dispatch_request(
            &pool,
            &rpc(3, "tools/call", json!({"name": "does_not_exist", "arguments": {}})),
        )
        .await
        .unwrap();
        assert!(resp["error"].is_object());
    }

    #[tokio::test]
    async fn given_stored_secret_when_get_secret_called_then_plaintext_returned() {
        let pool = test_pool().await;
        let uid = crate::repository::get_or_create_user(&pool, "tester")
            .await
            .unwrap()
            .id;
        let enc = crate::secrets::encrypt_for_user(&pool, &uid, "hunter2")
            .await
            .unwrap();
        crate::repository::create_secret_full(&pool, &uid, "ssh-pass:gh", &enc, "ssh_key", false)
            .await
            .unwrap();

        let resp = dispatch_request(
            &pool,
            &rpc(4, "tools/call", json!({"name": "get_secret", "arguments": {"name": "ssh-pass:gh"}})),
        )
        .await
        .unwrap();
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert_eq!(text, "hunter2");
    }

    #[tokio::test]
    async fn given_missing_secret_when_get_secret_called_then_error() {
        let pool = test_pool().await;
        let resp = dispatch_request(
            &pool,
            &rpc(5, "tools/call", json!({"name": "get_secret", "arguments": {"name": "nope"}})),
        )
        .await
        .unwrap();
        assert!(resp["error"].is_object());
    }
}
