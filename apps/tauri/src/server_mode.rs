//! Self-hosted server mode.
//!
//! The desktop and mobile apps are local-first: each keeps its own SQLite
//! database. Copying that database between devices is not safe, so to share one
//! set of data the app can instead open a self-hosted Wealthfolio server (the
//! Docker image, reached for example through a Tailscale HTTPS URL). In server
//! mode the main window loads the server's own web app; every read and write
//! goes to the server's API, the server's SQLite database is the only copy of
//! the data, and the server's login (password or OIDC) protects it. Changes made
//! on one device reach the others through the server's event stream.
//!
//! What is stored here is only the server URL, in the app config directory. It
//! is not a secret. Credentials are never stored by the app: the server's
//! HttpOnly session cookie lives in the platform web view's cookie store, and
//! signing out on the server removes it.
//!
//! The remote page is given no access to the app's commands (the app's
//! capabilities are local-only). To leave server mode it navigates back to the
//! app's own origin with `?wf_disconnect=1`, which the navigation hook below
//! handles.

use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::plugin::{Builder as PluginBuilder, TauriPlugin};
use tauri::{AppHandle, Manager, Runtime, Url};

const CONFIG_FILE: &str = "server-connection.json";
const DISCONNECT_PARAM: &str = "wf_disconnect";
/// Query parameter telling the server's web app which origin to return to.
const SHELL_RETURN_PARAM: &str = "wf_shell";
const HEALTH_PATH: &str = "/api/v1/healthz";
const AUTH_STATUS_PATH: &str = "/api/v1/auth/status";
const TEST_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ServerConnection {
    pub server_url: String,
    /// RFC3339 time the server last answered a connection check.
    pub last_verified_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ServerTestResult {
    pub server_url: String,
    pub reachable: bool,
    /// Whether the server requires signing in (`/auth/status`).
    pub auth_required: Option<bool>,
    pub http_status: Option<u16>,
    pub error: Option<String>,
}

/// Normalizes and validates a server URL.
///
/// HTTPS is required, because the session cookie and financial data cross the
/// network. Plain HTTP is accepted only for this device's loopback address
/// (local testing). Paths, queries and credentials in the URL are refused: the
/// server is addressed by origin.
pub fn normalize_server_url(input: &str) -> Result<Url, String> {
    let trimmed = input.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err("Enter the server URL".into());
    }
    let with_scheme = if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        // A bare Tailscale IP (e.g. 100.101.102.103:8088) is served over plain
        // HTTP inside the tailnet; any other bare name is assumed to be HTTPS.
        let bare = Url::parse(&format!("http://{trimmed}")).ok();
        if bare.as_ref().is_some_and(is_tailnet_ip) {
            format!("http://{trimmed}")
        } else {
            format!("https://{trimmed}")
        }
    };
    let url = Url::parse(&with_scheme).map_err(|_| "This is not a valid URL".to_string())?;
    let host = url
        .host_str()
        .ok_or_else(|| "The URL has no host name".to_string())?
        .to_ascii_lowercase();
    let loopback = host == "localhost" || host == "127.0.0.1" || host == "[::1]";
    match url.scheme() {
        "https" => {}
        // Plain HTTP only where the network itself is private and encrypted:
        // this device (testing) or a Tailscale address, whose traffic travels
        // inside WireGuard. Other HTTP addresses would expose the session cookie.
        "http" if loopback || is_tailnet_ip(&url) || host.ends_with(".ts.net") => {}
        "http" => {
            return Err(
                "Use https://, or http:// with your Tailscale IP (100.x.y.z) or .ts.net name"
                    .into(),
            )
        }
        _ => return Err("Only https:// and http:// URLs are supported".into()),
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("Do not put a user name or password in the URL".into());
    }
    if url.path() != "/" && !url.path().is_empty() {
        return Err("Enter only the server address, without a path".into());
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err("Enter only the server address, without ? or #".into());
    }
    if host == "tauri.localhost" {
        return Err("This address belongs to the app itself".into());
    }
    Ok(url)
}

/// Whether the URL's host is a Tailscale address: IPv4 100.64.0.0/10 (the
/// CGNAT range Tailscale assigns) or IPv6 fd7a:115c:a1e0::/48.
pub fn is_tailnet_ip(url: &Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    let host = host.trim_start_matches('[').trim_end_matches(']');
    match host.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(ip)) => {
            let [a, b, ..] = ip.octets();
            a == 100 && (64..=127).contains(&b)
        }
        Ok(std::net::IpAddr::V6(ip)) => {
            let s = ip.segments();
            s[0] == 0xfd7a && s[1] == 0x115c && s[2] == 0xa1e0
        }
        Err(_) => false,
    }
}

/// The app's own origin as the web view sees it.
fn is_app_origin(url: &Url) -> bool {
    matches!(
        (url.scheme(), url.host_str()),
        ("tauri", Some("localhost")) | ("http" | "https", Some("tauri.localhost"))
    )
}

