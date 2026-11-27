//! `neko-daemon [--data-dir DIR] [--port N]`: owns one data folder and serves the protocol to screens on this machine.
//!
//! One daemon per folder (an exclusive lock on `daemon.lock`); loopback only; every request carries the per-install token from the
//! folder's `token` file (mode 0600). SIGTERM, SIGINT or SIGHUP shut it down cleanly: connections are closed, the store is
//! checkpointed and `daemon.json` is removed. See docs/architecture.md.

mod briefing;
mod files;
mod folders;
mod hook;
mod http;
mod mcp_server;
mod plugins;
mod rounds;
mod runs;
mod server;
mod subagents;
mod undo;

use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "usage: neko-daemon [--data-dir DIR] [--port N] [--version]\n\
  --data-dir DIR  the data folder (default: $NEKO_DATA_DIR, else ~/Library/Application Support/neko)\n\
  --port N        the loopback port to listen on (default: any free one, written to daemon.json)";

/// Exit code when another daemon already owns the folder.
const EXIT_IN_USE: u8 = 3;

fn default_data_dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("NEKO_DATA_DIR").filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(d));
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support/neko"))
}

fn main() -> ExitCode {
    // As a run's guardian (`--neko-run-guard`), this binary only guards: it never opens the store or the network.
    neko_core::guard::run_if_requested();
    // As a run's `PreToolUse` hook (`--hook`), it asks the daemon about one step and prints the decision.
    if std::env::args().nth(1).as_deref() == Some("--hook") {
        return hook::helper();
    }
    // As a run's own MCP server (`--mcp`), it is Neko's tools for the agent: subagents (docs/knowledge/subagents.md).
    if std::env::args().nth(1).as_deref() == Some("--mcp") {
        return mcp_server::serve();
    }
    let mut data_dir = None;
    let mut port = 0u16;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--data-dir" => data_dir = args.next().map(PathBuf::from),
            "--port" => match args.next().and_then(|p| p.parse().ok()) {
                Some(p) => port = p,
                None => {
                    eprintln!("{USAGE}");
                    return ExitCode::from(2);
                }
            },
            "--version" => {
                println!("neko-daemon {} (protocol {})", env!("CARGO_PKG_VERSION"), neko_protocol::PROTOCOL_VERSION);
                return ExitCode::SUCCESS;
            }
            "--help" | "-h" => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("neko-daemon: unknown argument {other}\n{USAGE}");
                return ExitCode::from(2);
            }
        }
    }
    let Some(data_dir) = data_dir.or_else(default_data_dir) else {
        eprintln!("neko-daemon: no data folder (pass --data-dir)");
        return ExitCode::from(2);
    };
    match server::run(&data_dir, port) {
        Ok(()) => ExitCode::SUCCESS,
        Err(server::StartError::InUse) => {
            eprintln!("neko-daemon: another neko-daemon is running for {}", data_dir.display());
            ExitCode::from(EXIT_IN_USE)
        }
        Err(server::StartError::Other(e)) => {
            eprintln!("neko-daemon: {e}");
            ExitCode::FAILURE
        }
    }
}
