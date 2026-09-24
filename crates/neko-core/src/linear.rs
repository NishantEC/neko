//! Read-only Linear intake. Each connection has its own Keychain credential.
use neko_protocol::workbench::{Issue, LinearConnection};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::io::Read;
use std::time::Duration;

fn data(value: Value) -> Result<Value, String> {
    if value
        .get("errors")
        .is_some_and(|v| v.as_array().is_none_or(|v| !v.is_empty()))
    {
        return Err(
            "Linear rejected the query. Check the key's permissions and configured scope.".into(),
        );
    }
    value
        .get("data")
        .filter(|v| v.is_object())
        .cloned()
        .ok_or_else(|| "Linear returned no data".into())
}

fn field(value: &Value, key: &str) -> Result<String, String> {
    value[key]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| format!("Linear response is missing {key}"))
}

fn parse_page(
    value: &Value,
    connection: &LinearConnection,
) -> Result<(Vec<Issue>, Option<String>), String> {
    let page = &value["issues"];
    let nodes = page["nodes"]
        .as_array()
        .ok_or("Linear response is missing issues")?;
    let more = page["pageInfo"]["hasNextPage"]
        .as_bool()
        .ok_or("Linear response is missing pagination")?;
    let cursor = if more {
        Some(field(&page["pageInfo"], "endCursor")?)
    } else {
        None
    };
    let mut items = Vec::new();
    for node in nodes {
        let external_id = field(node, "id")?;
        items.push(Issue {
            id: format!("{}:{external_id}", connection.id),
            connection_id: connection.id.clone(),
            workspace_id: connection.workspace_id.clone(),
            external_id,
            identifier: field(node, "identifier")?,
            title: field(node, "title")?,
            description: bounded_description(node["description"].as_str().unwrap_or("")),
            url: field(node, "url")?,
            priority: node["priority"].as_u64().unwrap_or(0).min(4) as u8,
            updated_at: field(node, "updatedAt")?,
            assigned: true,
        });
    }
    Ok((items, cursor))
}

const SERVICE: &str = "app.neko.linear";
const MAX_DESCRIPTION_BYTES: usize = 32_000;
const DESCRIPTION_TRUNCATION_NOTICE: &str = "\n\n[Neko import truncated: this description is incomplete. Open the source issue for the remaining text.]";
const MAX_ISSUES: usize = 2000;
const MAX_PAGES: usize = 40;
const INBOX_QUERY: &str = "query NekoInbox($filter: IssueFilter, $after: String) { issues(first: 50, after: $after, filter: $filter, orderBy: updatedAt) { nodes { id identifier title description url priority updatedAt } pageInfo { hasNextPage endCursor } } }";
const INTAKE_LIMIT_ERROR: &str = "Linear inbox exceeds the 40-page / 2000-issue intake limit. Narrow the connection's team/project scope.";

fn bounded_description(text: &str) -> String {
    if text.len() <= MAX_DESCRIPTION_BYTES {
        return text.to_owned();
    }
    let mut description = bounded_text(
        text,
        MAX_DESCRIPTION_BYTES - DESCRIPTION_TRUNCATION_NOTICE.len(),
    );
    description.push_str(DESCRIPTION_TRUNCATION_NOTICE);
    description
}

fn bounded_text(text: &str, max: usize) -> String {
    let mut end = text.len().min(max);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}

pub fn store_key(connection_id: &str, key: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        security_framework::passwords::set_generic_password(SERVICE, connection_id, key.as_bytes())
            .map_err(|_| "Could not save the Linear key in macOS Keychain".into())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (connection_id, key);
        Err("Secure Linear credentials require macOS Keychain".into())
    }
}

pub fn remove_key(connection_id: &str) {
    #[cfg(target_os = "macos")]
    {
        let _ = security_framework::passwords::delete_generic_password(SERVICE, connection_id);
    }
}

