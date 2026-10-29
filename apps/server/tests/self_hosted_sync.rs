#![allow(clippy::unwrap_used, clippy::panic, reason = "test code")]
//! The self-hosted server as the shared source of truth: two clients (desktop
//! and Android in server mode) talk to the same API and database. Covers fixed
//! deposits, insurance policies, premium payments, concurrent edits, duplicate
//! requests, deletions and the payment-method summary.
use std::{net::SocketAddr, time::Duration};

use axum::{
    body::{to_bytes, Body},
    http::{Method, Request, StatusCode},
    Router,
};
use serde_json::{json, Value};
use tempfile::{tempdir, TempDir};
use tower::ServiceExt;
use wealthfolio_server::{api::app_router, build_state, config::Config};

fn test_config(db_path: String, addons_root: String) -> Config {
    Config {
        listen_addr: "127.0.0.1:0".parse::<SocketAddr>().unwrap(),
        db_path,
        cors_allow: vec!["*".to_string()],
        request_timeout: Duration::from_secs(30),
        static_dir: "dist".to_string(),
        addons_root,
        raw_secret_key: vec![7; 32],
        secrets_encryption_key: [7; 32],
        database_key: [9; 32],
        db_encryption_required: false,
        auth: None,
        oidc: None,
        mcp_enabled: false,
        mcp_audit_enabled: true,
        mcp_allowed_hosts: None,
    }
}

async fn server() -> (Router, TempDir) {
    let dir = tempdir().unwrap();
    let db = dir.path().join("app.db").to_string_lossy().into_owned();
    let addons = dir.path().join("addons").to_string_lossy().into_owned();
    let config = test_config(db, addons);
    let state = build_state(&config).await.unwrap();
    (app_router(state, &config).unwrap(), dir)
}

async fn call(app: &Router, method: Method, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(body.map_or_else(Body::empty, |b| Body::from(b.to_string())))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let value = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()));
    (status, value)
}

async fn holdings(app: &Router) -> Vec<Value> {
    let (status, body) = call(app, Method::GET, "/api/v1/alternative-holdings", None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body.as_array().unwrap().clone()
}

async fn asset_updated_at(app: &Router, id: &str) -> String {
    let (_, body) = call(app, Method::GET, "/api/v1/assets", None).await;
    body.as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == id)
        .unwrap()["updatedAt"]
        .as_str()
        .unwrap()
        .to_string()
}

fn fixed_deposit() -> Value {
    json!({
        "kind": "other",
        "name": "SBI FD",
        "currency": "INR",
        "currentValue": "100000",
        "valueDate": "2023-01-01",
        "metadata": {
            "sub_type": "fixed_deposit",
            "fd_issuer": "State Bank of India",
            "fd_principal": "100000",
            "fd_interest_rate": "7",
            "fd_interest_method": "compound",
            "fd_compounding": "quarterly",
            "fd_start_date": "2023-01-01",
            "fd_maturity_date": "2024-01-01"
        }
    })
}

