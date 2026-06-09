use super::PluginState;
use axum::{
    body::Body,
    extract::{Path as AxPath, State as AxState},
    http::{header, StatusCode},
    response::{Json, Response},
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{net::IpAddr, sync::Arc, time::Duration};

const PEER_PROXY_TIMEOUT: Duration = Duration::from_secs(5);

/// Helper to validate that a target (addr, port) is a known discovered LAN peer
/// and represents a valid private/local IP address to prevent arbitrary SSRF.
fn validate_peer(state: &PluginState, addr: &str, port: u16) -> Result<(), StatusCode> {
    // 1. Check if the peer is in the discovered peers snapshot
    let peers = state.lan.snapshot();
    let found = peers.iter().any(|p| p.addr == addr && p.file_server_port == port);
    if !found {
        tracing::warn!(addr = %addr, port = port, "Rejecting LAN request: peer not in discovered list");
        return Err(StatusCode::FORBIDDEN);
    }

    // 2. Parse address as IpAddr
    let ip: IpAddr = addr.parse().map_err(|_| {
        tracing::warn!(addr = %addr, "Rejecting LAN request: invalid IP address format");
        StatusCode::BAD_REQUEST
    })?;

    // 3. Reject loopback, multicast, or unspecified IP
    if ip.is_loopback() || ip.is_multicast() || ip.is_unspecified() {
        tracing::warn!(addr = %addr, "Rejecting LAN request: loopback, multicast, or unspecified IP");
        return Err(StatusCode::FORBIDDEN);
    }

    // 4. Validate IP subnet ranges (defense-in-depth)
    match ip {
        IpAddr::V4(v4) => {
            let octets = v4.octets();
            // Reject link-local (169.254.0.0/16) and metadata (169.254.169.254)
            if octets[0] == 169 && octets[1] == 254 {
                tracing::warn!(addr = %addr, "Rejecting LAN request: link-local IP");
                return Err(StatusCode::FORBIDDEN);
            }
            // Allow RFC 1918 private subnets: 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16
            let is_rfc1918 = octets[0] == 10
                || (octets[0] == 172 && octets[1] >= 16 && octets[1] <= 31)
                || (octets[0] == 192 && octets[1] == 168);
            // Allow CGNAT (Tailscale, etc.): 100.64.0.0/10
            let is_cgnat = octets[0] == 100 && (octets[1] & 0xc0) == 64;
            if !is_rfc1918 && !is_cgnat {
                tracing::warn!(addr = %addr, "Rejecting LAN request: IP is not RFC1918 or CGNAT");
                return Err(StatusCode::FORBIDDEN);
            }
        }
        IpAddr::V6(v6) => {
            let octets = v6.octets();
            // Reject link-local (fe80::/10)
            if octets[0] == 0xfe && (octets[1] & 0xc0) == 0x80 {
                tracing::warn!(addr = %addr, "Rejecting LAN request: link-local IPv6");
                return Err(StatusCode::FORBIDDEN);
            }
            // Allow Unique Local Addresses (ULA): fc00::/7
            let is_ula = (octets[0] & 0xfe) == 0xfc;
            if !is_ula {
                tracing::warn!(addr = %addr, "Rejecting LAN request: IPv6 is not ULA");
                return Err(StatusCode::FORBIDDEN);
            }
        }
    }

    Ok(())
}

/// Currently-discovered LAN peers (snapshot of the background listener).
pub(super) async fn get_lan_peers(AxState(state): AxState<PluginState>) -> Json<Value> {
    Json(serde_json::to_value(state.lan.snapshot()).unwrap_or(json!([])))
}

/// Proxy a peer's `GET /games` (server-side so the UI dodges mixed content).
pub(super) async fn get_lan_peer_games(
    AxState(state): AxState<PluginState>,
    AxPath((addr, port)): AxPath<(String, u16)>,
) -> Result<Json<Value>, StatusCode> {
    if port == 0 {
        return Err(StatusCode::BAD_REQUEST); // discovery-only peer, no file server
    }
    validate_peer(&state, &addr, port)?;
    let url = format!("http://{addr}:{port}/games");
    let resp = state
        .http
        .get(&url)
        .timeout(PEER_PROXY_TIMEOUT)
        .send()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    if !resp.status().is_success() {
        return Err(StatusCode::BAD_GATEWAY);
    }
    let games: Value = resp.json().await.map_err(|_| StatusCode::BAD_GATEWAY)?;
    Ok(Json(games))
}

/// Proxy a peer's cover image so the LAN grid can `<img>`-load it by URL.
pub(super) async fn get_lan_peer_cover(
    AxState(state): AxState<PluginState>,
    AxPath((addr, port, id)): AxPath<(String, u16, String)>,
) -> Result<Response, StatusCode> {
    if port == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    validate_peer(&state, &addr, port)?;
    let url = format!("http://{addr}:{port}/games/{id}/cover");
    let resp = state
        .http
        .get(&url)
        .timeout(PEER_PROXY_TIMEOUT)
        .send()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    if !resp.status().is_success() {
        return Err(StatusCode::NOT_FOUND);
    }
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("image/jpeg")
        .to_string();
    let bytes = resp.bytes().await.map_err(|_| StatusCode::BAD_GATEWAY)?;
    let mut response = Response::new(Body::from(bytes));
    if let Ok(value) = content_type.parse() {
        response.headers_mut().insert(header::CONTENT_TYPE, value);
    }
    Ok(response)
}

#[derive(Deserialize)]
pub(super) struct LanInstallRequest {
    peer_addr: String,
    peer_port: u16,
    game_id: String,
}

/// Start a LAN install. The Decky UI posts here when the user taps a game
/// tile; the heavy work runs in a spawned task. Returns the install_token
/// so the UI can correlate subsequent GET /lan/download polls.
pub(super) async fn post_lan_install(
    AxState(state): AxState<PluginState>,
    Json(body): Json<LanInstallRequest>,
) -> Result<Json<Value>, StatusCode> {
    validate_peer(&state, &body.peer_addr, body.peer_port)?;
    let config = crate::config::Config::load()
        .map(|c| c.data)
        .unwrap_or_default();

    let install_root = {
        let dir = &config.lan.install_dir;
        if dir.is_empty() {
            crate::paths::app_data_dir().join("lan-games")
        } else {
            std::path::PathBuf::from(dir)
        }
    };
    let max_bps = config.lan.download_max_mbps * 1_000_000.0 / 8.0;

    let token = crate::lan::install::begin_install(
        body.peer_addr,
        body.peer_port,
        body.game_id,
        state.http.clone(),
        state.download.clone(),
        // No-op: the Decky UI polls GET /lan/download instead of events.
        Arc::new(|_| {}),
        max_bps,
        install_root,
        state.library.clone(),
        // No library:changed Tauri event in the headless server.
        Arc::new(|_| {}),
        None,
    )
    .await
    .map_err(|e| {
        tracing::warn!(error = %e, "post_lan_install: begin_install failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(json!({ "install_token": token })))
}

/// Current download progress snapshot. Returns `null` when no install is
/// in flight. The Decky UI polls this at ~500 ms while a download is active.
pub(super) async fn get_lan_download(AxState(state): AxState<PluginState>) -> Json<Value> {
    match state.download.snapshot() {
        Some(p) => Json(serde_json::to_value(&p).unwrap_or(Value::Null)),
        None => Json(Value::Null),
    }
}

#[derive(Deserialize)]
pub(super) struct LanCancelRequest {
    install_token: String,
}

/// Cancel an in-flight install by token. Returns `{ cancelled: true }` if
/// the token matched an active install, `{ cancelled: false }` otherwise.
pub(super) async fn delete_lan_download(
    AxState(state): AxState<PluginState>,
    Json(body): Json<LanCancelRequest>,
) -> Json<Value> {
    let cancelled = state.download.request_cancel(&body.install_token);
    Json(json!({ "cancelled": cancelled }))
}
