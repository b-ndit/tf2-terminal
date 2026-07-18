use std::collections::HashMap;
use std::time::Duration;

use crate::domain::steam_id::SteamId64;
use crate::error::{AppError, AppResult};

const STEAM_OPENID_URL: &str = "https://steamcommunity.com/openid/login";
const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);

const SUCCESS_PAGE: &str = "\
<!doctype html><html><body style=\"font-family: sans-serif; text-align: center; padding-top: 4rem;\">\
<h2>Signed in</h2><p>You can close this tab and return to TF2 Terminal.</p>\
</body></html>";

const CANCELLED_PAGE: &str = "\
<!doctype html><html><body style=\"font-family: sans-serif; text-align: center; padding-top: 4rem;\">\
<h2>Sign-in cancelled</h2><p>You can close this tab.</p>\
</body></html>";

/// Builds the URL the system browser is sent to for Steam OpenID 2.0 login.
/// `return_to`/`realm` point back at our loopback listener.
fn build_login_url(return_to: &str, realm: &str) -> String {
    let params = [
        ("openid.ns", "http://specs.openid.net/auth/2.0"),
        ("openid.mode", "checkid_setup"),
        ("openid.return_to", return_to),
        ("openid.realm", realm),
        (
            "openid.identity",
            "http://specs.openid.net/auth/2.0/identifier_select",
        ),
        (
            "openid.claimed_id",
            "http://specs.openid.net/auth/2.0/identifier_select",
        ),
    ];
    let query = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(params)
        .finish();
    format!("{STEAM_OPENID_URL}?{query}")
}

/// Re-posts the callback's `openid.*` params to Steam with
/// `openid.mode=check_authentication` to confirm the assertion wasn't
/// forged (required by the OpenID 2.0 spec — a callback alone isn't proof).
/// `verify_url` is `STEAM_OPENID_URL` in production; parameterized so tests
/// can point it at a local server.
async fn verify_assertion(
    http: &reqwest::Client,
    params: &HashMap<String, String>,
    verify_url: &str,
) -> AppResult<bool> {
    let mut verify_params: Vec<(String, String)> =
        params.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    verify_params.retain(|(k, _)| k != "openid.mode");
    verify_params.push((
        "openid.mode".to_string(),
        "check_authentication".to_string(),
    ));

    let response = http
        .post(verify_url)
        .form(&verify_params)
        .send()
        .await?
        .error_for_status()?;
    let body = response.text().await?;

    Ok(body.lines().any(|line| line.trim() == "is_valid:true"))
}

fn parse_callback_query(url_path_and_query: &str) -> HashMap<String, String> {
    let full_url = format!("http://127.0.0.1{url_path_and_query}");
    let Ok(parsed) = url::Url::parse(&full_url) else {
        return HashMap::new();
    };
    parsed.query_pairs().into_owned().collect()
}

/// WSL interop only forwards an environment variable to a spawned Windows
/// process if it's listed in `WSLENV` — `open` 5.4.0's WSL code path sets
/// `OPEN_RS_TARGET` on its `powershell.exe` child (to work around
/// `wslview` being discontinued) but never adds it to `WSLENV` itself, so
/// on an unconfigured WSL install `$env:OPEN_RS_TARGET` is empty inside
/// PowerShell and `Start-Process -FilePath $env:OPEN_RS_TARGET` fails —
/// verified live against a real WSL2 install. Appends rather than
/// overwrites so any `WSLENV` entries the user already has keep working;
/// a no-op everywhere outside WSL, since nothing else reads `WSLENV`.
fn ensure_open_target_crosses_wsl_interop() {
    const VAR: &str = "OPEN_RS_TARGET";
    let existing = std::env::var("WSLENV").unwrap_or_default();
    if existing
        .split(':')
        .any(|entry| entry.split('/').next() == Some(VAR))
    {
        return;
    }
    let updated = if existing.is_empty() {
        VAR.to_string()
    } else {
        format!("{existing}:{VAR}")
    };
    // SAFETY: called from `login_via_browser`, which only ever runs on the
    // single-threaded-at-this-point startup/command path — no concurrent
    // env reads race this write in practice.
    unsafe { std::env::set_var("WSLENV", updated) };
}

