//! Diagnostic-only: connect to whatever daemon HOME points at and print the
//! kind/title of every row for a fixed set of queries, in order. Not part of
//! the shipped product — used to reproduce/verify the ranking defects in
//! fm/neko-ranking-2 against an isolated, seeded daemon. Safe to delete
//! before merge if unwanted.

use std::time::Duration;

use neko_protocol::{Request, Response};

fn main() {
    let (client, _events) = neko_client::NekoClient::connect(neko_protocol::socket_path());

    // Give the reconnect supervisor a moment to dial in.
    for _ in 0..100 {
        if client.is_connected() {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    if !client.is_connected() {
        eprintln!("could not connect to daemon at {:?}", neko_protocol::socket_path());
        std::process::exit(1);
    }

    let queries = ["sound", "storage", "appearance", "wallpaper", "accessibility", "displays", "bluetooth"];

    for q in queries {
        let request = Request::Search { query: q.to_string(), limit: 10, provider: None };
        let response = futures::executor::block_on(client.request(request));
        match response {
            Ok(Response::SearchResults { items }) => {
                let kinds: Vec<String> = items.iter().map(|i| i.kind.clone()).collect();
                let settings_titles: Vec<&str> = items.iter().filter(|i| i.kind == "settings").map(|i| i.title.as_str()).collect();
                println!("{q:15} kinds={kinds:?} settings_titles={settings_titles:?}");
            }
            other => println!("{q:15} unexpected response: {other:?}"),
        }
    }
}
