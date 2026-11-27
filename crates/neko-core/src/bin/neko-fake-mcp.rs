//! `neko-fake-mcp`: a stand-in for an MCP server, for tests and scenes only. It speaks JSON-RPC 2.0, one message per line, on stdin and
//! stdout (docs/knowledge/plugins.md), and answers `initialize` and `tools/list` from a script. It never reaches the network.
//!
//! Arguments (the way a test's manifest passes them):
//!
//! | Argument | Effect |
//! | --- | --- |
//! | `--mode ok` | The default: answers as a healthy server. |
//! | `--mode hang` | Reads everything and answers nothing, until it is killed. |
//! | `--mode crash` | Says something on stderr and exits with code 3 before answering. |
//! | `--mode garbage` | Prints a banner and a line of HTML (not MCP), then exits. |
//! | `--mode refuse` | Answers `initialize` with a JSON-RPC error ("Bad token"). |
//! | `--mode no-list` | Answers `initialize`, then `tools/list` with "method not found". |
//! | `--mode slow-list` | Answers `initialize`, then never answers `tools/list`. |
//! | `--name N`, `--version V` | What it says about itself (`fake-docs`, `1.2.3`). |
//! | `--tools a,b,c` | Its tools (`search,fetch_page,list_spaces`). |
//! | `--pages N` | Lists its tools N at a time, with a cursor. |
//! | `--pidfile F` | Writes its pid to F when it starts. |
//! | `--grandchild F` | Starts a `sleep 600` in a process group of its own and writes its pid to F. |
//! | `--echo-env NAME` | Adds the tool `env_NAME_set` or `env_NAME_unset`, by whether the variable was given. |
//!
//! It also serves as the run guard in tests (`--neko-run-guard`), as the fake CLI does.

use std::io::{BufRead, Write};
use std::time::Duration;

use serde_json::{Value, json};

fn out(v: &Value) {
    let mut o = std::io::stdout().lock();
    let _ = writeln!(o, "{v}");
    let _ = o.flush();
}

fn main() {
    neko_core::guard::run_if_requested();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let get = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    let mode = get("--mode").unwrap_or_else(|| "ok".into());
    let name = get("--name").unwrap_or_else(|| "fake-docs".into());
    let version = get("--version").unwrap_or_else(|| "1.2.3".into());
    let mut tools: Vec<String> = get("--tools").unwrap_or_else(|| "search,fetch_page,list_spaces".into()).split(',').filter(|t| !t.is_empty()).map(str::to_owned).collect();
    if let Some(var) = get("--echo-env") {
        tools.push(format!("env_{var}_{}", if std::env::var_os(&var).is_some_and(|v| !v.is_empty()) { "set" } else { "unset" }));
    }
    let pages: usize = get("--pages").and_then(|p| p.parse().ok()).unwrap_or(0);
    if let Some(f) = get("--pidfile") {
        let _ = std::fs::write(f, std::process::id().to_string());
    }
    if let Some(f) = get("--grandchild") {
        use std::os::unix::process::CommandExt;
        let script = format!("/bin/sleep 600 & echo $! > '{f}.tmp' && mv '{f}.tmp' '{f}'; wait");
        let _ = std::process::Command::new("/bin/sh").arg("-c").arg(script).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).process_group(0).spawn();
    }
    match mode.as_str() {
        "crash" => {
            eprintln!("fatal: the fake server fell over");
            std::process::exit(3);
        }
        "garbage" => {
            println!("Welcome to Fake Docs 2.0!");
            println!("<html><body>this is not MCP</body></html>");
            let _ = std::io::stdout().flush();
            std::thread::sleep(Duration::from_millis(200));
            return;
        }
        _ => {}
    }
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
        let id = v["id"].clone();
        match (v["method"].as_str(), mode.as_str()) {
            (_, "hang") => {}
            (Some("initialize"), "refuse") => out(&json!({"jsonrpc":"2.0","id":id,"error":{"code":-32000,"message":"Bad token"}})),
            (Some("initialize"), _) => out(&json!({"jsonrpc":"2.0","id":id,"result":{"protocolVersion":v["params"]["protocolVersion"],"capabilities":{"tools":{}},"serverInfo":{"name":name,"version":version}}})),
            (Some("tools/list"), "no-list") => out(&json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"Method not found"}})),
            (Some("tools/list"), "slow-list") => {}
            (Some("tools/list"), _) => {
                let start: usize = v["params"]["cursor"].as_str().and_then(|c| c.parse().ok()).unwrap_or(0);
                let end = if pages == 0 { tools.len() } else { (start + pages).min(tools.len()) };
                let list: Vec<Value> = tools[start..end].iter().map(|t| json!({"name":t,"description":format!("The fake {t} tool."),"inputSchema":{"type":"object"}})).collect();
                let mut result = json!({ "tools": list });
                if end < tools.len() {
                    result["nextCursor"] = json!(end.to_string());
                }
                out(&json!({"jsonrpc":"2.0","id":id,"result":result}));
            }
            (Some("ping"), _) => out(&json!({"jsonrpc":"2.0","id":id,"result":{}})),
            _ => {}
        }
    }
}