#[tokio::test]
async fn deposit_created_on_one_client_is_seen_edited_and_deleted_on_the_other() {
    let (server, _dir) = server().await;
    // Two devices in server mode share one server.
    let (android, mac) = (server.clone(), server.clone());

    let (status, created) = call(
        &android,
        Method::POST,
        "/api/v1/alternative-assets",
        Some(fixed_deposit()),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let id = created["assetId"].as_str().unwrap().to_string();

    // The other device sees it, valued from its terms.
    let on_mac = holdings(&mac).await;
    let deposit = on_mac.iter().find(|h| h["id"] == id.as_str()).unwrap();
    assert_eq!(deposit["fixedDeposit"]["expectedMaturityAmount"], 107185.9);
    assert_eq!(deposit["fixedDeposit"]["status"], "matured");

    // Both devices load the same version; the Mac saves first.
    let loaded = asset_updated_at(&mac, &id).await;
    let edit = |rate: &str, version: &str| {
        json!({
            "metadata": { "fd_interest_rate": rate },
            "expectedUpdatedAt": version,
        })
    };
    let uri = format!("/api/v1/alternative-assets/{id}/metadata");
    let (status, body) = call(&mac, Method::PUT, &uri, Some(edit("7.5", &loaded))).await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
    // Android's stale edit is refused instead of overwriting the newer one.
    let (status, body) = call(&android, Method::PUT, &uri, Some(edit("6", &loaded))).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    let after = holdings(&android).await;
    let deposit = after.iter().find(|h| h["id"] == id.as_str()).unwrap();
    assert_eq!(deposit["fixedDeposit"]["annualRatePct"], 7.5);

    // Invalid terms are rejected by the server.
    let fresh = asset_updated_at(&android, &id).await;
    let bad =
        json!({ "metadata": { "fd_maturity_date": "2022-01-01" }, "expectedUpdatedAt": fresh });
    let (status, _) = call(&android, Method::PUT, &uri, Some(bad)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Deletion on one device removes it for the other.
    let (status, _) = call(
        &android,
        Method::DELETE,
        &format!("/api/v1/alternative-assets/{id}"),
        None,
    )
    .await;
    assert!(status.is_success());
    assert!(holdings(&mac).await.iter().all(|h| h["id"] != id.as_str()));
}

#[tokio::test]
async fn premium_retries_from_two_devices_are_recorded_once() {
    let (server, _dir) = server().await;
    let (status, account) = call(
        &server,
        Method::POST,
        "/api/v1/accounts",
        Some(json!({
            "name": "HDFC Savings",
            "accountType": "CASH",
            "currency": "INR",
            "isDefault": false,
            "isActive": true,
            "trackingMode": "TRANSACTIONS"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{account}");
    let account_id = account["id"].as_str().unwrap().to_string();

    let (status, created) = call(
        &server,
        Method::POST,
        "/api/v1/alternative-assets",
        Some(json!({
            "kind": "other",
            "name": "Jeevan Anand",
            "currency": "INR",
            "currentValue": "30000",
            "valueDate": "2020-01-01",
            "metadata": {
                "sub_type": "insurance_policy",
                "policy_insurer": "LIC",
                "policy_premium_amount": "10000",
                "policy_premium_frequency": "yearly",
                "policy_commencement_date": "2020-01-01",
                "policy_term_years": "20",
                "policy_premium_term_years": "20",
                "policy_premiums_paid_before": "3"
            }
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let policy_id = created["assetId"].as_str().unwrap().to_string();

    let premium = json!({
        "accountId": account_id,
        "activityType": "WITHDRAWAL",
        "activityDate": "2023-01-05T06:30:00Z",
        "amount": 10000,
        "currency": "INR",
        "comment": "LIC premium",
        "idempotencyKey": format!("policy-premium:{policy_id}:4"),
        "metadata": "{\"policy_premium\":{\"policy_id\":\"POLICY\",\"installment\":4},\"payment_method\":\"upi\"}".replace("POLICY", &policy_id)
    });
    let (status, body) = call(
        &server,
        Method::POST,
        "/api/v1/activities",
        Some(premium.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["metadata"]["payment_method"] == "upi", "{body}");
    // The same request again (retry, or the other device) is refused.
    let (status, body) = call(&server, Method::POST, "/api/v1/activities", Some(premium)).await;
    assert!(!status.is_success(), "duplicate accepted: {body}");
    assert!(body.to_string().contains("Duplicate"), "{body}");

    let all = holdings(&server).await;
    let policy = all.iter().find(|h| h["id"] == policy_id.as_str()).unwrap();
    assert_eq!(policy["policy"]["premiumsPaid"], 4);
    assert_eq!(policy["policy"]["totalPremiumsPaid"], 40000.0);
    assert_eq!(policy["policy"]["payments"].as_array().unwrap().len(), 1);
    assert_eq!(policy["marketValue"], "40000");
}

#[tokio::test]
async fn payment_method_summary_is_served_to_both_runtimes() {
    let (server, _dir) = server().await;
    let (status, body) = call(
        &server,
        Method::POST,
        "/api/v1/spending/payment-methods/summary",
        Some(json!({ "paymentMethods": ["upi"] })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["methods"].is_array(), "{body}");
}

#[tokio::test]
async fn new_endpoints_require_authentication() {
    use argon2::{password_hash::SaltString, Argon2, PasswordHasher};
    use rand::rngs::OsRng;
    use wealthfolio_server::auth::{AuthConfig, CookieSecurePolicy};

    let dir = tempdir().unwrap();
    let mut config = test_config(
        dir.path().join("app.db").to_string_lossy().into_owned(),
        dir.path().join("addons").to_string_lossy().into_owned(),
    );
    config.cors_allow = vec!["https://wealthfolio.example.ts.net".into()];
    let salt = SaltString::generate(&mut OsRng);
    config.auth = Some(AuthConfig {
        password_hash: Some(
            Argon2::default()
                .hash_password(b"correct horse", &salt)
                .unwrap()
                .to_string(),
        ),
        jwt_secret: vec![3; 32],
        access_token_ttl: Duration::from_secs(600),
        cookie_secure: CookieSecurePolicy::Always,
    });
    let state = build_state(&config).await.unwrap();
    let app = app_router(state, &config).unwrap();

    for (method, uri, body) in [
        (Method::GET, "/api/v1/alternative-holdings", None),
        (
            Method::POST,
            "/api/v1/alternative-assets",
            Some(fixed_deposit()),
        ),
        (
            Method::PUT,
            "/api/v1/alternative-assets/x/metadata",
            Some(json!({ "metadata": {} })),
        ),
        (
            Method::POST,
            "/api/v1/spending/payment-methods/summary",
            Some(json!({})),
        ),
        (Method::POST, "/api/v1/activities", Some(json!({}))),
    ] {
        let (status, _) = call(&app, method.clone(), uri, body).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {uri}");
    }
    // The connection test used by the apps stays public and reveals nothing else.
    let (status, body) = call(&app, Method::GET, "/api/v1/auth/status", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["requiresPassword"], true);
}

/// The apps and browsers open the server at its Tailscale IP over plain HTTP
/// (`http://100.x.y.z:8088`). The login cookie must not be marked Secure there,
/// or the browser would drop it and every later request would be unauthorized.
#[tokio::test]
async fn login_works_over_plain_http_on_a_tailscale_ip() {
    use argon2::{password_hash::SaltString, Argon2, PasswordHasher};
    use axum::extract::ConnectInfo;
    use axum::http::header;
    use rand::rngs::OsRng;
    use wealthfolio_server::auth::{AuthConfig, CookieSecurePolicy};

    const ORIGIN: &str = "http://100.101.102.103:8088";
    let dir = tempdir().unwrap();
    let mut config = test_config(
        dir.path().join("app.db").to_string_lossy().into_owned(),
        dir.path().join("addons").to_string_lossy().into_owned(),
    );
    config.listen_addr = "0.0.0.0:8088".parse().unwrap();
    config.cors_allow = vec![ORIGIN.into()];
    let salt = SaltString::generate(&mut OsRng);
    config.auth = Some(AuthConfig {
        password_hash: Some(
            Argon2::default()
                .hash_password(b"correct horse", &salt)
                .unwrap()
                .to_string(),
        ),
        jwt_secret: vec![5; 32],
        access_token_ttl: Duration::from_secs(600),
        // What compose.tailscale.yml sets (WF_COOKIE_SECURE=auto).
        cookie_secure: CookieSecurePolicy::Auto,
    });
    let state = build_state(&config).await.unwrap();
    let app = app_router(state, &config).unwrap();

    let mut login = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/auth/login")
        .header(header::HOST, "100.101.102.103:8088")
        .header(header::ORIGIN, ORIGIN)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({ "password": "correct horse" }).to_string(),
        ))
        .unwrap();
    login
        .extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([100, 64, 0, 7], 50000))));
    let response = app.clone().oneshot(login).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    assert!(cookie.contains("HttpOnly"), "{cookie}");
    assert!(
        !cookie.contains("Secure"),
        "cookie would be dropped over HTTP: {cookie}"
    );
    let session = cookie.split(';').next().unwrap().to_owned();

    // Same-origin API call from the page served at the Tailscale IP.
    let request = Request::builder()
        .uri("/api/v1/alternative-holdings")
        .header(header::HOST, "100.101.102.103:8088")
        .header(header::ORIGIN, ORIGIN)
        .header(header::COOKIE, &session)
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // The connection test the apps run before saving the address.
    let (status, _) = call(&app, Method::GET, "/api/v1/healthz", None).await;
    assert_eq!(status, StatusCode::OK);
}
