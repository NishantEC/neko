//! The cats' requests (`Request::Cats`, docs/knowledge/cats.md) and what the daemon answers. [`CatCall`] is one request each of
//! `neko_protocol::cats::CatCommand` the cat screens send; the daemon stays the only writer and every write answers with the
//! active list, so a click here and the next snapshot agree. The web UI does not send these yet: they are in
//! `tests/allow_list.rs`'s `RUNE_EXTRAS` with that reason, and `Archive` is built only inside a confirmation (`src/cats.rs`).

use neko_protocol::cats::{Cat, CatCommand, CatFileKind, CatFiles, CatSuggestion, CatsReply, FaceRef, ModelPolicy};
use neko_protocol::Request;

/// One request about cats.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CatCall {
    /// The active cats in order (never the archived: the sidebar has no place for them).
    List,
    Create { name: String, face: String, workspace_id: String, area: String, model_policy: ModelPolicy, daily_limit_micro_usd: Option<u64>, purpose: String },
    /// Only what changed; `daily_limit_micro_usd: Some(None)` removes the limit and `None` leaves it.
    Update { id: String, name: Option<String>, face: Option<String>, area: Option<String>, model_policy: Option<ModelPolicy>, daily_limit_micro_usd: Option<Option<u64>>, purpose: Option<String> },
    /// Hides the cat and unpins it (its files and tasks stay). Only built inside a confirm dialog.
    Archive { id: String },
    Pin { id: String, pinned: bool },
    /// The new order of every active cat.
    Reorder { ids: Vec<String> },
    /// A proposed first cat from the busiest workspace (a read).
    Suggest,
    /// The four files of one cat.
    Files { id: String },
    /// Replaces one file; empty text clears it (the editor asks first).
    WriteFile { id: String, kind: CatFileKind, text: String },
}

/// What came back from a cat request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CatsData {
    Listing(Vec<Cat>),
    Suggestion(CatSuggestion),
    Files(CatFiles),
}

impl CatsData {
    /// A daemon answer as the small thing the app applies; the cat's own thread (not asked for here) is nothing.
    pub fn from_reply(reply: CatsReply) -> Option<CatsData> {
        match reply {
            CatsReply::Listing(cats) => Some(CatsData::Listing(cats)),
            CatsReply::Suggestion(s) => Some(CatsData::Suggestion(s)),
            CatsReply::Files(f) => Some(CatsData::Files(f)),
            CatsReply::Thread(_) => None,
        }
    }
}

impl CatCall {
    /// Whether this only fetches.
    pub fn reads(&self) -> bool {
        matches!(self, CatCall::List | CatCall::Suggest | CatCall::Files { .. })
    }

    /// The request the daemon receives.
    pub fn wire(&self) -> Request {
        Request::Cats(match self.clone() {
            CatCall::List => CatCommand::List { include_archived: false },
            CatCall::Create { name, face, workspace_id, area, model_policy, daily_limit_micro_usd, purpose } => CatCommand::Create { name, face: FaceRef(face), workspace_id, area, model_policy, daily_limit_micro_usd, purpose },
            CatCall::Update { id, name, face, area, model_policy, daily_limit_micro_usd, purpose } => CatCommand::Update { id, name, face: face.map(FaceRef), area, model_policy, daily_limit_micro_usd, purpose },
            CatCall::Archive { id } => CatCommand::Archive { id },
            CatCall::Pin { id, pinned } => CatCommand::Pin { id, pinned },
            CatCall::Reorder { ids } => CatCommand::Reorder { ids },
            CatCall::Suggest => CatCommand::Suggest,
            CatCall::Files { id } => CatCommand::Files { id },
            CatCall::WriteFile { id, kind, text } => CatCommand::WriteFile { id, kind, text },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json(call: &CatCall) -> serde_json::Value {
        serde_json::to_value(call.wire()).unwrap()
    }

    #[test]
    fn each_call_is_the_daemons_cat_command() {
        assert_eq!(json(&CatCall::List), serde_json::json!({"Cats": {"List": {"include_archived": false}}}));
        assert_eq!(json(&CatCall::Suggest), serde_json::json!({"Cats": "Suggest"}));
        assert_eq!(json(&CatCall::Pin { id: "c1".into(), pinned: true }), serde_json::json!({"Cats": {"Pin": {"id": "c1", "pinned": true}}}));
        assert_eq!(json(&CatCall::Reorder { ids: vec!["b".into(), "a".into()] }), serde_json::json!({"Cats": {"Reorder": {"ids": ["b", "a"]}}}));
        assert_eq!(json(&CatCall::Archive { id: "c1".into() }), serde_json::json!({"Cats": {"Archive": {"id": "c1"}}}));
        let write = json(&CatCall::WriteFile { id: "c1".into(), kind: CatFileKind::TaskRules, text: "# Task rules".into() });
        assert_eq!(write, serde_json::json!({"Cats": {"WriteFile": {"id": "c1", "kind": "TaskRules", "text": "# Task rules"}}}));
        assert_eq!(json(&CatCall::Files { id: "c1".into() }), serde_json::json!({"Cats": {"Files": {"id": "c1"}}}));
    }

    #[test]
    fn create_carries_every_field_and_an_update_only_the_changed_ones() {
        let create = json(&CatCall::Create { name: "miso".into(), face: "black-glasses".into(), workspace_id: "w1".into(), area: "Web".into(), model_policy: ModelPolicy::default(), daily_limit_micro_usd: Some(5_000_000), purpose: "Look after the web app.".into() });
        let c = &create["Cats"]["Create"];
        assert_eq!((c["name"].as_str(), c["face"].as_str(), c["workspace_id"].as_str(), c["area"].as_str()), (Some("miso"), Some("black-glasses"), Some("w1"), Some("Web")));
        assert_eq!((c["daily_limit_micro_usd"].as_u64(), c["purpose"].as_str()), (Some(5_000_000), Some("Look after the web app.")));
        // An update that changes the area only: the daemon reads a missing field as "leave it", and `null` as "remove the limit", so an
        // unchanged limit must not be written at all (the protocol skips it; a `null` here would silently clear the person's limit).
        let one = CatCall::Update { id: "c1".into(), name: None, face: None, area: Some("Mobile".into()), model_policy: None, daily_limit_micro_usd: None, purpose: None };
        let text = serde_json::to_string(&one.wire()).unwrap();
        assert!(!text.contains("daily_limit"), "{text}");
        let back: Request = serde_json::from_str(&text).unwrap();
        match back {
            Request::Cats(CatCommand::Update { area, name, daily_limit_micro_usd, .. }) => assert_eq!((area.as_deref(), name, daily_limit_micro_usd), (Some("Mobile"), None, None)),
            other => panic!("{other:?}"),
        }
        let clear = CatCall::Update { id: "c1".into(), name: None, face: None, area: None, model_policy: None, daily_limit_micro_usd: Some(None), purpose: None };
        let back: Request = serde_json::from_str(&serde_json::to_string(&clear.wire()).unwrap()).unwrap();
        match back {
            Request::Cats(CatCommand::Update { daily_limit_micro_usd, .. }) => assert_eq!(daily_limit_micro_usd, Some(None)),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn only_the_three_reads_are_reads() {
        assert!(CatCall::List.reads() && CatCall::Suggest.reads() && CatCall::Files { id: "c".into() }.reads());
        assert!(!CatCall::Archive { id: "c".into() }.reads() && !CatCall::Pin { id: "c".into(), pinned: true }.reads() && !CatCall::WriteFile { id: "c".into(), kind: CatFileKind::Rules, text: String::new() }.reads());
    }
}