fn read_key(connection_id: &str) -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        let bytes = security_framework::passwords::get_generic_password(SERVICE, connection_id)
            .map_err(|_| {
                "Linear key is unavailable. Reconnect this account or allow Keychain access."
                    .to_string()
            })?;
        String::from_utf8(bytes).map_err(|_| "Invalid Linear credential".into())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = connection_id;
        Err("Secure Linear credentials require macOS Keychain".into())
    }
}

struct Api {
    client: reqwest::blocking::Client,
    key: reqwest::header::HeaderValue,
}
impl Api {
    fn new(key: &str) -> Result<Self, String> {
        if key.trim().is_empty() || key.len() > 4096 {
            return Err("Enter a Linear personal API key".into());
        }
        let mut header =
            reqwest::header::HeaderValue::from_str(key.trim()).map_err(|_| "Invalid Linear key")?;
        header.set_sensitive(true);
        Ok(Self {
            client: reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(20))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .map_err(|_| "Could not initialize Linear transport")?,
            key: header,
        })
    }
    fn query(&self, query: &str, variables: Value) -> Result<Value, String> {
        let response = self
            .client
            .post("https://api.linear.app/graphql")
            .header(reqwest::header::AUTHORIZATION, self.key.clone())
            .json(&json!({"query":query,"variables":variables}))
            .send()
            .map_err(|_| "Linear could not be reached within 20 seconds".to_string())?;
        if !response.status().is_success() {
            return Err(format!(
                "Linear returned HTTP {}. Check connection permissions or try again later.",
                response.status().as_u16()
            ));
        }
        let mut bytes = Vec::new();
        response
            .take(4_000_001)
            .read_to_end(&mut bytes)
            .map_err(|_| "Could not read Linear response")?;
        if bytes.len() > 4_000_000 {
            return Err("Linear response exceeded the safe size limit".into());
        }
        serde_json::from_slice(&bytes).map_err(|_| "Linear returned invalid JSON".into())
    }
}

/// Validate identity before persisting any account. Never invokes a mutation.
pub fn identity(key: &str) -> Result<(String, String, String), String> {
    let value = data(Api::new(key)?.query(
        "query NekoIdentity { viewer { id } organization { id name } }",
        json!({}),
    )?)?;
    Ok((
        field(&value["organization"], "id")?,
        field(&value["organization"], "name")?,
        field(&value["viewer"], "id")?,
    ))
}

pub fn sync(connection: &LinearConnection) -> Result<Vec<Issue>, String> {
    let api = Api::new(&read_key(&connection.id)?)?;
    collect_issues(connection, |query, variables| api.query(query, variables))
}

