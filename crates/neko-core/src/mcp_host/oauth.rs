//! Native public-client OAuth using rmcp's discovery, PKCE and token state machine.
//! All SDK tracing is suppressed: upstream debug events include authorization codes.
use reqwest_mcp::Url;
use rmcp::transport::auth::{
    self, AuthError, AuthorizationCallback, AuthorizationManager, AuthorizationMetadata,
    CredentialStore, OAuthClientConfig, OAuthHttpClient, OAuthHttpClientFuture,
    OAuthHttpRedirectPolicy, OAuthHttpRequest, StoredCredentials,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    net::IpAddr,
    sync::{
        Arc, LazyLock, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const SERVICE: &str = "app.neko.mcp.oauth";
const FAILED: &str =
    "MCP authentication failed. Check the server configuration and authenticate again.";
static REFRESH_LOCK: Mutex<()> = Mutex::new(());
static CREDENTIAL_GATE: LazyLock<CredentialGate> = LazyLock::new(CredentialGate::default);

/// Serializes Keychain reads/writes, without holding a lock across network IO.
/// Epochs invalidate in-flight refreshes and authorizations on remove/replace.
#[derive(Default)]
struct CredentialGate(Mutex<HashMap<String, u64>>);
impl CredentialGate {
    fn snapshot<T>(
        &self,
        id: &str,
        read: impl FnOnce() -> Result<T, String>,
    ) -> Result<(u64, T), String> {
        let epochs = self.0.lock().map_err(|_| FAILED)?;
        Ok((*epochs.get(id).unwrap_or(&0), read()?))
    }
    fn invalidate<T>(&self, id: &str, action: impl FnOnce() -> T) -> Result<(u64, T), String> {
        let mut epochs = self.0.lock().map_err(|_| FAILED)?;
        let epoch = epochs.entry(id.into()).or_default();
        *epoch = epoch.checked_add(1).ok_or(FAILED)?;
        Ok((*epoch, action()))
    }
    fn commit(
        &self,
        id: &str,
        expected: u64,
        replace: bool,
        write: impl FnOnce() -> Result<(), String>,
    ) -> Result<(), String> {
        let mut epochs = self.0.lock().map_err(|_| FAILED)?;
        let epoch = epochs.entry(id.into()).or_default();
        if *epoch != expected {
            return Err("Authentication was replaced or removed. Try again.".into());
        }
        let next = if replace {
            epoch.checked_add(1).ok_or(FAILED)?
        } else {
            *epoch
        };
        write()?;
        *epoch = next;
        Ok(())
    }
}

async fn bounded<T>(
    deadline: tokio::time::Instant,
    cancel: &AtomicBool,
    future: impl std::future::Future<Output = Result<T, String>>,
) -> Result<T, String> {
    tokio::select! {
        biased;
        _ = async { while !cancel.load(Ordering::Acquire) { tokio::time::sleep(Duration::from_millis(20)).await; } } => Err("Authentication cancelled".into()),
        result = tokio::time::timeout_at(deadline, future) => result.map_err(|_| "Authentication timed out. Try again.".to_string())?,
    }
}

async fn read_callback(
    stream: &mut tokio::net::TcpStream,
    deadline: tokio::time::Instant,
    cancel: &AtomicBool,
) -> Result<AuthorizationCallback, String> {
    bounded(deadline, cancel, async {
        let mut bytes = Vec::new();
        let mut part = [0; 1024];
        loop {
            let remaining = 8192usize.saturating_sub(bytes.len());
            if remaining == 0 {
                return Err(FAILED.into());
            }
            let limit = remaining.min(part.len());
            let count = stream.read(&mut part[..limit]).await.map_err(|_| FAILED)?;
            if count == 0 {
                return Err(FAILED.into());
            }
            bytes.extend_from_slice(&part[..count]);
            if bytes.windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
        }
        let first = std::str::from_utf8(&bytes)
            .map_err(|_| FAILED)?
            .lines()
            .next()
            .ok_or(FAILED)?;
        let fields: Vec<_> = first.split_whitespace().collect();
        if fields.len() != 3 || fields[0] != "GET" || !matches!(fields[2], "HTTP/1.0" | "HTTP/1.1")
        {
            return Err(FAILED.into());
        }
        callback(fields[1])
    })
    .await
}

fn safe_url(raw: &str) -> Result<Url, String> {
    let url = Url::parse(raw).map_err(|_| FAILED)?;
    let host = url.host_str().ok_or(FAILED)?;
    let private = host
        .trim_matches(['[', ']'])
        .parse::<IpAddr>()
        .is_ok_and(|ip| match ip {
            IpAddr::V4(ip) => {
                ip.is_private()
                    || ip.is_loopback()
                    || ip.is_link_local()
                    || ip.is_unspecified()
                    || ip.is_broadcast()
                    || ip.is_multicast()
            }
            IpAddr::V6(ip) => {
                ip.is_loopback()
                    || ip.is_unspecified()
                    || ip.is_unique_local()
                    || ip.is_unicast_link_local()
                    || ip.is_multicast()
                    || ip.to_ipv4_mapped().is_some()
            }
        });
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || private
        || host == "localhost"
        || host.ends_with(".localhost")
        || host.ends_with(".internal")
        || host == "metadata"
    {
        return Err(FAILED.into());
    }
    Ok(url)
}

struct SafeHttp {
    inner: Box<dyn OAuthHttpClient>,
}
impl OAuthHttpClient for SafeHttp {
    fn execute(&self, mut request: OAuthHttpRequest) -> OAuthHttpClientFuture<'_> {
        Box::pin(async move {
            safe_url(&request.request.uri().to_string())
                .map_err(|_| std::io::Error::other(FAILED))?;
            request.redirect_policy = OAuthHttpRedirectPolicy::Stop;
            let response = self.inner.execute(request).await?;
            if response.status().is_redirection() {
                return Err(std::io::Error::other(FAILED).into());
            }
            Ok(response)
        })
    }
}

#[derive(Clone, Default)]
struct MemoryStore(Arc<Mutex<Option<StoredCredentials>>>);
#[async_trait::async_trait]
impl CredentialStore for MemoryStore {
    async fn load(&self) -> Result<Option<StoredCredentials>, AuthError> {
        Ok(self
            .0
            .lock()
            .map_err(|_| AuthError::InternalError(FAILED.into()))?
            .clone())
    }
    async fn save(&self, credentials: StoredCredentials) -> Result<(), AuthError> {
        *self
            .0
            .lock()
            .map_err(|_| AuthError::InternalError(FAILED.into()))? = Some(credentials);
        Ok(())
    }
    async fn clear(&self) -> Result<(), AuthError> {
        *self
            .0
            .lock()
            .map_err(|_| AuthError::InternalError(FAILED.into()))? = None;
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
struct Saved {
    endpoint: String,
    resource: String,
    metadata: AuthorizationMetadata,
    credentials: StoredCredentials,
}
impl Saved {
    fn validate(&self, endpoint: &str) -> Result<(), String> {
        if self.endpoint != endpoint || self.credentials.issuer != self.metadata.issuer {
            return Err(FAILED.into());
        }
        safe_url(endpoint)?;
        safe_url(&self.resource)?;
        validate_metadata(&self.metadata)
    }
}
fn validate_metadata(metadata: &AuthorizationMetadata) -> Result<(), String> {
    safe_url(metadata.issuer.as_deref().ok_or(FAILED)?)?;
    safe_url(&metadata.authorization_endpoint)?;
    safe_url(&metadata.token_endpoint)?;
    if let Some(url) = &metadata.registration_endpoint {
        safe_url(url)?;
    }
    Ok(())
}
fn runtime<T>(future: impl std::future::Future<Output = Result<T, String>>) -> Result<T, String> {
    tracing::subscriber::with_default(tracing::subscriber::NoSubscriber::default(), || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| FAILED)?
            .block_on(future)
    })
}
async fn manager(endpoint: &str, store: MemoryStore) -> Result<AuthorizationManager, String> {
    let inner = auth::default_oauth_http_client().map_err(|_| FAILED)?;
    let mut manager = AuthorizationManager::new_with_oauth_http_client(
        endpoint,
        Arc::new(SafeHttp {
            inner: Box::new(inner),
        }),
    )
    .await
    .map_err(|_| FAILED)?;
    manager.set_credential_store(store);
    Ok(manager)
}
fn callback(target: &str) -> Result<AuthorizationCallback, String> {
    if target.len() > 8192 || !target.starts_with("/callback?") {
        return Err(FAILED.into());
    }
    let url = Url::parse(&format!("http://127.0.0.1{target}")).map_err(|_| FAILED)?;
    if url.query_pairs().any(|(key, value)| {
        key == "error" || (matches!(key.as_ref(), "code" | "state" | "iss") && value.is_empty())
    }) {
        return Err(FAILED.into());
    }
    for key in ["code", "state", "iss", "error"] {
        if url.query_pairs().filter(|(k, _)| k == key).count() > 1 {
            return Err(FAILED.into());
        }
    }
    AuthorizationCallback::from_redirect_url(url.as_str()).map_err(|_| FAILED.into())
}

/// Called only following an explicit Authenticate action. URL delivery does not itself open a browser.
pub fn authorize(
    id: &str,
    url: &str,
    client_id: Option<&str>,
    cancel: &AtomicBool,
    on_url: impl FnOnce(&str),
) -> Result<(), String> {
    let deadline = tokio::time::Instant::from_std(Instant::now() + Duration::from_secs(120));
    if cancel.load(Ordering::Acquire) {
        return Err("Authentication cancelled".into());
    }
    safe_url(url)?;
    let epoch = CREDENTIAL_GATE.invalidate(id, || ())?.0;
    runtime(async {
        bounded(deadline, cancel, async {
            let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await.map_err(|_| FAILED)?;
            let redirect = format!("http://127.0.0.1:{}/callback", listener.local_addr().map_err(|_| FAILED)?.port());
            let store = MemoryStore::default();
            let mut manager = manager(url, store.clone()).await?;
            let resolution = manager.resolve_metadata().await.map_err(|_| FAILED)?;
            if !resolution.source.is_discovered() { return Err("Server does not publish supported OAuth metadata".into()); }
            let mut metadata = resolution.metadata;
            validate_metadata(&metadata)?;
            // Supported scopes are not required scopes. Remove the SDK's fallback and
            // implicit offline_access addition; retain only protected-resource requirements.
            metadata.scopes_supported = None;
            manager.set_metadata(metadata.clone());
            let scopes = manager.select_scopes(None, &[]);
            let scopes: Vec<&str> = scopes.iter().map(String::as_str).collect();
            let config = if let Some(client_id) = client_id {
                OAuthClientConfig::new(client_id, &redirect)
            } else {
                manager.register_client("neko", &redirect, &scopes).await.map_err(|_| "Server requires a pre-registered OAuth client ID or supports no public-client registration")?
            };
            if config.client_secret.is_some() { return Err("Server requires a confidential OAuth client; neko supports public native clients".into()); }
            manager.configure_client(config).map_err(|_| FAILED)?;
            let auth_url = manager.get_authorization_url(&scopes).await.map_err(|_| FAILED)?;
            let parsed = safe_url(&auth_url)?;
            let resource = parsed.query_pairs().find(|(key, _)| key == "resource").map(|(_, v)| v.into_owned()).ok_or(FAILED)?;
            if cancel.load(Ordering::Acquire) { return Err("Authentication cancelled".into()); }
            on_url(&auth_url);
            loop {
                match listener.accept().await {
                    Ok((mut stream, address)) => {
                        if !address.ip().is_loopback() { continue; }
                        let read_deadline = deadline.min(tokio::time::Instant::now() + Duration::from_secs(2));
                        let result = read_callback(&mut stream, read_deadline, cancel).await;
                        let success = if let Ok(callback) = result {
                            manager.exchange_code_for_token_with_issuer(&callback.code, &callback.csrf_token, callback.issuer.as_deref()).await.is_ok()
                        } else { false };
                        let response = if success { "HTTP/1.1 200 OK\r\nContent-Length: 37\r\nConnection: close\r\nContent-Type: text/plain\r\n\r\nAuthenticated. You can close this tab." } else { "HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n" };
                        let _ = tokio::time::timeout_at(deadline.min(tokio::time::Instant::now() + Duration::from_secs(1)), stream.write_all(response.as_bytes())).await;
                        if success {
                            if cancel.load(Ordering::Acquire) { return Err("Authentication cancelled".into()); }
                            let credentials = store.load().await.map_err(|_| FAILED)?.ok_or(FAILED)?;
                            return CREDENTIAL_GATE.commit(id, epoch, true, || save(id, &Saved { endpoint: url.into(), resource, metadata, credentials }));
                        }
                    }
                    Err(_) => return Err(FAILED.into()),
                }
            }
        }).await
    })
}

pub fn bearer(id: &str, url: &str) -> Result<String, String> {
    bearer_cancellable(id, url, &AtomicBool::new(false))
}

async fn refresh_guard(lock: &Mutex<()>) -> Result<std::sync::MutexGuard<'_, ()>, String> {
    loop {
        match lock.try_lock() {
            Ok(guard) => return Ok(guard),
            Err(std::sync::TryLockError::WouldBlock) => {
                tokio::time::sleep(Duration::from_millis(10)).await
            }
            Err(std::sync::TryLockError::Poisoned(_)) => return Err(FAILED.into()),
        }
    }
}

