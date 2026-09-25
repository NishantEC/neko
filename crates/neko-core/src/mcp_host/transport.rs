//! Bounded synchronous facade over the official MCP SDK.
use neko_protocol::mcp_host::{McpTool, ServerConfig};
use std::{collections::BTreeMap, sync::atomic::AtomicBool};

#[derive(Default)]
pub struct Credentials {
    pub bearer: Option<String>,
    pub environment: BTreeMap<String, String>,
}
impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Credentials { [redacted] }")
    }
}
pub fn discover(
    config: &ServerConfig,
    credentials: &Credentials,
    cancel: &AtomicBool,
) -> Result<Vec<McpTool>, String> {
    match run(config, credentials, Operation::Discover, cancel, &|| true)? {
        Output::Tools(tools) => Ok(tools),
        _ => unreachable!(),
    }
}
pub fn call(
    config: &ServerConfig,
    credentials: &Credentials,
    tool: &str,
    expected_schema_hash: &str,
    arguments_json: &str,
    cancel: &AtomicBool,
) -> Result<String, String> {
    call_guarded(
        config,
        credentials,
        tool,
        expected_schema_hash,
        arguments_json,
        cancel,
        &|| true,
    )
}
pub fn call_guarded(
    config: &ServerConfig,
    credentials: &Credentials,
    tool: &str,
    expected_schema_hash: &str,
    arguments_json: &str,
    cancel: &AtomicBool,
    authorized: &(dyn Fn() -> bool + Sync),
) -> Result<String, String> {
    if arguments_json.len() > MAX_BYTES {
        return Err("MCP arguments exceed limit".into());
    }
    let arguments =
        serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(arguments_json)
            .map_err(|_| "MCP arguments must be a JSON object")?;
    match run(
        config,
        credentials,
        Operation::Call(tool.to_owned(), expected_schema_hash.to_owned(), arguments),
        cancel,
        authorized,
    )? {
        Output::Result(result) => Ok(result),
        _ => unreachable!(),
    }
}