/// Collect a complete, bounded inbox or return an error without partial data.
/// The injectable query is pure at this seam: tests provide GraphQL envelopes,
/// while production alone owns credentials, the fixed endpoint and transport.
fn collect_issues(
    connection: &LinearConnection,
    mut query: impl FnMut(&str, Value) -> Result<Value, String>,
) -> Result<Vec<Issue>, String> {
    let mut filter = json!({"assignee":{"id":{"eq":connection.viewer_id}},"state":{"type":{"nin":["completed","canceled"]}}});
    if !connection.team_ids.is_empty() {
        filter["team"] = json!({"id":{"in":connection.team_ids}});
    }
    if !connection.project_ids.is_empty() {
        filter["project"] = json!({"id":{"in":connection.project_ids}});
    }
    let mut cursor: Option<String> = None;
    let mut seen_cursors = HashSet::new();
    let mut items = Vec::new();
    for _ in 0..MAX_PAGES {
        let value = data(query(INBOX_QUERY, json!({"filter":filter,"after":cursor}))?)?;
        let (page, next) = parse_page(&value, connection)?;
        if page.len() > MAX_ISSUES - items.len() {
            return Err(INTAKE_LIMIT_ERROR.into());
        }
        items.extend(page);
        let Some(next) = next else {
            return Ok(items);
        };
        if !seen_cursors.insert(next.clone()) {
            return Err("Linear returned a repeated pagination cursor".into());
        }
        cursor = Some(next);
    }
    Err(INTAKE_LIMIT_ERROR.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn graphql_errors_fail_even_with_partial_data() {
        assert!(
            data(json!({"data":{"issues":{}},"errors":[{"message":"private response"}]})).is_err()
        );
    }
    #[test]
    fn graphql_errors_do_not_echo_remote_content() {
        let error = data(json!({"errors":[{"message":"secret-token"}]})).unwrap_err();
        assert!(!error.contains("secret-token"));
    }
    fn connection() -> LinearConnection {
        LinearConnection {
            id: "c".into(),
            workspace_id: "w".into(),
            name: "A".into(),
            organization_id: "org".into(),
            viewer_id: "me".into(),
            team_ids: vec![],
            project_ids: vec![],
            enabled: true,
            last_sync_ms: None,
            error: None,
            intake_notice: None,
        }
    }
    #[test]
    fn malformed_pagination_fails_instead_of_silently_truncating() {
        assert!(
            parse_page(
                &json!({"issues":{"nodes":[],"pageInfo":{"hasNextPage":true,"endCursor":null}}}),
                &connection()
            )
            .is_err()
        );
    }
    #[test]
    fn issue_identity_is_scoped_to_connection() {
        let value = json!({"issues":{"nodes":[{"id":"i","identifier":"A-1","title":"Fix","description":null,"url":"https://linear.app/a/issue/A-1","priority":2,"updatedAt":"now"}],"pageInfo":{"hasNextPage":false,"endCursor":null}}});
        let (items, cursor) = parse_page(&value, &connection()).unwrap();
        assert_eq!(items[0].id, "c:i");
        assert_eq!(items[0].workspace_id, "w");
        assert!(cursor.is_none());
    }

    fn response(start: usize, count: usize, cursor: Option<&str>) -> Value {
        let nodes: Vec<_> = (start..start + count)
            .map(|i| {
                json!({
                    "id":format!("issue-{i}"), "identifier":format!("ENG-{i}"),
                    "title":"Fix one thing", "description": "Evidence",
                    "url":format!("https://linear.app/example/issue/ENG-{i}"),
                    "priority":2, "updatedAt":"2026-09-24T00:00:00Z"
                })
            })
            .collect();
        json!({"data":{"issues":{"nodes":nodes,"pageInfo":{"hasNextPage":cursor.is_some(),"endCursor":cursor}}}})
    }

    #[test]
    fn collects_multiple_pages_and_carries_explicit_account_scope() {
        let mut connection = connection();
        connection.team_ids = vec!["team-a".into()];
        connection.project_ids = vec!["project-a".into()];
        let mut calls = 0;
        let items = collect_issues(&connection, |query, variables| {
            assert!(query.starts_with("query NekoInbox"));
            assert!(!query.contains("mutation"));
            assert_eq!(variables["filter"]["assignee"]["id"]["eq"], "me");
            assert_eq!(variables["filter"]["team"]["id"]["in"], json!(["team-a"]));
            assert_eq!(
                variables["filter"]["project"]["id"]["in"],
                json!(["project-a"])
            );
            let page = match calls {
                0 => {
                    assert_eq!(variables["after"], Value::Null);
                    response(0, 2, Some("page-two"))
                }
                1 => {
                    assert_eq!(variables["after"], "page-two");
                    response(2, 1, None)
                }
                _ => panic!("Unexpected extra page"),
            };
            calls += 1;
            Ok(page)
        })
        .unwrap();
        assert_eq!(calls, 2);
        assert_eq!(
            items
                .iter()
                .map(|issue| issue.id.as_str())
                .collect::<Vec<_>>(),
            vec!["c:issue-0", "c:issue-1", "c:issue-2"]
        );
        assert!(
            items
                .iter()
                .all(|issue| issue.workspace_id == "w" && issue.connection_id == "c")
        );
    }

    #[test]
    fn cursor_repetition_and_longer_cycles_fail_without_partial_success() {
        for cursors in [vec!["same", "same"], vec!["first", "second", "first"]] {
            let mut calls = 0;
            let error = collect_issues(&connection(), |_, _| {
                let cursor = cursors
                    .get(calls)
                    .expect("Cycle should be detected immediately");
                calls += 1;
                Ok(response(calls, 1, Some(cursor)))
            })
            .unwrap_err();
            assert!(error.contains("repeated pagination cursor"), "{error}");
            assert_eq!(calls, cursors.len());
        }
    }

    #[test]
    fn exactly_2000_issues_are_accepted_only_with_a_complete_last_page() {
        let mut calls = 0;
        let issues = collect_issues(&connection(), |_, _| {
            let start = calls * 50;
            calls += 1;
            let cursor = format!("page-{calls}");
            Ok(response(
                start,
                50,
                if calls == 40 { None } else { Some(&cursor) },
            ))
        });
        assert_eq!(issues.unwrap().len(), 2000);
        assert_eq!(calls, 40);
    }

    #[test]
    fn unfinished_page_40_reports_a_limit_instead_of_returning_2000_as_complete() {
        let mut calls = 0;
        let error = collect_issues(&connection(), |_, _| {
            let start = calls * 50;
            calls += 1;
            Ok(response(start, 50, Some(&format!("page-{calls}"))))
        })
        .unwrap_err();
        assert_eq!(calls, 40);
        assert!(error.contains("2000"), "{error}");
        assert!(error.contains("Narrow"), "{error}");
    }

    #[test]
    fn oversized_terminal_page_cannot_bypass_the_2000_issue_cap() {
        let mut calls = 0;
        let error = collect_issues(&connection(), |_, _| {
            calls += 1;
            Ok(response(0, 2001, None))
        })
        .unwrap_err();
        assert_eq!(calls, 1);
        assert!(error.contains("2000"), "{error}");
    }

    #[test]
    fn malformed_later_page_does_not_return_an_apparently_complete_partial_inbox() {
        let mut calls = 0;
        let error = collect_issues(&connection(), |_, _| {
            calls += 1;
            Ok(if calls == 1 {
                response(0, 1, Some("next"))
            } else {
                json!({"data":{"issues":{"nodes":[],"pageInfo":{"hasNextPage":true}}}})
            })
        })
        .unwrap_err();
        assert_eq!(calls, 2);
        assert!(error.contains("endCursor"), "{error}");
    }

    #[test]
    fn http_200_graphql_error_on_a_later_page_discards_partial_data_and_hides_remote_text() {
        let mut calls = 0;
        let error = collect_issues(&connection(), |_, _| {
            calls += 1;
            if calls == 1 {
                return Ok(response(0, 1, Some("next")));
            }
            let mut partial = response(1, 1, None);
            partial["errors"] = json!([{"message":"private-token-response"}]);
            Ok(partial)
        })
        .unwrap_err();
        assert_eq!(calls, 2);
        assert!(error.contains("rejected the query"), "{error}");
        assert!(!error.contains("private-token-response"));
    }

    #[test]
    fn long_unicode_description_has_an_explicit_notice_inside_the_byte_limit() {
        let original = "猫🦊".repeat(6000);
        let mut value = response(0, 1, None);
        value["data"]["issues"]["nodes"][0]["description"] = json!(original);
        let (issues, _) = parse_page(&value["data"], &connection()).unwrap();
        let description = &issues[0].description;
        assert!(description.len() <= 32_000);
        assert!(
            description.contains("description is incomplete"),
            "Missing truncation notice"
        );
        let prefix = description
            .split("\n\n[Neko import truncated:")
            .next()
            .unwrap();
        assert!(!prefix.is_empty());
        assert!(original.starts_with(prefix));
    }

    #[test]
    fn descriptions_at_the_limit_and_null_are_preserved_without_a_notice() {
        for description in [
            json!("x".repeat(32_000)),
            json!("猫".repeat(10_666)),
            Value::Null,
        ] {
            let mut value = response(0, 1, None);
            value["data"]["issues"]["nodes"][0]["description"] = description.clone();
            let (issues, _) = parse_page(&value["data"], &connection()).unwrap();
            assert_eq!(issues[0].description, description.as_str().unwrap_or(""));
        }
    }
}