/// Bounds lock waiting and network IO together; cancelled refreshes never commit.
pub fn bearer_cancellable(id: &str, url: &str, cancel: &AtomicBool) -> Result<String, String> {
    let deadline = tokio::time::Instant::from_std(Instant::now() + Duration::from_secs(20));
    if cancel.load(Ordering::Acquire) {
        return Err("Authentication cancelled".into());
    }
    runtime(async {
        bounded(deadline, cancel, async {
            let _guard = refresh_guard(&REFRESH_LOCK).await?;
            let (epoch, mut saved) = CREDENTIAL_GATE.snapshot(id, || load(id))?;
            saved.validate(url)?;
            if cancel.load(Ordering::Acquire) || tokio::time::Instant::now() >= deadline {
                return Err("Authentication cancelled or timed out".into());
            }
            let store = MemoryStore::default();
            store
                .save(saved.credentials.clone())
                .await
                .map_err(|_| FAILED)?;
            let mut manager = manager(&saved.resource, store.clone()).await?;
            manager.set_metadata(saved.metadata.clone());
            manager
                .configure_client_id(&saved.credentials.client_id)
                .map_err(|_| FAILED)?;
            let token = manager.get_access_token().await.map_err(|_| FAILED)?;
            if token.is_empty() || token.len() > 8192 || token.chars().any(char::is_control) {
                return Err(FAILED.into());
            }
            saved.credentials = store.load().await.map_err(|_| FAILED)?.ok_or(FAILED)?;
            CREDENTIAL_GATE.commit(id, epoch, false, || {
                if cancel.load(Ordering::Acquire) || tokio::time::Instant::now() >= deadline {
                    return Err("Authentication cancelled or timed out".into());
                }
                save(id, &saved)
            })?;
            Ok(token)
        })
        .await
    })
}
fn save(id: &str, saved: &Saved) -> Result<(), String> {
    saved.validate(&saved.endpoint)?;
    let bytes = serde_json::to_vec(saved).map_err(|_| FAILED)?;
    if bytes.len() > 131072 {
        return Err(FAILED.into());
    }
    #[cfg(target_os = "macos")]
    {
        security_framework::passwords::set_generic_password(SERVICE, id, &bytes)
            .map_err(|_| "Could not save MCP authentication in Keychain".into())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (id, bytes);
        Err("OAuth requires macOS Keychain".into())
    }
}
fn load(id: &str) -> Result<Saved, String> {
    #[cfg(target_os = "macos")]
    {
        let bytes =
            security_framework::passwords::get_generic_password(SERVICE, id).map_err(|_| FAILED)?;
        if bytes.len() > 131072 {
            return Err(FAILED.into());
        }
        serde_json::from_slice(&bytes).map_err(|_| FAILED.into())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = id;
        Err("OAuth requires macOS Keychain".into())
    }
}
pub fn remove(id: &str) {
    let _ = CREDENTIAL_GATE.invalidate(id, || {
        #[cfg(target_os = "macos")]
        let _ = security_framework::passwords::delete_generic_password(SERVICE, id);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
    };
    #[test]
    fn refresh_lock_wait_is_deadline_bound_and_cancellable() {
        let lock = Mutex::new(());
        let _held = lock.lock().unwrap();
        runtime(async {
            let cancel = AtomicBool::new(false);
            let start = Instant::now();
            assert!(
                bounded(
                    tokio::time::Instant::now() + Duration::from_millis(40),
                    &cancel,
                    refresh_guard(&lock)
                )
                .await
                .is_err()
            );
            assert!(start.elapsed() < Duration::from_millis(500));
            cancel.store(true, Ordering::Release);
            assert_eq!(
                bounded(
                    tokio::time::Instant::now() + Duration::from_secs(20),
                    &cancel,
                    refresh_guard(&lock)
                )
                .await
                .err()
                .unwrap(),
                "Authentication cancelled"
            );
            assert_eq!(
                bearer_cancellable("never-read-keychain", "https://example.com/mcp", &cancel)
                    .err()
                    .unwrap(),
                "Authentication cancelled"
            );
            Ok(())
        })
        .unwrap();
    }
    #[test]
    fn fragmented_callback_waits_for_complete_headers() {
        runtime(async {
            use tokio::io::AsyncWriteExt;
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let client = tokio::spawn(async move {
                let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
                tokio::time::sleep(Duration::from_millis(30)).await;
                for part in [
                    "GET /callback?code=test",
                    "&state=csrf HTTP/1.1\r\n",
                    "Host: localhost\r\n",
                    "\r\n",
                ] {
                    stream.write_all(part.as_bytes()).await.unwrap();
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            });
            let (mut stream, _) = listener.accept().await.unwrap();
            let result = read_callback(
                &mut stream,
                tokio::time::Instant::now() + Duration::from_secs(2),
                &AtomicBool::new(false),
            )
            .await?;
            assert_eq!(result.code, "test");
            assert_eq!(result.csrf_token, "csrf");
            client.await.unwrap();
            Ok(())
        })
        .unwrap();
    }
    #[test]
    fn incomplete_callback_obeys_deadline_and_cancellation() {
        runtime(async {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let _client = tokio::net::TcpStream::connect(listener.local_addr().unwrap())
                .await
                .unwrap();
            let (mut stream, _) = listener.accept().await.unwrap();
            let start = Instant::now();
            assert!(
                read_callback(
                    &mut stream,
                    tokio::time::Instant::now() + Duration::from_millis(40),
                    &AtomicBool::new(false)
                )
                .await
                .is_err()
            );
            assert!(start.elapsed() < Duration::from_millis(500));
            let cancel = AtomicBool::new(false);
            let cancel_later = async {
                tokio::time::sleep(Duration::from_millis(20)).await;
                cancel.store(true, Ordering::Release);
            };
            let (result, _) = tokio::join!(
                read_callback(
                    &mut stream,
                    tokio::time::Instant::now() + Duration::from_secs(2),
                    &cancel
                ),
                cancel_later
            );
            assert_eq!(result.err().unwrap(), "Authentication cancelled");
            Ok(())
        })
        .unwrap();
    }
    #[test]
    fn removed_or_reauthorized_credentials_reject_stale_writes() {
        let gate = CredentialGate::default();
        let epoch = gate.snapshot("test", || Ok(())).unwrap().0;
        gate.invalidate("test", || ()).unwrap();
        assert!(
            gate.commit("test", epoch, false, || panic!(
                "stale refresh wrote tokens"
            ))
            .is_err()
        );
        let authorization_epoch = gate.invalidate("test", || ()).unwrap().0;
        let refresh_epoch = gate.snapshot("test", || Ok(())).unwrap().0;
        gate.commit("test", authorization_epoch, true, || Ok(()))
            .unwrap();
        assert!(
            gate.commit("test", refresh_epoch, false, || panic!(
                "refresh overwrote reauthorization"
            ))
            .is_err()
        );
        let authorization_epoch = gate.invalidate("test", || ()).unwrap().0;
        gate.invalidate("test", || ()).unwrap();
        assert!(
            gate.commit("test", authorization_epoch, true, || panic!(
                "removed authorization restored credentials"
            ))
            .is_err()
        );
        let other = gate.snapshot("other", || Ok(())).unwrap().0;
        gate.commit("other", other, false, || Ok(())).unwrap();
    }
    #[test]
    fn remove_serializes_with_a_refresh_already_committing() {
        let gate = Arc::new(CredentialGate::default());
        let value = Arc::new(Mutex::new(Some("old")));
        let epoch = gate.snapshot("test", || Ok(())).unwrap().0;
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let refresh = {
            let gate = gate.clone();
            let value = value.clone();
            std::thread::spawn(move || {
                gate.commit("test", epoch, false, || {
                    entered_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                    *value.lock().unwrap() = Some("refreshed");
                    Ok(())
                })
                .unwrap()
            })
        };
        entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let removal = {
            let gate = gate.clone();
            let value = value.clone();
            std::thread::spawn(move || {
                gate.invalidate("test", || *value.lock().unwrap() = None)
                    .unwrap()
            })
        };
        release_tx.send(()).unwrap();
        refresh.join().unwrap();
        removal.join().unwrap();
        assert_eq!(*value.lock().unwrap(), None);
        assert!(
            gate.commit("test", epoch, false, || panic!(
                "removed credential restored"
            ))
            .is_err()
        );
    }
    fn metadata() -> AuthorizationMetadata {
        serde_json::from_value(serde_json::json!({
            "issuer":"https://auth.example.com", "authorization_endpoint":"https://auth.example.com/authorize",
            "token_endpoint":"https://auth.example.com/token", "authorization_response_iss_parameter_supported":true
        })).unwrap()
    }
    #[test]
    fn sdk_rejects_forged_state_and_issuer_before_token_exchange() {
        runtime(async {
            let mut manager = manager("https://example.com/mcp", MemoryStore::default()).await?;
            manager.set_metadata(metadata());
            manager
                .configure_client(OAuthClientConfig::new(
                    "test",
                    "http://127.0.0.1:9999/callback",
                ))
                .unwrap();
            let url = Url::parse(&manager.get_authorization_url(&[]).await.unwrap()).unwrap();
            let state = url
                .query_pairs()
                .find(|(k, _)| k == "state")
                .unwrap()
                .1
                .into_owned();
            assert!(
                url.query_pairs()
                    .any(|(k, v)| k == "code_challenge_method" && v == "S256")
            );
            for (state, issuer) in [
                ("forged", Some("https://auth.example.com")),
                (state.as_str(), Some("https://evil.example.com")),
                (state.as_str(), None),
            ] {
                assert!(
                    manager
                        .exchange_code_for_token_with_issuer("secret-code", state, issuer)
                        .await
                        .is_err()
                );
            }
            Ok(())
        })
        .unwrap();
    }
    #[test]
    fn persisted_tokens_are_bound_to_exact_endpoint_and_issuer() {
        let credentials = StoredCredentials::new("test".into(), None, vec![], None)
            .with_issuer(Some("https://auth.example.com".into()));
        let mut saved = Saved {
            endpoint: "https://example.com/mcp".into(),
            resource: "https://example.com/mcp".into(),
            metadata: metadata(),
            credentials,
        };
        assert!(saved.validate("https://example.com/mcp").is_ok());
        for endpoint in ["https://example.com/other", "https://evil.example.com/mcp"] {
            assert_eq!(saved.validate(endpoint).unwrap_err(), FAILED);
        }
        saved.metadata.issuer = Some("https://changed.example.com".into());
        assert_eq!(
            saved.validate("https://example.com/mcp").unwrap_err(),
            FAILED
        );
    }
    #[test]
    fn expired_token_refresh_preserves_refresh_token_and_resource() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let token_endpoint = format!("{base}/token");
        let fixture = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut bytes = vec![];
            let mut chunk = [0; 4096];
            loop {
                let n = stream.read(&mut chunk).unwrap();
                bytes.extend_from_slice(&chunk[..n]);
                if let Some(header_end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&bytes[..header_end]);
                    let len: usize = headers
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .map(|s| s.trim().parse().unwrap())
                        })
                        .unwrap();
                    if bytes.len() >= header_end + 4 + len {
                        break;
                    }
                }
            }
            let body = r#"{"access_token":"new-token","token_type":"Bearer","expires_in":3600}"#;
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
            String::from_utf8(bytes).unwrap()
        });
        runtime(async {
            let store = MemoryStore::default();
            let token = serde_json::from_value(serde_json::json!({"access_token":"expired", "refresh_token":"refresh-secret", "token_type":"Bearer", "expires_in":1})).unwrap();
            store.save(StoredCredentials::new("test".into(), Some(token), vec!["read".into()], Some(1)).with_issuer(Some("https://auth.example.com".into()))).await.unwrap();
            // Only this isolated fixture bypasses the production HTTPS-only HTTP adapter.
            let mut manager = AuthorizationManager::new("https://example.com/resource").await.unwrap();
            manager.set_credential_store(store.clone());
            let mut meta = metadata(); meta.token_endpoint = token_endpoint;
            manager.set_metadata(meta);
            manager.configure_client_id("test").unwrap();
            assert_eq!(manager.get_access_token().await.unwrap(), "new-token");
            let serialized = serde_json::to_value(store.load().await.unwrap().unwrap()).unwrap();
            assert_eq!(serialized["token_response"]["refresh_token"], "refresh-secret");
            assert_eq!(manager.get_access_token().await.unwrap(), "new-token");
            Ok(())
        }).unwrap();
        let request = fixture.join().unwrap();
        assert!(request.contains("grant_type=refresh_token"));
        assert!(request.contains("resource=https%3A%2F%2Fexample.com%2Fresource"));
        assert!(!request.contains("offline_access"));
    }
    #[test]
    fn endpoints_reject_insecure_userinfo_fragments_and_private_addresses() {
        for url in [
            "http://example.com",
            "https://u:secret@example.com",
            "https://example.com/#secret",
            "https://127.0.0.1",
            "https://192.168.1.1",
        ] {
            assert!(safe_url(url).is_err());
        }
        assert!(safe_url("https://example.com/mcp").is_ok());
    }
    #[test]
    fn callback_rejects_ambiguous_fields_and_wrong_path() {
        for target in [
            "/other?code=x&state=y",
            "/callback?code=x&state=a&state=b",
            "/callback?code=x&state=a&iss=a&iss=b",
            "/callback?error=secret",
            "/callback?code=x&state=y&error=secret",
            "/callback?code=&state=",
        ] {
            assert!(callback(target).is_err());
        }
        assert!(callback("/callback?code=x&state=y").is_ok());
    }
}