use rmcp::{
    RoleClient, ServiceExt,
    service::RunningService,
    transport::{
        StreamableHttpClientTransport, Transport, common::client_side_sse::NeverRetry,
        streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use sha2::{Digest, Sha256};
use std::{
    future::Future,
    path::Path,
    pin::Pin,
    sync::{Arc, atomic::Ordering},
    task::{Context, Poll},
    time::Duration,
};
use tokio::io::{AsyncRead, ReadBuf};
const MAX_BYTES: usize = 256 * 1024;
const MAX_SCHEMA: usize = 32 * 1024;
const TIMEOUT: Duration = Duration::from_secs(20);
enum Operation {
    Discover,
    Call(String, String, serde_json::Map<String, serde_json::Value>),
}
enum Output {
    Tools(Vec<McpTool>),
    Result(String),
}

fn validate(config: &ServerConfig) -> Result<(), String> {
    match config {
        ServerConfig::Stdio { command, .. } if !Path::new(command).is_absolute() => {
            Err("MCP executable must be an absolute path".into())
        }
        ServerConfig::Http { url } => {
            let url = reqwest_mcp::Url::parse(url).map_err(|_| "Invalid MCP URL")?;
            let loopback = url.host_str().is_some_and(|host| {
                host == "localhost"
                    || host
                        .trim_matches(['[', ']'])
                        .parse::<std::net::IpAddr>()
                        .is_ok_and(|ip| ip.is_loopback())
            });
            if !url.username().is_empty()
                || url.password().is_some()
                || url.fragment().is_some()
                || !(url.scheme() == "https" || (url.scheme() == "http" && loopback))
            {
                return Err("MCP URL requires HTTPS (HTTP is allowed only on explicit loopback); embedded credentials are forbidden".into());
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

async fn bounded<T>(
    future: impl Future<Output = Result<T, String>>,
    cancel: &AtomicBool,
) -> Result<T, String> {
    bounded_with_timeout(future, cancel, TIMEOUT).await
}

async fn bounded_with_timeout<T>(
    future: impl Future<Output = Result<T, String>>,
    cancel: &AtomicBool,
    timeout: Duration,
) -> Result<T, String> {
    tokio::select! {
        result = tokio::time::timeout(timeout, future) => result.map_err(|_| "MCP operation timed out".to_owned())?,
        _ = async { loop { if cancel.load(Ordering::Acquire) { break; } tokio::time::sleep(Duration::from_millis(20)).await; } } => Err("MCP operation cancelled".into()),
    }
}

fn run(
    config: &ServerConfig,
    credentials: &Credentials,
    operation: Operation,
    cancel: &AtomicBool,
    authorized: &(dyn Fn() -> bool + Sync),
) -> Result<Output, String> {
    if cancel.load(Ordering::Acquire) {
        return Err("MCP operation cancelled".into());
    }
    validate(config)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| "MCP runtime unavailable")?;
    runtime.block_on(async {
        match config {
            ServerConfig::Stdio { command, args } => {
                let mut cmd = tokio::process::Command::new(command);
                cmd.args(args)
                    .env_clear()
                    .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
                    .envs(&credentials.environment)
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::null())
                    .kill_on_drop(true);
                #[cfg(unix)]
                cmd.process_group(0);
                let mut child = cmd.spawn().map_err(|_| "MCP server could not be started")?;
                let process_group = ProcessGroup(child.id());
                let stdout = FrameBounded {
                    inner: child.stdout.take().ok_or("MCP stdout unavailable")?,
                    length: 0,
                };
                let stdin = child.stdin.take().ok_or("MCP stdin unavailable")?;
                let transport =
                    rmcp::transport::async_rw::AsyncRwTransport::new_client(stdout, stdin);
                let result =
                    bounded(session(transport, operation, cancel, authorized), cancel).await;
                drop(process_group);
                let _ = child.kill().await;
                let _ = child.wait().await;
                result
            }
            ServerConfig::Http { url } => {
                let client = reqwest_mcp::Client::builder()
                    .redirect(reqwest_mcp::redirect::Policy::none())
                    .retry(reqwest_mcp::retry::never())
                    .timeout(TIMEOUT)
                    .build()
                    .map_err(|_| "MCP HTTP client unavailable")?;
                let mut options = StreamableHttpClientTransportConfig::with_uri(url.clone())
                    .reinit_on_expired_session(false)
                    .max_sse_event_size(MAX_BYTES);
                options.retry_config = Arc::new(NeverRetry::default());
                options.auth_header = credentials.bearer.clone();
                bounded(
                    session(
                        StreamableHttpClientTransport::with_client(BoundedHttp(client), options),
                        operation,
                        cancel,
                        authorized,
                    ),
                    cancel,
                )
                .await
            }
        }
    })
}

// rmcp 3.4.1 bounds SSE events but its reqwest adapter buffers JSON and
// HTTP error bodies without a size limit. Override POST to bound both before
// deserialization; keep the official SDK's protocol/session implementation.
#[derive(Clone, Default)]
struct BoundedHttp(reqwest_mcp::Client);
use reqwest_mcp::header::{HeaderName, HeaderValue};
use rmcp::{
    model::{ClientJsonRpcMessage, ServerJsonRpcMessage},
    transport::streamable_http_client::{
        StreamableHttpClient, StreamableHttpError, StreamableHttpPostResponse,
    },
};
type Headers = std::collections::HashMap<HeaderName, HeaderValue>;
type HttpError = StreamableHttpError<reqwest_mcp::Error>;
type SseEvents = futures::stream::BoxStream<'static, Result<sse_stream::Sse, sse_stream::Error>>;
fn http_error(message: &'static str) -> HttpError {
    StreamableHttpError::UnexpectedServerResponse(message.into())
}

impl StreamableHttpClient for BoundedHttp {
    type Error = reqwest_mcp::Error;
    async fn post_message(
        &self,
        uri: Arc<str>,
        message: ClientJsonRpcMessage,
        session_id: Option<Arc<str>>,
        auth: Option<String>,
        headers: Headers,
    ) -> Result<StreamableHttpPostResponse, HttpError> {
        self.post_message_with_max_sse_event_size(
            uri, message, session_id, auth, headers, MAX_BYTES,
        )
        .await
    }
    async fn post_message_with_max_sse_event_size(
        &self,
        uri: Arc<str>,
        message: ClientJsonRpcMessage,
        session_id: Option<Arc<str>>,
        auth: Option<String>,
        headers: Headers,
        max_size: usize,
    ) -> Result<StreamableHttpPostResponse, HttpError> {
        use futures::StreamExt;
        let max_size = max_size.min(MAX_BYTES);
        let attached = session_id.is_some();
        let mut request = self
            .0
            .post(uri.as_ref())
            .header("accept", "application/json, text/event-stream");
        for (name, value) in headers {
            if matches!(
                name.as_str(),
                "authorization"
                    | "mcp-session-id"
                    | "last-event-id"
                    | "accept"
                    | "content-type"
                    | "host"
                    | "content-length"
            ) {
                return Err(http_error("Reserved MCP HTTP header"));
            }
            request = request.header(name, value);
        }
        if let Some(auth) = auth {
            request = request.bearer_auth(auth);
        }
        if let Some(session) = session_id {
            request = request.header("mcp-session-id", session.as_ref());
        }
        let mut response = request
            .json(&message)
            .send()
            .await
            .map_err(|_| http_error("MCP HTTP request failed"))?;
        let status = response.status();
        if status.as_u16() == 404 && attached {
            return Err(StreamableHttpError::SessionExpired);
        }
        if status.is_redirection() || matches!(status.as_u16(), 401 | 403) {
            return Err(http_error("MCP HTTP access failed"));
        }
        if matches!(status.as_u16(), 202 | 204) {
            return Ok(StreamableHttpPostResponse::Accepted);
        }
        let session = response
            .headers()
            .get("mcp-session-id")
            .and_then(|s| s.to_str().ok())
            .map(str::to_owned);
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|s| s.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        if response
            .content_length()
            .is_some_and(|size| size > max_size as u64)
        {
            return Err(http_error("MCP HTTP body exceeds limit"));
        }
        if status.is_success() && content_type.starts_with("text/event-stream") {
            // Each POST response is bounded as a whole (stricter than one SSE
            // event), including comments and whitespace before any event.
            let mut consumed = 0usize;
            let bytes = response.bytes_stream().map(move |chunk| {
                let chunk = chunk.map_err(|_| std::io::Error::other("MCP HTTP body failed"))?;
                consumed = consumed.saturating_add(chunk.len());
                if consumed > max_size {
                    return Err(std::io::Error::other("MCP HTTP body exceeds limit"));
                }
                Ok(chunk)
            });
            return Ok(StreamableHttpPostResponse::Sse(
                sse_stream::SseStream::from_bytes_stream(bytes).boxed(),
                session,
            ));
        }
        let mut body = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| http_error("MCP HTTP body failed"))?
        {
            if body.len().saturating_add(chunk.len()) > max_size {
                return Err(http_error("MCP HTTP body exceeds limit"));
            }
            body.extend_from_slice(&chunk);
        }
        // Older Streamable HTTP servers reject the SDK's modern discover
        // request with HTTP 4xx. Preserve the SDK's fallback to initialize.
        if !attached && status.is_client_error() {
            if let ClientJsonRpcMessage::Request(request) = &message {
                if matches!(
                    request.request,
                    rmcp::model::ClientRequest::DiscoverRequest(_)
                ) {
                    return Ok(StreamableHttpPostResponse::Json(
                        ServerJsonRpcMessage::error(
                            rmcp::model::ErrorData::invalid_request(
                                "MCP discovery unsupported",
                                None,
                            ),
                            Some(request.id.clone()),
                        ),
                        None,
                    ));
                }
            }
        }
        if content_type.starts_with("application/json") {
            if let Ok(parsed) = serde_json::from_slice::<ServerJsonRpcMessage>(&body) {
                if status.is_success() || matches!(parsed, ServerJsonRpcMessage::Error(_)) {
                    return Ok(StreamableHttpPostResponse::Json(parsed, session));
                }
                return Err(http_error("MCP HTTP request failed"));
            }
        }
        if status.is_success() && !matches!(message, ClientJsonRpcMessage::Request(_)) {
            return Ok(StreamableHttpPostResponse::Accepted);
        }
        Err(http_error("MCP HTTP response invalid"))
    }
    async fn delete_session(
        &self,
        uri: Arc<str>,
        session: Arc<str>,
        auth: Option<String>,
        headers: Headers,
    ) -> Result<(), HttpError> {
        self.0
            .delete_session(uri, session, auth, headers)
            .await
            .map_err(|_| http_error("MCP session cleanup failed"))
    }
    async fn get_stream(
        &self,
        uri: Arc<str>,
        session: Option<Arc<str>>,
        last_event: Option<String>,
        auth: Option<String>,
        headers: Headers,
    ) -> Result<SseEvents, HttpError> {
        self.get_stream_with_max_sse_event_size(uri, session, last_event, auth, headers, MAX_BYTES)
            .await
    }
    async fn get_stream_with_max_sse_event_size(
        &self,
        uri: Arc<str>,
        session: Option<Arc<str>>,
        last_event: Option<String>,
        auth: Option<String>,
        headers: Headers,
        max_size: usize,
    ) -> Result<SseEvents, HttpError> {
        self.0
            .get_stream_with_max_sse_event_size(
                uri,
                session,
                last_event,
                auth,
                headers,
                max_size.min(MAX_BYTES),
            )
            .await
    }
}

// Kill the whole isolated process group, including server descendants. The
// Tokio child is then explicitly waited, preventing zombies on normal exit.
struct ProcessGroup(Option<u32>);
impl Drop for ProcessGroup {
    fn drop(&mut self) {
        #[cfg(unix)]
        if let Some(pid) = self.0 {
            unsafe {
                libc::kill(-(pid as i32), libc::SIGKILL);
            }
        }
    }
}

// Bound each raw stdio JSON frame before the SDK can accumulate/deserialize it.
pub(super) struct FrameBounded<R> {
    inner: R,
    length: usize,
}
impl<R> FrameBounded<R> {
    pub(super) fn new(inner: R) -> Self {
        Self { inner, length: 0 }
    }
}
impl<R: AsyncRead + Unpin> AsyncRead for FrameBounded<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let before = buf.filled().len();
        match Pin::new(&mut self.inner).poll_read(cx, buf) {
            Poll::Ready(Ok(())) => {
                for &byte in &buf.filled()[before..] {
                    self.length += 1;
                    if self.length > MAX_BYTES {
                        // AsyncRead must not expose newly filled bytes on an
                        // error; read_to_end and the SDK rely on that contract.
                        buf.set_filled(before);
                        return Poll::Ready(Err(std::io::Error::other("MCP frame exceeds limit")));
                    }
                    if byte == b'\n' {
                        self.length = 0;
                    }
                }
                Poll::Ready(Ok(()))
            }
            result => result,
        }
    }
}

async fn session<T: Transport<RoleClient> + 'static>(
    transport: T,
    operation: Operation,
    cancel: &AtomicBool,
    authorized: &(dyn Fn() -> bool + Sync),
) -> Result<Output, String> {
    let service = ().serve(transport).await.map_err(|_| "MCP handshake failed")?;
    let result = operate(&service, operation, cancel, authorized).await;
    let _ = service.cancel().await;
    result
}

