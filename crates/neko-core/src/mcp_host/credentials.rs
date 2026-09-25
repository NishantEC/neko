//! Secrets stay in the host. Configuration snapshots contain only presence flags.
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Credentials {
    pub bearer: Option<String>,
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
}
impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Credentials([REDACTED])")
    }
}
pub fn parse(input: &str) -> Result<Credentials, String> {
    let invalid = || "Invalid credential configuration".to_string();
    if input.len() > 32_768 {
        return Err(invalid());
    }
    let credentials: Credentials = serde_json::from_str(input).map_err(|_| invalid())?;
    if credentials
        .bearer
        .as_ref()
        .is_some_and(|s| s.is_empty() || s.len() > 8192 || s.chars().any(char::is_control))
        || credentials.environment.len() > 32
    {
        return Err(invalid());
    }
    for (key, value) in &credentials.environment {
        if key.is_empty()
            || key.len() > 128
            || !key.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
            || key.as_bytes()[0].is_ascii_digit()
            || value.len() > 8192
            || value.contains('\0')
        {
            return Err(invalid());
        }
    }
    Ok(credentials)
}

const SERVICE: &str = "app.neko.mcp";
pub fn store(id: &str, input: &str) -> Result<(), String> {
    parse(input)?;
    #[cfg(target_os = "macos")]
    {
        security_framework::passwords::set_generic_password(SERVICE, id, input.as_bytes())
            .map_err(|_| "Could not save MCP credentials in Keychain".into())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = id;
        Err("Secure credentials require macOS Keychain".into())
    }
}
pub fn load(id: &str, has_credentials: bool) -> Result<Credentials, String> {
    if !has_credentials {
        return Ok(Credentials::default());
    }
    #[cfg(target_os = "macos")]
    {
        let bytes = security_framework::passwords::get_generic_password(SERVICE, id)
            .map_err(|_| "MCP credentials unavailable. Reconnect or allow Keychain access.")?;
        let raw = String::from_utf8(bytes).map_err(|_| "Invalid MCP credentials")?;
        parse(&raw)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = id;
        Err("Secure credentials require macOS Keychain".into())
    }
}
pub fn remove(id: &str) {
    #[cfg(target_os = "macos")]
    {
        let _ = security_framework::passwords::delete_generic_password(SERVICE, id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn supports_optional_bearer_and_explicit_environment_without_debug_leaks() {
        let c = parse(r#"{"bearer":"sentinel_secret","environment":{"TOKEN":"another_secret"}}"#)
            .unwrap();
        assert_eq!(c.bearer.as_deref(), Some("sentinel_secret"));
        assert_eq!(c.environment["TOKEN"], "another_secret");
        assert!(!format!("{c:?}").contains("secret"));
        assert!(parse("{}").is_ok());
    }
    #[test]
    fn rejects_header_injection_unknown_fields_and_excessive_credentials() {
        for bad in [
            r#"{"bearer":"x\r\nAuthorization: injected"}"#,
            r#"{"password":"do-not-echo"}"#,
            r#"{"environment":{"BAD=KEY":"v"}}"#,
            r#"{"environment":{"TOKEN":"x\u0000y"}}"#,
        ] {
            let error = parse(bad).unwrap_err();
            assert!(!error.contains("do-not-echo"));
        }
        assert!(parse(&" ".repeat(32_769)).is_err());
    }
}