/// Whether a navigation is the remote page asking to leave server mode.
pub fn is_disconnect_request(url: &Url) -> bool {
    is_app_origin(url)
        && url
            .query_pairs()
            .any(|(k, v)| k == DISCONNECT_PARAM && v == "1")
}

/// URL the window opens: the server's origin, told where to return on disconnect.
pub fn server_entry_url(server: &Url, app_origin: &str) -> Url {
    let mut entry = server.clone();
    entry
        .query_pairs_mut()
        .append_pair(SHELL_RETURN_PARAM, app_origin);
    entry
}

fn config_path<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("No app config directory: {e}"))?;
    Ok(dir.join(CONFIG_FILE))
}

pub fn load_connection<R: Runtime>(app: &AppHandle<R>) -> Option<ServerConnection> {
    let path = config_path(app).ok()?;
    let text = std::fs::read_to_string(path).ok()?;
    let connection: ServerConnection = serde_json::from_str(&text).ok()?;
    // A hand-edited or old file with an unsafe URL is ignored, not followed.
    normalize_server_url(&connection.server_url).ok()?;
    Some(connection)
}

fn save_connection<R: Runtime>(
    app: &AppHandle<R>,
    connection: &ServerConnection,
) -> Result<(), String> {
    let path = config_path(app)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("Cannot save settings: {e}"))?;
    }
    let text = serde_json::to_string_pretty(connection).map_err(|e| e.to_string())?;
    // Write then rename, so a crash cannot leave a half-written file.
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text).map_err(|e| format!("Cannot save settings: {e}"))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("Cannot save settings: {e}"))
}

pub fn clear_connection<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let path = config_path(app)?;
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("Cannot remove server settings: {e}")),
    }
}

async fn check_server(url: &Url) -> ServerTestResult {
    let mut result = ServerTestResult {
        server_url: url.as_str().trim_end_matches('/').to_string(),
        reachable: false,
        auth_required: None,
        http_status: None,
        error: None,
    };
    // Verified TLS with the app's standard roots; no redirects to other hosts.
    let client = match wealthfolio_http::client_builder()
        .timeout(TEST_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
    {
        Ok(client) => client,
        Err(e) => {
            result.error = Some(format!("Could not start a secure connection: {e}"));
            return result;
        }
    };
    let health = match url.join(HEALTH_PATH) {
        Ok(health) => health,
        Err(e) => {
            result.error = Some(e.to_string());
            return result;
        }
    };
    match client.get(health).send().await {
        Ok(response) => {
            result.http_status = Some(response.status().as_u16());
            if response.status().is_success() {
                result.reachable = true;
            } else {
                result.error = Some(format!(
                    "The server answered {} at {HEALTH_PATH}. Is this a Wealthfolio server?",
                    response.status()
                ));
                return result;
            }
        }
        Err(e) => {
            result.error = Some(if e.is_timeout() {
                "The server did not answer in time. Check that it is running and that this device is connected to your tailnet.".to_string()
            } else if e.is_connect() {
                "Could not connect. Check the address, that the server is running, and that this device is connected to your tailnet.".to_string()
            } else {
                // TLS verification failures land here; they are never bypassed.
                format!("Connection failed: {e}")
            });
            return result;
        }
    }
    if let Ok(status_url) = url.join(AUTH_STATUS_PATH) {
        if let Ok(response) = client.get(status_url).send().await {
            if let Ok(body) = response.json::<serde_json::Value>().await {
                let flag = |key: &str| body.get(key).and_then(|v| v.as_bool());
                result.auth_required = match (flag("requiresPassword"), flag("oidcEnabled")) {
                    (None, None) => None,
                    (password, oidc) => Some(password.unwrap_or(false) || oidc.unwrap_or(false)),
                };
            }
        }
    }
    result
}

/// The saved server, if this device uses one.
#[tauri::command]
pub fn get_server_connection<R: Runtime>(app: AppHandle<R>) -> Option<ServerConnection> {
    load_connection(&app)
}

/// Checks that a URL answers as a Wealthfolio server, without saving anything.
#[tauri::command]
pub async fn test_server_connection(server_url: String) -> Result<ServerTestResult, String> {
    let url = normalize_server_url(&server_url)?;
    Ok(check_server(&url).await)
}

/// Saves the server and opens it in the main window when it answers.
#[tauri::command]
pub async fn connect_server<R: Runtime>(
    app: AppHandle<R>,
    server_url: String,
) -> Result<ServerTestResult, String> {
    let url = normalize_server_url(&server_url)?;
    let result = check_server(&url).await;
    if !result.reachable {
        return Ok(result);
    }
    save_connection(
        &app,
        &ServerConnection {
            server_url: result.server_url.clone(),
            last_verified_at: Some(chrono::Utc::now().to_rfc3339()),
        },
    )?;
    open_in_main_window(&app, &url)?;
    Ok(result)
}

/// Opens the saved server (used at startup). Leaves the window on the local app
/// and returns the failure when the server does not answer.
#[tauri::command]
pub async fn open_saved_server<R: Runtime>(app: AppHandle<R>) -> Result<ServerTestResult, String> {
    let connection = load_connection(&app).ok_or("No server is saved on this device")?;
    let url = normalize_server_url(&connection.server_url)?;
    let result = check_server(&url).await;
    if result.reachable {
        save_connection(
            &app,
            &ServerConnection {
                last_verified_at: Some(chrono::Utc::now().to_rfc3339()),
                ..connection
            },
        )?;
        open_in_main_window(&app, &url)?;
    }
    Ok(result)
}

/// Forgets the server; the app uses this device's local data again.
#[tauri::command]
pub fn disconnect_server<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    clear_connection(&app)
}

