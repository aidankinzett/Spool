//! Cloud-setup pairing: copy this device's rclone cloud remote to another Spool
//! device on the LAN, so the remote is only set up once.
//!
//! The host (a device with a remote configured) opens a short **pairing window**
//! from Settings → Cloud saves: a random 6-digit code, valid for two minutes and
//! three attempts. The joiner (a fresh install, in onboarding) picks the host from
//! the discovered peers and types the code. The transport is plain HTTP, so the
//! code is never sent. Both sides run a SPAKE2 exchange keyed on it, derive
//! per-direction keys from the result, and the host sends the remote's connection
//! details as a ChaCha20-Poly1305 blob that only decrypts under the same key.
//! A wrong code therefore yields a failed proof, not a payload, and each failed
//! attempt counts against the window.
//!
//! Only OAuth providers (Google Drive, OneDrive, Dropbox, Box) and WebDAV are
//! transferred. The payload carries the remote's rclone parameters (for OAuth, the
//! token and client settings; for WebDAV, the URL, user and revealed password) and
//! the Spool settings that point at it; the receiver rebuilds the remote with the
//! same `rclone config create` / `ludusavi cloud set webdav` calls the Settings
//! forms use.

use crate::config::{ConfigData, SharedConfig};
use crate::error::{AppError, AppResult};
use crate::lan::LanState;
use axum::http::StatusCode;
use base64::Engine as _;
use chacha20poly1305::aead::{Aead, AeadCore, KeyInit, OsRng};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use serde::{Deserialize, Serialize};
use spake2::{Ed25519Group, Identity, Password, Spake2};
use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};

/// How long a code stays valid after the host opens the window.
const CODE_TTL: Duration = Duration::from_secs(120);
/// Handshakes allowed per window. A joiner that mistypes the code uses one.
const MAX_ATTEMPTS: u8 = 3;

const ID_HOST: &[u8] = b"spool-pair-host";
const ID_JOINER: &[u8] = b"spool-pair-joiner";
/// Plaintext the joiner encrypts to show it derived the same key.
const PROOF: &[u8] = b"spool-pair-ok";
const KEY_CTX_JOINER: &str = "spool cloud pairing v1 joiner->host";
const KEY_CTX_HOST: &str = "spool cloud pairing v1 host->joiner";

/// rclone parameters copied for an OAuth remote. Anything else in the payload is
/// ignored on import, so a payload can't set arbitrary backend options.
const OAUTH_PARAMS: &[&str] = &[
    "token",
    "client_id",
    "client_secret",
    "scope",
    "drive_id",
    "drive_type",
];

// ── State ───────────────────────────────────────────────────────────────────

struct Pending {
    session: String,
    joiner_key: [u8; 32],
    host_key: [u8; 32],
}

struct Window {
    code: String,
    expires: Instant,
    attempts: u8,
    pending: Option<Pending>,
}

/// The host's pairing window. At most one is open at a time.
#[derive(Default)]
pub struct CloudPairing {
    window: Mutex<Option<Window>>,
}

impl CloudPairing {
    /// True while a window is open and unexpired. Drives the `pairing_port`
    /// advertised in the discovery announce.
    pub fn is_open(&self) -> bool {
        let Ok(mut g) = self.window.lock() else {
            return false;
        };
        if g.as_ref().is_some_and(|w| w.expires <= Instant::now()) {
            *g = None;
        }
        g.is_some()
    }

    fn open(&self) -> AppResult<String> {
        let code = format!("{:06}", rand_below(1_000_000));
        let mut g = self.window.lock().map_err(|_| AppError::LockPoisoned)?;
        *g = Some(Window {
            code: code.clone(),
            expires: Instant::now() + CODE_TTL,
            attempts: 0,
            pending: None,
        });
        Ok(code)
    }

    fn close(&self) {
        if let Ok(mut g) = self.window.lock() {
            *g = None;
        }
    }
}

fn rand_below(n: u32) -> u32 {
    use chacha20poly1305::aead::rand_core::RngCore;
    OsRng.next_u32() % n
}