/// Runs the full Steam OpenID login flow: starts a one-shot loopback HTTP
/// listener, opens the system browser to Steam's login page, waits for the
/// redirect, verifies the assertion, and returns the resulting SteamID64.
/// We never see the user's Steam credentials — only this identifier.
pub async fn login_via_browser() -> AppResult<SteamId64> {
    let server = tiny_http::Server::http("127.0.0.1:0")
        .map_err(|e| AppError::Internal(format!("failed to start loopback listener: {e}")))?;

    let port = server
        .server_addr()
        .to_ip()
        .ok_or_else(|| AppError::Internal("loopback listener has no IP address".to_string()))?
        .port();

    let return_to = format!("http://127.0.0.1:{port}/callback");
    let realm = format!("http://127.0.0.1:{port}/");
    let login_url = build_login_url(&return_to, &realm);

    ensure_open_target_crosses_wsl_interop();
    open::that(&login_url)
        .map_err(|e| AppError::Internal(format!("failed to open system browser: {e}")))?;

    let request = tokio::task::spawn_blocking(move || server.recv_timeout(LOGIN_TIMEOUT))
        .await
        .map_err(|e| AppError::Internal(format!("login listener task panicked: {e}")))?
        .map_err(|e| AppError::Internal(format!("login listener error: {e}")))?
        .ok_or_else(|| AppError::Config("timed out waiting for Steam login".to_string()))?;

    let query = parse_callback_query(request.url());
    let cancelled = query.get("openid.mode").map(String::as_str) != Some("id_res");

    let page = if cancelled {
        CANCELLED_PAGE
    } else {
        SUCCESS_PAGE
    };
    let response = tiny_http::Response::from_string(page).with_header(
        tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..])
            .expect("static header is valid"),
    );
    request
        .respond(response)
        .map_err(|e| AppError::Internal(format!("failed to respond to browser: {e}")))?;

    if cancelled {
        return Err(AppError::Config(
            "Steam login was cancelled or denied".to_string(),
        ));
    }

    let http = reqwest::Client::new();
    if !verify_assertion(&http, &query, STEAM_OPENID_URL).await? {
        return Err(AppError::Config(
            "Steam login assertion failed verification".to_string(),
        ));
    }

    let claimed_id = query
        .get("openid.claimed_id")
        .ok_or_else(|| AppError::Config("Steam login response missing claimed_id".to_string()))?;

    SteamId64::parse_claimed_id(claimed_id)
        .map_err(|e| AppError::Config(format!("invalid SteamID64 in login response: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `WSLENV` is process-global state, so both scenarios live in one test
    /// — `cargo test` runs tests in parallel by default, and two tests each
    /// mutating the same env var raced and flaked when this was split.
    #[test]
    fn ensure_open_target_crosses_wsl_interop_sets_and_dedupes() {
        unsafe { std::env::remove_var("WSLENV") };
        ensure_open_target_crosses_wsl_interop();
        assert_eq!(std::env::var("WSLENV").unwrap(), "OPEN_RS_TARGET");

        unsafe { std::env::set_var("WSLENV", "FOO/p:BAR") };
        ensure_open_target_crosses_wsl_interop();
        assert_eq!(std::env::var("WSLENV").unwrap(), "FOO/p:BAR:OPEN_RS_TARGET");

        // Calling it again shouldn't append a second time.
        ensure_open_target_crosses_wsl_interop();
        assert_eq!(std::env::var("WSLENV").unwrap(), "FOO/p:BAR:OPEN_RS_TARGET");

        unsafe { std::env::remove_var("WSLENV") };
    }

    #[test]
    fn build_login_url_includes_required_openid_params() {
        let url = build_login_url("http://127.0.0.1:12345/callback", "http://127.0.0.1:12345/");
        assert!(url.starts_with(STEAM_OPENID_URL));
        assert!(url.contains("openid.mode=checkid_setup"));
        assert!(url.contains("openid.return_to=http%3A%2F%2F127.0.0.1%3A12345%2Fcallback"));
        assert!(url.contains("openid.realm=http%3A%2F%2F127.0.0.1%3A12345%2F"));
        assert!(url.contains("identifier_select"));
    }

    #[test]
    fn parse_callback_query_extracts_params() {
        let query = parse_callback_query(
            "/callback?openid.mode=id_res&openid.claimed_id=https%3A%2F%2Fsteamcommunity.com%2Fopenid%2Fid%2F76561198000000000",
        );
        assert_eq!(query.get("openid.mode"), Some(&"id_res".to_string()));
        assert_eq!(
            query.get("openid.claimed_id"),
            Some(&"https://steamcommunity.com/openid/id/76561198000000000".to_string())
        );
    }

    #[test]
    fn parse_callback_query_handles_malformed_input() {
        let query = parse_callback_query("not a valid url fragment");
        assert!(query.is_empty());
    }

    /// Spins up a one-shot local `tiny_http` server that replies with
    /// `response_body`, returning its base URL.
    fn spawn_one_shot_server(response_body: &'static str) -> String {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let port = server.server_addr().to_ip().unwrap().port();
        std::thread::spawn(move || {
            if let Ok(Some(request)) = server.recv_timeout(Duration::from_secs(5)) {
                let _ = request.respond(tiny_http::Response::from_string(response_body));
            }
        });
        format!("http://127.0.0.1:{port}/openid/login")
    }

    #[tokio::test]
    async fn verify_assertion_accepts_is_valid_true() {
        let url = spawn_one_shot_server("ns:http://specs.openid.net/auth/2.0\nis_valid:true\n");
        let http = reqwest::Client::new();
        let params = HashMap::from([("openid.mode".to_string(), "id_res".to_string())]);

        assert!(verify_assertion(&http, &params, &url).await.unwrap());
    }

    #[tokio::test]
    async fn verify_assertion_rejects_when_response_lacks_is_valid_true() {
        let url = spawn_one_shot_server("ns:http://specs.openid.net/auth/2.0\nis_valid:false\n");
        let http = reqwest::Client::new();
        let params = HashMap::from([("openid.mode".to_string(), "id_res".to_string())]);

        assert!(!verify_assertion(&http, &params, &url).await.unwrap());
    }
}