fn open_in_main_window<R: Runtime>(app: &AppHandle<R>, server: &Url) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or("The main window is not open")?;
    let current = window.url().map_err(|e| e.to_string())?;
    let origin = current.origin().ascii_serialization();
    window
        .navigate(server_entry_url(server, &origin))
        .map_err(|e| format!("Could not open the server: {e}"))
}

/// Watches main-window navigations for the remote page's disconnect request.
pub fn plugin<R: Runtime>() -> TauriPlugin<R> {
    PluginBuilder::new("server-mode")
        .on_navigation(|webview, url| {
            if is_disconnect_request(url) {
                if let Err(e) = clear_connection(webview.app_handle()) {
                    log::warn!("Server disconnect failed: {e}");
                }
            }
            true
        })
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_https_and_tailscale_names() {
        assert_eq!(
            normalize_server_url("https://wealthfolio.example.ts.net/")
                .unwrap()
                .as_str(),
            "https://wealthfolio.example.ts.net/"
        );
        // A bare host name gets https.
        assert_eq!(
            normalize_server_url("wealthfolio.example.ts.net")
                .unwrap()
                .scheme(),
            "https"
        );
        assert!(normalize_server_url("https://wf.example.ts.net:8443").is_ok());
        assert!(normalize_server_url("http://127.0.0.1:8088").is_ok());
    }

    #[test]
    fn accepts_plain_http_to_tailscale_addresses() {
        let ip = normalize_server_url("http://100.101.102.103:8088").unwrap();
        assert_eq!(ip.as_str(), "http://100.101.102.103:8088/");
        // A bare Tailscale IP defaults to http, keeping the port.
        let bare = normalize_server_url("100.101.102.103:8088").unwrap();
        assert_eq!(bare.as_str(), "http://100.101.102.103:8088/");
        assert!(normalize_server_url("http://100.64.0.1:8088").is_ok());
        assert!(normalize_server_url("http://100.127.255.254:8088").is_ok());
        assert!(normalize_server_url("http://[fd7a:115c:a1e0::1]:8088").is_ok());
        assert!(normalize_server_url("http://wealthfolio.example.ts.net:8088").is_ok());
    }

    #[test]
    fn refuses_unsafe_or_ambiguous_urls() {
        assert!(normalize_server_url("").is_err());
        // HTTP outside the tailnet range is refused.
        assert!(normalize_server_url("http://192.168.1.10:8088").is_err());
        assert!(normalize_server_url("http://100.128.0.1:8088").is_err());
        assert!(normalize_server_url("http://100.63.255.255:8088").is_err());
        assert!(normalize_server_url("http://example.com").is_err());
        assert!(normalize_server_url("ftp://example.com").is_err());
        assert!(normalize_server_url("https://user:pw@example.com").is_err());
        assert!(normalize_server_url("https://example.com/app").is_err());
        assert!(normalize_server_url("https://example.com/?x=1").is_err());
        assert!(normalize_server_url("https://tauri.localhost").is_err());
    }

    #[test]
    fn recognizes_disconnect_requests_only_on_the_app_origin() {
        let yes = |s: &str| is_disconnect_request(&Url::parse(s).unwrap());
        assert!(yes("tauri://localhost/?wf_disconnect=1"));
        assert!(yes("http://tauri.localhost/settings?wf_disconnect=1"));
        assert!(yes("https://tauri.localhost/?wf_disconnect=1"));
        assert!(!yes("https://evil.example.com/?wf_disconnect=1"));
        assert!(!yes("tauri://localhost/?wf_disconnect=0"));
        assert!(!yes("tauri://localhost/"));
    }

    #[test]
    fn entry_url_carries_the_return_origin() {
        let server = Url::parse("https://wf.example.ts.net").unwrap();
        assert_eq!(
            server_entry_url(&server, "tauri://localhost").as_str(),
            "https://wf.example.ts.net/?wf_shell=tauri%3A%2F%2Flocalhost"
        );
    }
}