// ── Wire types ──────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize)]
pub(super) struct StartReq {
    /// Joiner's SPAKE2 message, base64.
    msg: String,
}

#[derive(Serialize, Deserialize)]
pub(super) struct StartResp {
    session: String,
    /// Host's SPAKE2 message, base64.
    msg: String,
}

#[derive(Serialize, Deserialize)]
pub(super) struct FinishReq {
    session: String,
    /// `PROOF` sealed under the joiner→host key, base64.
    proof: String,
}

#[derive(Serialize, Deserialize)]
pub(super) struct FinishResp {
    /// The [`CloudSetup`] JSON sealed under the host→joiner key, base64.
    payload: String,
}

/// Why a pairing request was refused. Mapped to an HTTP status by the server.
#[derive(Debug)]
pub(super) enum PairError {
    /// No open window (or it expired / was consumed).
    Closed,
    /// Out of attempts.
    Locked,
    /// Malformed request, or the proof didn't verify.
    Rejected,
    Internal,
}

impl PairError {
    pub(super) fn status(&self) -> StatusCode {
        match self {
            PairError::Closed => StatusCode::FORBIDDEN,
            PairError::Locked => StatusCode::TOO_MANY_REQUESTS,
            PairError::Rejected => StatusCode::FORBIDDEN,
            PairError::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

/// What the joiner receives, sealed.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CloudSetup {
    /// Spool provider id: `google-drive`, `onedrive`, `dropbox`, `box`, `webdav`.
    provider: String,
    base_path: String,
    /// OAuth remotes: the allow-listed rclone parameters.
    #[serde(default)]
    params: BTreeMap<String, String>,
    /// WebDAV remotes.
    #[serde(default)]
    webdav: Option<WebdavSetup>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WebdavSetup {
    url: String,
    username: String,
    password: String,
    vendor: String,
}

// ── Crypto helpers ──────────────────────────────────────────────────────────

fn b64() -> base64::engine::GeneralPurpose {
    base64::engine::general_purpose::STANDARD
}

/// Splits the SPAKE2 output into one key per direction.
fn derive_keys(shared: &[u8]) -> ([u8; 32], [u8; 32]) {
    (
        blake3::derive_key(KEY_CTX_JOINER, shared),
        blake3::derive_key(KEY_CTX_HOST, shared),
    )
}

/// `nonce || ciphertext`.
fn seal(key: &[u8; 32], plaintext: &[u8]) -> AppResult<Vec<u8>> {
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
    let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);
    let mut out = nonce.to_vec();
    out.extend(
        cipher
            .encrypt(&nonce, plaintext)
            .map_err(|_| AppError::Other("encryption failed".into()))?,
    );
    Ok(out)
}

fn open(key: &[u8; 32], blob: &[u8]) -> Option<Vec<u8>> {
    if blob.len() < 12 {
        return None;
    }
    let (nonce, ct) = blob.split_at(12);
    ChaCha20Poly1305::new(Key::from_slice(key))
        .decrypt(Nonce::from_slice(nonce), ct)
        .ok()
}

// ── Host side ───────────────────────────────────────────────────────────────

/// `POST /pair/start`. Runs the host half of SPAKE2 against the open window's
/// code and stores the derived keys. Counts as one attempt.
pub(super) fn handle_start(app: &AppHandle, req: StartReq) -> Result<StartResp, PairError> {
    let msg_a = b64().decode(&req.msg).map_err(|_| PairError::Rejected)?;
    let pairing = app.state::<CloudPairing>();
    let mut g = pairing.window.lock().map_err(|_| PairError::Internal)?;
    let Some(w) = g.as_mut().filter(|w| w.expires > Instant::now()) else {
        *g = None;
        return Err(PairError::Closed);
    };
    if w.attempts >= MAX_ATTEMPTS {
        return Err(PairError::Locked);
    }
    w.attempts += 1;

    let (spake, msg_b) = Spake2::<Ed25519Group>::start_b(
        &Password::new(w.code.as_bytes()),
        &Identity::new(ID_JOINER),
        &Identity::new(ID_HOST),
    );
    let shared = spake.finish(&msg_a).map_err(|_| PairError::Rejected)?;
    let (joiner_key, host_key) = derive_keys(&shared);
    let session = uuid::Uuid::new_v4().to_string();
    w.pending = Some(Pending {
        session: session.clone(),
        joiner_key,
        host_key,
    });
    Ok(StartResp {
        session,
        msg: b64().encode(msg_b),
    })
}

/// `POST /pair/finish`. Verifies the joiner's proof; on success closes the window
/// (a code is single-use) and returns the sealed cloud setup.
pub(super) async fn handle_finish(
    app: &AppHandle,
    req: FinishReq,
) -> Result<FinishResp, PairError> {
    let proof = b64().decode(&req.proof).map_err(|_| PairError::Rejected)?;
    let host_key = {
        let pairing = app.state::<CloudPairing>();
        let mut g = pairing.window.lock().map_err(|_| PairError::Internal)?;
        let Some(w) = g.as_mut().filter(|w| w.expires > Instant::now()) else {
            *g = None;
            return Err(PairError::Closed);
        };
        // Only consume the pending handshake when the session matches, so a stray
        // request with a wrong session can't discard the real joiner's.
        if !w.pending.as_ref().is_some_and(|p| p.session == req.session) {
            return Err(PairError::Rejected);
        }
        let Some(p) = w.pending.take() else {
            return Err(PairError::Rejected);
        };
        if open(&p.joiner_key, &proof).as_deref() != Some(PROOF) {
            if w.attempts >= MAX_ATTEMPTS {
                *g = None;
                let _ = app.emit("pairing:ended", "locked");
            }
            return Err(PairError::Rejected);
        }
        *g = None;
        p.host_key
    };

    let cfg = {
        let config = app.state::<SharedConfig>();
        let g = config.lock().map_err(|_| PairError::Internal)?;
        g.data.clone()
    };
    // The proof verified and the window is already closed, so any failure from
    // here on ends the pairing; tell the host UI so it stops showing the code.
    let sealed = async {
        let setup = export_setup(&cfg).await.map_err(|e| {
            tracing::warn!(error = %e, "cloud pairing: could not export the remote");
            PairError::Internal
        })?;
        let json = serde_json::to_vec(&setup).map_err(|_| PairError::Internal)?;
        seal(&host_key, &json).map_err(|_| PairError::Internal)
    }
    .await;
    match sealed {
        Ok(blob) => {
            let _ = app.emit("pairing:ended", "paired");
            Ok(FinishResp {
                payload: b64().encode(blob),
            })
        }
        Err(e) => {
            let _ = app.emit("pairing:ended", "failed");
            Err(e)
        }
    }
}

/// Reads the configured remote out of rclone's config into a [`CloudSetup`].
async fn export_setup(cfg: &ConfigData) -> AppResult<CloudSetup> {
    let provider = cfg.cloud.provider.clone();
    let dump = rclone_dump().await?;
    let base_path = crate::rclone::base_path(cfg);

    if let Some((_, name)) = crate::rclone::oauth_remote(&provider) {
        let remote = dump
            .get(name)
            .and_then(|v| v.as_object())
            .ok_or_else(|| AppError::Other(format!("rclone remote '{name}' not found")))?;
        let params: BTreeMap<String, String> = OAUTH_PARAMS
            .iter()
            .filter_map(|k| {
                let v = remote.get(*k)?.as_str()?;
                Some(((*k).to_string(), v.to_string()))
            })
            .collect();
        if !params.contains_key("token") {
            return Err(AppError::Other(format!("remote '{name}' has no token")));
        }
        return Ok(CloudSetup {
            provider,
            base_path,
            params,
            webdav: None,
        });
    }

    if provider == "webdav" {
        let yaml: serde_yaml::Value =
            std::fs::read_to_string(crate::paths::ludusavi_config_file())
                .ok()
                .and_then(|s| serde_yaml::from_str(&s).ok())
                .ok_or_else(|| AppError::Other("ludusavi config unreadable".into()))?;
        let name = crate::rclone::remote_name_from_yaml(&yaml)
            .ok_or_else(|| AppError::Other("no WebDAV remote configured".into()))?;
        let remote = dump
            .get(&name)
            .and_then(|v| v.as_object())
            .ok_or_else(|| AppError::Other(format!("rclone remote '{name}' not found")))?;
        let field = |k: &str| {
            remote
                .get(k)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string()
        };
        let password = rclone_reveal(&field("pass")).await?;
        return Ok(CloudSetup {
            provider,
            base_path,
            params: BTreeMap::new(),
            webdav: Some(WebdavSetup {
                url: field("url"),
                username: field("user"),
                password,
                vendor: field("vendor"),
            }),
        });
    }

    Err(AppError::Other(
        "only OAuth and WebDAV cloud setups can be shared".into(),
    ))
}

async fn run_rclone(args: &[&str], what: &str) -> AppResult<String> {
    let exe = crate::paths::resolve_rclone_path()
        .ok_or_else(|| AppError::Other("rclone binary not found".into()))?;
    let mut cmd = tokio::process::Command::new(exe);
    crate::capture_stdio!(cmd);
    cmd.kill_on_drop(true);
    cmd.args(args);
    let out = tokio::time::timeout(Duration::from_secs(15), cmd.output())
        .await
        .map_err(|_| AppError::Other(format!("rclone {what} timed out")))?
        .map_err(|e| AppError::Other(format!("rclone {what}: {e}")))?;
    if !out.status.success() {
        return Err(AppError::Other(format!(
            "rclone {what} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

async fn rclone_dump() -> AppResult<serde_json::Value> {
    let out = run_rclone(&["config", "dump"], "config dump").await?;
    serde_json::from_str(&out)
        .map_err(|e| AppError::Other(format!("failed to parse rclone config dump: {e}")))
}

async fn rclone_reveal(obscured: &str) -> AppResult<String> {
    if obscured.is_empty() {
        return Ok(String::new());
    }
    let out = run_rclone(&["reveal", obscured], "reveal").await?;
    Ok(out.trim_end_matches(['\r', '\n']).to_string())
}

// ── Joiner side ─────────────────────────────────────────────────────────────

fn http_err(what: &str, status: reqwest::StatusCode) -> AppError {
    AppError::Other(match status.as_u16() {
        403 => format!("{what}: wrong code, or the other device isn't sharing right now"),
        429 => format!("{what}: too many attempts — start sharing again on the other device"),
        s => format!("{what}: HTTP {s}"),
    })
}

/// Runs the joiner half against `host:port` and returns the decrypted setup.
async fn fetch_setup(host: &str, port: u16, code: &str) -> AppResult<CloudSetup> {
    let host = if host.contains(':') {
        format!("[{host}]")
    } else {
        host.to_string()
    };
    let base = format!("http://{host}:{port}");
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| AppError::Other(format!("http client: {e}")))?;

    let (spake, msg_a) = Spake2::<Ed25519Group>::start_a(
        &Password::new(code.as_bytes()),
        &Identity::new(ID_JOINER),
        &Identity::new(ID_HOST),
    );
    let resp = http
        .post(format!("{base}/pair/start"))
        .json(&StartReq {
            msg: b64().encode(msg_a),
        })
        .send()
        .await
        .map_err(|e| AppError::Other(format!("could not reach the other device: {e}")))?;
    if !resp.status().is_success() {
        return Err(http_err("pairing", resp.status()));
    }
    let start: StartResp = resp
        .json()
        .await
        .map_err(|e| AppError::Other(format!("bad pairing reply: {e}")))?;

    let msg_b = b64()
        .decode(&start.msg)
        .map_err(|_| AppError::Other("bad pairing reply".into()))?;
    let shared = spake
        .finish(&msg_b)
        .map_err(|_| AppError::Other("pairing failed".into()))?;
    let (joiner_key, host_key) = derive_keys(&shared);

    let resp = http
        .post(format!("{base}/pair/finish"))
        .json(&FinishReq {
            session: start.session,
            proof: b64().encode(seal(&joiner_key, PROOF)?),
        })
        .send()
        .await
        .map_err(|e| AppError::Other(format!("could not reach the other device: {e}")))?;
    if !resp.status().is_success() {
        return Err(http_err("pairing", resp.status()));
    }
    let finish: FinishResp = resp
        .json()
        .await
        .map_err(|e| AppError::Other(format!("bad pairing reply: {e}")))?;
    let blob = b64()
        .decode(&finish.payload)
        .map_err(|_| AppError::Other("bad pairing reply".into()))?;
    let json = open(&host_key, &blob)
        .ok_or_else(|| AppError::Other("pairing reply failed verification".into()))?;
    serde_json::from_slice(&json)
        .map_err(|e| AppError::Other(format!("bad cloud setup payload: {e}")))
}

/// Recreates the remote locally and points Spool at it.
async fn apply_setup(app: &AppHandle, setup: &CloudSetup) -> AppResult<()> {
    let base = setup.base_path.trim().trim_matches('/');
    if base.is_empty() || base.split('/').any(|p| p == "..") {
        return Err(AppError::Other("received an invalid base folder".into()));
    }

    if let Some((rclone_type, remote_name)) = crate::rclone::oauth_remote(&setup.provider) {
        if !setup.params.contains_key("token") {
            return Err(AppError::Other("received no token".into()));
        }
        let mut args: Vec<String> = vec![
            "config".into(),
            "create".into(),
            remote_name.into(),
            rclone_type.into(),
        ];
        for key in OAUTH_PARAMS {
            if let Some(v) = setup.params.get(*key) {
                args.push(format!("{key}={v}"));
            }
        }
        // As in `connect_cloud_oauth`: store the params, don't re-run the browser flow.
        args.push("--non-interactive".into());
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        run_rclone(&refs, "config create").await?;
    } else if setup.provider == "webdav" {
        let w = setup
            .webdav
            .as_ref()
            .ok_or_else(|| AppError::Other("received no WebDAV details".into()))?;
        let scheme_ok = url::Url::parse(&w.url)
            .map(|u| matches!(u.scheme(), "http" | "https"))
            .unwrap_or(false);
        if !scheme_ok {
            return Err(AppError::Other("received an invalid WebDAV URL".into()));
        }
        crate::ludusavi::apply_webdav_remote(&w.url, &w.username, &w.password, &w.vendor, false)
            .await?;
    } else {
        return Err(AppError::Other(
            "the other device shared an unsupported cloud setup".into(),
        ));
    }

    let (rclone_args, remote_path) = {
        let config = app.state::<SharedConfig>();
        let mut cfg = config.lock().map_err(|_| AppError::LockPoisoned)?;
        cfg.data.cloud.provider = setup.provider.clone();
        cfg.data.cloud.base_path = base.to_string();
        if let Some(w) = &setup.webdav {
            cfg.data.cloud.webdav_url = w.url.clone();
            cfg.data.cloud.webdav_username = w.username.clone();
        }
        let remote_path = format!("{}/ludusavi-backup", crate::rclone::base_path(&cfg.data));
        let rclone_args = cfg.data.cloud.rclone_args.clone();
        cfg.save()?;
        (rclone_args, remote_path)
    };
    crate::ludusavi_config::set_cloud(
        Some(&setup.provider),
        Some(""),
        Some(&remote_path),
        None,
        Some(&rclone_args),
    )?;
    crate::rclone::poll_once(app).await;
    Ok(())
}

// ── Commands ────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct PairingInfo {
    pub code: String,
    pub expires_in_secs: u64,
}

/// Opens a pairing window and returns the code to show. Fails when there's no
/// shareable cloud setup on this device.
#[tauri::command]
pub async fn start_cloud_pairing(
    config: State<'_, SharedConfig>,
    pairing: State<'_, CloudPairing>,
) -> AppResult<PairingInfo> {
    let provider = {
        let g = config.lock().map_err(|_| AppError::LockPoisoned)?;
        g.data.cloud.provider.clone()
    };
    if crate::rclone::oauth_remote(&provider).is_none() && provider != "webdav" {
        return Err(AppError::Other(
            "set up Google Drive, OneDrive, Dropbox, Box or WebDAV first — other cloud setups can't be shared".into(),
        ));
    }
    let code = pairing.open()?;
    Ok(PairingInfo {
        code,
        expires_in_secs: CODE_TTL.as_secs(),
    })
}

#[tauri::command]
pub fn cancel_cloud_pairing(pairing: State<'_, CloudPairing>) {
    pairing.close();
}

/// Pairs with a discovered peer using the code shown on its screen and applies
/// the cloud setup it sends. Returns the provider id that was configured.
#[tauri::command]
pub async fn join_cloud_pairing(
    app: AppHandle,
    lan: State<'_, LanState>,
    peer_device_id: String,
    code: String,
) -> AppResult<String> {
    let (addr, port) = lan
        .snapshot()
        .into_iter()
        .find(|p| p.device_id == peer_device_id)
        .map(|p| (p.addr, p.pairing_port))
        .ok_or_else(|| AppError::Other("that device is no longer visible".into()))?;
    if port == 0 {
        return Err(AppError::Other(
            "that device isn't sharing its cloud setup right now".into(),
        ));
    }
    let code: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    if code.len() != 6 || !code.chars().all(|c| c.is_ascii_digit()) {
        return Err(AppError::Other("the code is 6 digits".into()));
    }
    let setup = fetch_setup(&addr, port, &code).await?;
    apply_setup(&app, &setup).await?;
    Ok(setup.provider)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_round_trips_and_rejects_wrong_key() {
        let (k1, k2) = derive_keys(b"shared secret");
        let blob = seal(&k1, b"hello").unwrap();
        assert_eq!(open(&k1, &blob).as_deref(), Some(&b"hello"[..]));
        assert!(open(&k2, &blob).is_none());
        assert!(open(&k1, &blob[..5]).is_none());
    }

    #[test]
    fn spake_agrees_on_the_same_code_and_not_on_different_ones() {
        let run = |a_code: &str, b_code: &str| {
            let (a, msg_a) = Spake2::<Ed25519Group>::start_a(
                &Password::new(a_code.as_bytes()),
                &Identity::new(ID_JOINER),
                &Identity::new(ID_HOST),
            );
            let (b, msg_b) = Spake2::<Ed25519Group>::start_b(
                &Password::new(b_code.as_bytes()),
                &Identity::new(ID_JOINER),
                &Identity::new(ID_HOST),
            );
            (a.finish(&msg_b).unwrap(), b.finish(&msg_a).unwrap())
        };
        let (ka, kb) = run("123456", "123456");
        assert_eq!(ka, kb);
        let (ka, kb) = run("123456", "654321");
        assert_ne!(ka, kb);
        // The proof the joiner sends only verifies under the matching key.
        let (joiner_key, _) = derive_keys(&ka);
        let (host_key_for_wrong_code, _) = derive_keys(&kb);
        assert!(open(&host_key_for_wrong_code, &seal(&joiner_key, PROOF).unwrap()).is_none());
    }

    #[test]
    fn window_expires_and_closes() {
        let p = CloudPairing::default();
        assert!(!p.is_open());
        let code = p.open().unwrap();
        assert_eq!(code.len(), 6);
        assert!(p.is_open());
        p.close();
        assert!(!p.is_open());
        p.open().unwrap();
        p.window.lock().unwrap().as_mut().unwrap().expires = Instant::now();
        assert!(!p.is_open());
    }

    #[test]
    fn setup_payload_round_trips() {
        let s = CloudSetup {
            provider: "webdav".into(),
            base_path: "Spool".into(),
            params: BTreeMap::new(),
            webdav: Some(WebdavSetup {
                url: "https://dav.example/x".into(),
                username: "u".into(),
                password: "p".into(),
                vendor: "other".into(),
            }),
        };
        let back: CloudSetup = serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap();
        assert_eq!(back.webdav.unwrap().password, "p");
    }
}