async fn operate(
    service: &RunningService<RoleClient, ()>,
    operation: Operation,
    cancel: &AtomicBool,
    authorized: &(dyn Fn() -> bool + Sync),
) -> Result<Output, String> {
    match operation {
        Operation::Discover => {
            let mut cursor = None;
            let mut seen = std::collections::HashSet::new();
            let mut tools = Vec::new();
            let mut total = 0;
            for _ in 0..16 {
                let params = cursor.map(|cursor| {
                    let mut params = rmcp::model::PaginatedRequestParams::default();
                    params.cursor = Some(cursor);
                    params
                });
                let page = service
                    .list_tools(params)
                    .await
                    .map_err(|_| "MCP tool discovery failed")?;
                for tool in page.tools {
                    let input_schema = serde_json::to_string(&tool.input_schema)
                        .map_err(|_| "Invalid MCP tool schema")?;
                    let name = tool.name.to_string();
                    let description = tool.description.map(|d| d.into_owned()).unwrap_or_default();
                    total += name.len() + description.len() + input_schema.len();
                    if tools.len() >= 128
                        || input_schema.len() > MAX_SCHEMA
                        || description.len() > 8 * 1024
                        || total > MAX_BYTES
                    {
                        return Err("MCP tool discovery exceeds limit".into());
                    }
                    if !seen.insert(name.clone()) {
                        return Err("MCP server returned duplicate tool names".into());
                    }
                    let read_only = tool.annotations.as_ref().and_then(|a| a.read_only_hint).unwrap_or(false);
                    // Preserve existing grants for conservative/unknown tools;
                    // declaring read-only changes identity and needs a regrant.
                    let identity = if read_only {
                        serde_json::to_vec(&(&name, &description, &input_schema, true))
                    } else {
                        serde_json::to_vec(&(&name, &description, &input_schema))
                    }.map_err(|_| "Invalid MCP tool")?;
                    let schema_hash = format!("{:x}", Sha256::digest(identity));
                    tools.push(McpTool {
                        read_only,
                        name,
                        description,
                        input_schema,
                        schema_hash,
                    });
                }
                cursor = page.next_cursor;
                if cursor.is_none() {
                    return Ok(Output::Tools(tools));
                }
            }
            Err("MCP tool discovery exceeds page limit".into())
        }
        Operation::Call(name, expected_schema_hash, arguments) => {
            let Output::Tools(tools) =
                Box::pin(operate(service, Operation::Discover, cancel, authorized)).await?
            else {
                unreachable!()
            };
            if !tools
                .iter()
                .any(|tool| tool.name == name && tool.schema_hash == expected_schema_hash)
            {
                return Err("MCP tool schema changed or tool is unavailable; rediscover and approve it again".into());
            }
            if cancel.load(Ordering::Acquire) {
                return Err("MCP operation cancelled".into());
            }
            if !authorized() {
                return Err("MCP permission revoked before dispatch".into());
            }
            let response = service
                .call_tool(rmcp::model::CallToolRequestParams::new(name).with_arguments(arguments))
                .await
                .map_err(|_| "MCP tool call failed")?;
            if response.is_error == Some(true) {
                return Err("MCP tool reported an error".into());
            }
            let text = serde_json::to_string(&response).map_err(|_| "Invalid MCP response")?;
            if text.len() > MAX_BYTES {
                return Err("MCP response exceeds limit".into());
            }
            Ok(Output::Result(text))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn local() -> ServerConfig {
        ServerConfig::Stdio {
            command: "/usr/bin/false".into(),
            args: vec![],
        }
    }
    #[test]
    fn cancellation_prevents_spawn() {
        assert_eq!(
            discover(&local(), &Credentials::default(), &AtomicBool::new(true)).unwrap_err(),
            "MCP operation cancelled"
        );
    }
    #[test]
    fn rejects_relative_executable() {
        let c = ServerConfig::Stdio {
            command: "node".into(),
            args: vec![],
        };
        assert!(
            discover(&c, &Credentials::default(), &AtomicBool::new(false))
                .unwrap_err()
                .contains("absolute")
        );
    }
    #[test]
    fn rejects_insecure_and_credential_urls() {
        for url in [
            "http://example.com/mcp",
            "https://token@example.com/mcp",
            "file:///tmp/mcp",
        ] {
            assert!(
                discover(
                    &ServerConfig::Http { url: url.into() },
                    &Credentials::default(),
                    &AtomicBool::new(false)
                )
                .is_err()
            );
        }
    }
    #[test]
    fn arguments_must_be_a_bounded_object() {
        for args in [
            "null".into(),
            "[]".into(),
            "{".into(),
            format!("{{\"x\":\"{}\"}}", "x".repeat(262144)),
        ] {
            assert!(
                call(
                    &local(),
                    &Credentials::default(),
                    "echo",
                    "unused",
                    &args,
                    &AtomicBool::new(false)
                )
                .unwrap_err()
                .contains("arguments")
            );
        }
    }
    #[test]
    fn debug_redacts_secrets() {
        let c = Credentials {
            bearer: Some("bearer-secret".into()),
            environment: BTreeMap::from([("TOKEN".into(), "env-secret".into())]),
        };
        let s = format!("{c:?}");
        assert!(!s.contains("bearer-secret") && !s.contains("env-secret"));
    }
    #[test]
    fn host_guard_is_checked_after_discovery_before_call() {
        let config = fixture("normal");
        let credentials = Credentials::default();
        let cancel = AtomicBool::new(false);
        let tools = discover(&config, &credentials, &cancel).unwrap();
        let checked = AtomicBool::new(false);
        let error = call_guarded(
            &config,
            &credentials,
            "echo",
            &tools[0].schema_hash,
            "{}",
            &cancel,
            &|| {
                checked.store(true, Ordering::Release);
                false
            },
        )
        .unwrap_err();
        assert!(checked.load(Ordering::Acquire));
        assert_eq!(error, "MCP permission revoked before dispatch");
    }
    fn fixture(mode: &str) -> ServerConfig {
        let node = std::process::Command::new("which")
            .arg("node")
            .output()
            .expect("node fixture runtime");
        let command = String::from_utf8(node.stdout).unwrap().trim().to_owned();
        assert!(
            Path::new(&command).is_absolute(),
            "node must be installed for transport tests"
        );
        ServerConfig::Stdio {
            command,
            args: vec![
                format!(
                    "{}/../../scripts/fixtures/mcp-host.mjs",
                    env!("CARGO_MANIFEST_DIR")
                ),
                mode.into(),
            ],
        }
    }
    #[test]
    fn stdio_fixture_discovers_calls_and_hashes_description() {
        let cancel = AtomicBool::new(false);
        let credentials = Credentials {
            bearer: None,
            environment: BTreeMap::from([("FIXTURE_VALUE".into(), "explicit-value".into())]),
        };
        let tools = discover(&fixture("normal"), &credentials, &cancel).unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "echo");
        assert_eq!(tools[0].schema_hash.len(), 64);
        let changed = discover(&fixture("changed"), &credentials, &cancel).unwrap();
        assert_ne!(tools[0].schema_hash, changed[0].schema_hash);
        let readonly = discover(&fixture("readonly"), &credentials, &cancel).unwrap();
        assert!(!tools[0].read_only && readonly[0].read_only);
        assert_ne!(tools[0].schema_hash, readonly[0].schema_hash, "Changing a tool's effect declaration requires a fresh grant");
        let response = call(
            &fixture("normal"),
            &credentials,
            "echo",
            &tools[0].schema_hash,
            "{\"text\":\"hello\"}",
            &cancel,
        )
        .unwrap();
        assert!(response.contains("hello") && response.contains("explicit-value"));
        let response: serde_json::Value = serde_json::from_str(&response).unwrap();
        let echoed: serde_json::Value =
            serde_json::from_str(response["content"][0]["text"].as_str().unwrap()).unwrap();
        assert!(
            echoed["inherited"].is_null(),
            "parent HOME must not be inherited"
        );
        #[cfg(unix)]
        assert_eq!(
            unsafe { libc::kill(echoed["pid"].as_i64().unwrap() as i32, 0) },
            -1,
            "child must be reaped before returning"
        );
    }
    #[test]
    #[cfg(unix)]
    fn stdio_cleanup_kills_server_descendants() {
        let credentials = Credentials::default();
        let cancel = AtomicBool::new(false);
        let tools = discover(&fixture("normal"), &credentials, &cancel).unwrap();
        let response = call(
            &fixture("descendant"),
            &credentials,
            "echo",
            &tools[0].schema_hash,
            "{}",
            &cancel,
        )
        .unwrap();
        let response: serde_json::Value = serde_json::from_str(&response).unwrap();
        let content: serde_json::Value =
            serde_json::from_str(response["content"][0]["text"].as_str().unwrap()).unwrap();
        let descendant = content["descendant"].as_i64().unwrap() as i32;
        // Grandchildren are reaped by the OS after their parent is killed.
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while unsafe { libc::kill(descendant, 0) } == 0 && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            unsafe { libc::kill(descendant, 0) },
            -1,
            "server descendant must not survive session cleanup"
        );
    }
    #[test]
    fn fixture_enforces_schema_tool_page_and_frame_limits() {
        for mode in ["schema", "many", "pages"] {
            assert!(
                discover(
                    &fixture(mode),
                    &Credentials::default(),
                    &AtomicBool::new(false)
                )
                .unwrap_err()
                .contains("limit"),
                "{mode}"
            );
        }
        let tools = discover(
            &fixture("normal"),
            &Credentials::default(),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert!(
            call(
                &fixture("oversize"),
                &Credentials::default(),
                "echo",
                &tools[0].schema_hash,
                "{}",
                &AtomicBool::new(false)
            )
            .is_err()
        );
    }
    #[test]
    fn server_errors_do_not_escape() {
        let tools = discover(
            &fixture("normal"),
            &Credentials::default(),
            &AtomicBool::new(false),
        )
        .unwrap();
        let error = call(
            &fixture("error"),
            &Credentials::default(),
            "echo",
            &tools[0].schema_hash,
            "{}",
            &AtomicBool::new(false),
        )
        .unwrap_err();
        assert!(!error.contains("secret-token"));
    }
    #[test]
    fn cancellation_interrupts_handshake() {
        let cancel = AtomicBool::new(false);
        let start = std::time::Instant::now();
        std::thread::scope(|scope| {
            scope.spawn(|| {
                std::thread::sleep(Duration::from_millis(150));
                cancel.store(true, Ordering::Release);
            });
            assert_eq!(
                discover(&fixture("hang"), &Credentials::default(), &cancel).unwrap_err(),
                "MCP operation cancelled"
            );
        });
        assert!(start.elapsed() < Duration::from_secs(2));
    }
    #[test]
    fn operation_deadline_ends_a_stalled_future() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let cancel = AtomicBool::new(false);
            let result = tokio::time::timeout(
                Duration::from_millis(500),
                bounded_with_timeout(
                    std::future::pending::<Result<(), String>>(),
                    &cancel,
                    Duration::from_millis(25),
                ),
            )
            .await;
            assert_eq!(
                result
                    .expect("injected operation deadline must finish before outer guard")
                    .unwrap_err(),
                "MCP operation timed out"
            );
        });
    }
    #[test]
    fn http_failure_status_cannot_be_promoted_to_json_rpc_success() {
        use std::io::{Read, Write};
        for status in [429, 500] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}", listener.local_addr().unwrap());
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buf = [0; 4096];
                let _ = stream.read(&mut buf);
                let body = r#"{"jsonrpc":"2.0","id":1,"result":{"tools":[]}}"#;
                write!(stream, "HTTP/1.1 {status} Error\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            });
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            let result = runtime.block_on(async {
                let message =
                    serde_json::from_str(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#)
                        .unwrap();
                BoundedHttp::default()
                    .post_message(url.into(), message, None, None, Default::default())
                    .await
            });
            server.join().unwrap();
            assert!(result.is_err(), "HTTP {status} was treated as success");
        }
    }
    #[test]
    fn http_rejects_oversized_json_before_deserializing() {
        use rmcp::transport::streamable_http_client::StreamableHttpClient;
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 4096];
            let _ = stream.read(&mut request);
            let body = format!(
                "{{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{{\"tools\":[],\"ignored\":\"{}\"}}}}",
                "x".repeat(MAX_BYTES)
            );
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
        });
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let message =
                serde_json::from_str("{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/list\"}")
                    .unwrap();
            let client = BoundedHttp::default();
            assert!(
                client
                    .post_message(url.into(), message, None, None, Default::default())
                    .await
                    .is_err()
            );
        });
        server.join().unwrap();
    }
    #[test]
    fn chunked_http_body_is_bounded_without_content_length() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0; 4096];
            let _ = stream.read(&mut buf);
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n").unwrap();
            let chunk = "x".repeat(16384);
            for _ in 0..17 {
                if write!(stream, "{:x}\r\n{chunk}\r\n", chunk.len()).is_err() {
                    return;
                }
            }
            let _ = write!(stream, "0\r\n\r\n");
        });
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let result = runtime.block_on(async {
            let message =
                serde_json::from_str(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#).unwrap();
            BoundedHttp::default()
                .post_message(url.into(), message, None, None, Default::default())
                .await
        });
        server.join().unwrap();
        assert!(result.unwrap_err().to_string().contains("exceeds limit"));
    }
    #[test]
    fn http_redirect_never_reaches_target() {
        use std::io::{Read, Write};
        let target = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        target.set_nonblocking(true).unwrap();
        let destination = target.local_addr().unwrap();
        let source = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let config = ServerConfig::Http {
            url: format!("http://{}", source.local_addr().unwrap()),
        };
        let server = std::thread::spawn(move || {
            let (mut stream, _) = source.accept().unwrap();
            let mut buf = [0; 4096];
            let _ = stream.read(&mut buf);
            write!(stream, "HTTP/1.1 307 Temporary Redirect\r\nLocation: http://{destination}/forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
        });
        let credentials = Credentials {
            bearer: Some("fixture-secret".into()),
            environment: Default::default(),
        };
        assert!(discover(&config, &credentials, &AtomicBool::new(false)).is_err());
        server.join().unwrap();
        assert!(
            matches!(target.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock),
            "redirect target must receive no connection or bearer token"
        );
    }
    #[test]
    fn fixture_accept_waits_for_delayed_request_bytes() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let client = std::thread::spawn(move || {
            let mut stream = std::net::TcpStream::connect(address).unwrap();
            std::thread::sleep(Duration::from_millis(50));
            let _ = stream.write_all(b"x");
        });
        let stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(_) => std::thread::sleep(Duration::from_millis(1)),
            }
        };
        let mut stream = fixture_stream(stream);
        let mut byte = [0];
        let result = stream.read_exact(&mut byte);
        client.join().unwrap();
        result.expect("accepted fixture socket must wait for bytes, not fail WouldBlock");
        assert_eq!(byte, *b"x");
    }
    fn fixture_stream(stream: std::net::TcpStream) -> std::net::TcpStream {
        // macOS inherits O_NONBLOCK from the listening socket. A timeout alone
        // does not clear it; reads otherwise race arrival of the request bytes.
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        stream
    }
    #[test]
    fn http_json_and_sse_sessions_discover_and_call_once() {
        use std::io::{Read, Write};
        use std::sync::atomic::AtomicUsize;
        for sse in [false, true] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            listener.set_nonblocking(true).unwrap();
            let config = ServerConfig::Http {
                url: format!("http://{}/mcp", listener.local_addr().unwrap()),
            };
            let stop = Arc::new(AtomicBool::new(false));
            let calls = Arc::new(AtomicUsize::new(0));
            let server_stop = stop.clone();
            let server_calls = calls.clone();
            let server = std::thread::spawn(move || {
                while !server_stop.load(Ordering::Acquire) {
                    let (mut stream, _) = match listener.accept() {
                        Ok(pair) => pair,
                        Err(_) => {
                            std::thread::sleep(Duration::from_millis(2));
                            continue;
                        }
                    };
                    stream = fixture_stream(stream);
                    let mut wire = Vec::new();
                    let mut buf = [0u8; 4096];
                    let (headers, body) = loop {
                        let n = stream.read(&mut buf).unwrap_or(0);
                        if n == 0 {
                            break (String::new(), vec![]);
                        }
                        wire.extend_from_slice(&buf[..n]);
                        if let Some(end) = wire.windows(4).position(|w| w == b"\r\n\r\n") {
                            let headers = String::from_utf8_lossy(&wire[..end]).into_owned();
                            let length = headers
                                .lines()
                                .find_map(|line| {
                                    line.to_ascii_lowercase()
                                        .strip_prefix("content-length:")
                                        .and_then(|v| v.trim().parse::<usize>().ok())
                                })
                                .unwrap_or(0);
                            if wire.len() >= end + 4 + length {
                                break (headers, wire[end + 4..end + 4 + length].to_vec());
                            }
                        }
                    };
                    if headers.starts_with("GET") {
                        let _ = write!(
                            stream,
                            "HTTP/1.1 405 Method Not Allowed\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                        );
                        continue;
                    }
                    if headers.starts_with("DELETE") {
                        let _ = write!(
                            stream,
                            "HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n"
                        );
                        continue;
                    }
                    let Ok(request) = serde_json::from_slice::<serde_json::Value>(&body) else {
                        continue;
                    };
                    if request.get("id").is_none() {
                        let _ = write!(
                            stream,
                            "HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                        );
                        continue;
                    }
                    let method = request["method"].as_str().unwrap();
                    let mut response = serde_json::json!({"jsonrpc":"2.0","id":request["id"]});
                    match method {
                        "initialize" => {
                            response["result"] = serde_json::json!({"protocolVersion":"2025-03-26","capabilities":{"tools":{}},"serverInfo":{"name":"fixture","version":"1"}})
                        }
                        "tools/list" => {
                            response["result"] = serde_json::json!({"tools":[{"name":"echo","description":"Echo","inputSchema":{"type":"object"}}]})
                        }
                        "tools/call" => {
                            server_calls.fetch_add(1, Ordering::AcqRel);
                            response["result"] =
                                serde_json::json!({"content":[{"type":"text","text":"http-ok"}]});
                        }
                        _ => {
                            response["error"] =
                                serde_json::json!({"code":-32601,"message":"unsupported"})
                        }
                    }
                    let (mime, body) = if sse {
                        (
                            "text/event-stream",
                            format!("event: message\ndata: {response}\n\n"),
                        )
                    } else {
                        ("application/json", response.to_string())
                    };
                    let _ = write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Type: {mime}\r\nMcp-Session-Id: fixture-session\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                }
            });
            let result = (|| {
                let tools = discover(&config, &Credentials::default(), &AtomicBool::new(false))?;
                assert_eq!(tools[0].name, "echo");
                let response = call(
                    &config,
                    &Credentials::default(),
                    "echo",
                    &tools[0].schema_hash,
                    "{}",
                    &AtomicBool::new(false),
                )?;
                assert!(response.contains("http-ok"));
                Ok::<(), String>(())
            })();
            stop.store(true, Ordering::Release);
            server.join().unwrap();
            result.unwrap();
            assert_eq!(calls.load(Ordering::Acquire), 1);
        }
    }
    #[test]
    fn same_session_grant_rejects_schema_drift_and_unknown_tools() {
        let credentials = Credentials::default();
        let cancel = AtomicBool::new(false);
        let tools = discover(&fixture("normal"), &credentials, &cancel).unwrap();
        let drift = call(
            &fixture("changed"),
            &credentials,
            "echo",
            &tools[0].schema_hash,
            "{}",
            &cancel,
        )
        .unwrap_err();
        assert!(drift.contains("schema"));
        let unknown = call(
            &fixture("normal"),
            &credentials,
            "unknown",
            &tools[0].schema_hash,
            "{}",
            &cancel,
        )
        .unwrap_err();
        assert!(unknown.contains("schema"));
    }
}
