//! Sentry Mode: an agent-activated focus guard.
//!
//! Hermes (or any caller) starts a session with `POST /sentry/start`. While a
//! session runs, the frontmost app, window title and browser tab are sampled
//! every `interval_seconds`, classified as allowed, blocked or unknown, and
//! continuous blocked time beyond `grace_seconds` raises an escalating
//! violation. When no session is running nothing is monitored.

pub mod core;
mod probe;

use self::core::{
    fallback_roast, rfc3339, sign_webhook_v2, validate_callback_url, validate_context,
    violation_payload, Classifier, Engine, SessionConfig, SessionSummary, Violation,
    DEFAULT_GRACE_SECONDS, DEFAULT_INTERVAL_SECONDS, MAX_GRACE_SECONDS, MAX_INTERVAL_SECONDS,
    MIN_INTERVAL_SECONDS,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::AsyncWriteExt;

const MAX_LIST_ITEMS: usize = 200;
const ANGRY_EXPRESSION: u32 = 4;

#[derive(Debug, Clone, Deserialize)]
pub struct StartReq {
    pub target: String,
    #[serde(default)]
    pub allow: Vec<String>,
    #[serde(default)]
    pub block: Vec<String>,
    #[serde(default)]
    pub grace_seconds: Option<u64>,
    #[serde(default)]
    pub interval_seconds: Option<u64>,
    #[serde(default)]
    pub callback_url: Option<String>,
    /// Optional shared secret. When set, callbacks are signed as a Hermes
    /// generic webhook V2 (`X-Webhook-Timestamp` + `X-Webhook-Signature-V2`).
    #[serde(default)]
    pub callback_secret: Option<String>,
    /// Optional opaque JSON object or string (max 2 KB), echoed unchanged in
    /// status, the stop summary and every violation callback. Agents put the
    /// originating platform and chat here so reactions go back to that chat.
    #[serde(default)]
    pub context: Option<Value>,
}

struct Session {
    id: u64,
    engine: Engine,
    allow: Vec<String>,
    block: Vec<String>,
    callback_url: Option<String>,
    callback_secret: Option<String>,
    last_error: Option<String>,
    last_sample_unix: Option<u64>,
    task: Option<tokio::task::AbortHandle>,
}

/// Tauri managed state holding the (at most one) running session.
#[derive(Default)]
pub struct SentryState {
    inner: Mutex<Option<Session>>,
    next_id: Mutex<u64>,
}

#[derive(Debug, Clone, Serialize)]
struct UiState {
    active: bool,
    target: Option<String>,
    level: u8,
    violations: u32,
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn clean_list(items: Vec<String>) -> Vec<String> {
    items
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .take(MAX_LIST_ITEMS)
        .collect()
}

fn status_json(s: &Session, now: u64) -> Value {
    let e = &s.engine;
    let current = e.last_sample.as_ref().map(|(sample, class)| {
        json!({
            "app": sample.app,
            "title": sample.title,
            "url": sample.url,
            "label": class.label,
            "reason": class.reason,
        })
    });
    json!({
        "ok": true,
        "active": true,
        "target": e.config.target,
        "started_at": rfc3339(e.started_unix()),
        "elapsed_seconds": now.saturating_sub(e.started_unix()),
        "grace_seconds": e.config.grace_seconds,
        "interval_seconds": e.config.interval_seconds,
        "allow": s.allow,
        "block": s.block,
        "callback": s.callback_url.is_some(),
        "focused_seconds": e.focused_seconds,
        "off_task_seconds": e.off_task_seconds,
        "unknown_seconds": e.unknown_seconds,
        "streak_off_task_seconds": e.streak_seconds,
        "violations": e.violation_count,
        "level": e.current_level(),
        "current": current,
        "last_sample_at": s.last_sample_unix.map(rfc3339),
        "last_error": s.last_error,
        "context": e.config.context,
    })
}

fn emit_ui(app: &AppHandle, state: UiState) {
    let _ = app.emit("sentry:state", state);
}

/// Start (or restart) a session. A running session is replaced and its
/// summary is returned as `previous_summary`.
pub fn start(app: &AppHandle, req: StartReq) -> Result<Value, String> {
    let target = req.target.trim().to_string();
    if target.is_empty() {
        return Err("target must not be empty".into());
    }
    if target.chars().count() > 200 {
        return Err("target must be at most 200 characters".into());
    }
    let grace = req.grace_seconds.unwrap_or(DEFAULT_GRACE_SECONDS);
    if grace > MAX_GRACE_SECONDS {
        return Err(format!("grace_seconds must be at most {MAX_GRACE_SECONDS}"));
    }
    let interval = req.interval_seconds.unwrap_or(DEFAULT_INTERVAL_SECONDS);
    if !(MIN_INTERVAL_SECONDS..=MAX_INTERVAL_SECONDS).contains(&interval) {
        return Err(format!(
            "interval_seconds must be between {MIN_INTERVAL_SECONDS} and {MAX_INTERVAL_SECONDS}"
        ));
    }
    let callback_url = req
        .callback_url
        .map(|u| u.trim().to_string())
        .filter(|u| !u.is_empty());
    if let Some(url) = &callback_url {
        validate_callback_url(url)?;
    }
    let callback_secret = req.callback_secret.filter(|s| !s.is_empty());
    let context = validate_context(req.context)?;

    let allow = clean_list(req.allow);
    let block = clean_list(req.block);
    let now = now_unix();
    let engine = Engine::new(
        SessionConfig {
            target: target.clone(),
            grace_seconds: grace,
            interval_seconds: interval,
            context,
        },
        Classifier::new(&allow, &block),
        now,
    );

    let state = app.state::<SentryState>();
    let id = {
        let mut next = state.next_id.lock().unwrap();
        *next += 1;
        *next
    };
    let session = Session {
        id,
        engine,
        allow,
        block,
        callback_url,
        callback_secret,
        last_error: None,
        last_sample_unix: None,
        task: None,
    };

    let (previous, status) = {
        let mut guard = state.inner.lock().unwrap();
        let previous = guard.take().map(|old| {
            if let Some(task) = &old.task {
                task.abort();
            }
            old.engine.summary(now)
        });
        let status = status_json(&session, now);
        *guard = Some(session);
        // Spawn only after the session is stored, so the loop always finds it.
        let task = tokio::spawn(run_loop(app.clone(), id, interval)).abort_handle();
        if let Some(s) = guard.as_mut() {
            s.task = Some(task);
        }
        (previous, status)
    };

    emit_ui(
        app,
        UiState {
            active: true,
            target: Some(target.clone()),
            level: 0,
            violations: 0,
        },
    );
    println!("Sentry Mode started: {target}");

    Ok(json!({
        "ok": true,
        "message": "sentry started",
        "status": status,
        "previous_summary": previous,
    }))
}

/// Stop the session and return its summary. Idempotent.
pub fn stop(app: &AppHandle) -> Value {
    let state = app.state::<SentryState>();
    let session = state.inner.lock().unwrap().take();
    match session {
        Some(s) => {
            if let Some(task) = &s.task {
                task.abort();
            }
            let summary: SessionSummary = s.engine.summary(now_unix());
            emit_ui(
                app,
                UiState {
                    active: false,
                    target: None,
                    level: 0,
                    violations: 0,
                },
            );
            println!("Sentry Mode stopped: {}", summary.target);
            json!({
                "ok": true,
                "active": false,
                "message": "sentry stopped",
                "summary": summary,
            })
        }
        None => json!({
            "ok": true,
            "active": false,
            "message": "sentry was not running",
            "summary": null,
        }),
    }
}

pub fn status(app: &AppHandle) -> Value {
    let state = app.state::<SentryState>();
    let guard = state.inner.lock().unwrap();
    match guard.as_ref() {
        Some(s) => status_json(s, now_unix()),
        None => json!({ "ok": true, "active": false }),
    }
}

struct ViolationJob {
    violation: Violation,
    target: String,
    violations: u32,
    session_started: u64,
    callback_url: Option<String>,
    callback_secret: Option<String>,
    context: Option<Value>,
}

async fn run_loop(app: AppHandle, id: u64, interval: u64) {
    let mut ticker = tokio::time::interval(Duration::from_secs(interval));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        ticker.tick().await;
        let probe = probe::sample().await;
        let now = now_unix();

        let job = {
            let state = app.state::<SentryState>();
            let mut guard = state.inner.lock().unwrap();
            let Some(session) = guard.as_mut().filter(|s| s.id == id) else {
                return; // stopped or replaced
            };
            session.last_error = probe.error;
            session.last_sample_unix = Some(now);
            let (_, violation) = session.engine.ingest(probe.sample, now);
            violation.map(|v| ViolationJob {
                violation: v,
                target: session.engine.config.target.clone(),
                violations: session.engine.violation_count,
                session_started: session.engine.started_unix(),
                callback_url: session.callback_url.clone(),
                callback_secret: session.callback_secret.clone(),
                context: session.engine.config.context.clone(),
            })
        };

        if let Some(job) = job {
            // Do not block sampling on a slow callback.
            tokio::spawn(handle_violation(app.clone(), id, job));
        }
    }
}

async fn handle_violation(app: AppHandle, id: u64, job: ViolationJob) {
    let v = &job.violation;
    println!(
        "Sentry violation level {} ({}s off task): {} {}",
        v.level,
        v.off_task_seconds,
        v.app,
        v.url.as_deref().unwrap_or("")
    );

    emit_ui(
        &app,
        UiState {
            active: true,
            target: Some(job.target.clone()),
            level: v.level,
            violations: job.violations,
        },
    );
    let _ = app.emit("sentry:violation", v);
    let _ = app.emit("api:expression", json!({ "id": ANGRY_EXPRESSION }));

    let delivered = match &job.callback_url {
        Some(url) => {
            let payload = violation_payload(
                v,
                &job.target,
                job.violations,
                job.session_started,
                job.context.as_ref(),
            );
            let request_id = format!("sparky-sentry-{}-{}", job.session_started, job.violations);
            match post_callback(url, job.callback_secret.as_deref(), &payload, &request_id).await {
                Ok(()) => true,
                Err(e) => {
                    eprintln!("Sentry callback failed: {e}");
                    record_error(&app, id, format!("callback failed: {e}"));
                    false
                }
            }
        }
        None => false,
    };

    if !delivered {
        // No agent to roast for us: scold locally with a bubble and angry face.
        let _ = app.emit(
            "api:speak",
            json!({
                "text": fallback_roast(v.level, &job.target),
                "audio_url": null,
                "expression": ANGRY_EXPRESSION,
            }),
        );
    }
}

fn record_error(app: &AppHandle, id: u64, message: String) {
    let state = app.state::<SentryState>();
    let mut guard = state.inner.lock().unwrap();
    if let Some(s) = guard.as_mut().filter(|s| s.id == id) {
        s.last_error = Some(message);
    }
}

/// POST the violation with the system `curl` (always present on macOS), so
/// the app needs no HTTP client or TLS stack of its own.
async fn post_callback(
    url: &str,
    secret: Option<&str>,
    payload: &Value,
    request_id: &str,
) -> Result<(), String> {
    let body = payload.to_string();
    let mut args: Vec<String> = vec![
        "-sS".into(),
        "--fail".into(),
        "--max-time".into(),
        "10".into(),
        "-X".into(),
        "POST".into(),
        "-H".into(),
        "Content-Type: application/json".into(),
        "-H".into(),
        format!("X-Request-ID: {request_id}"),
    ];
    if let Some(secret) = secret {
        let ts = now_unix();
        args.push("-H".into());
        args.push(format!("X-Webhook-Timestamp: {ts}"));
        args.push("-H".into());
        args.push(format!(
            "X-Webhook-Signature-V2: {}",
            sign_webhook_v2(secret, ts, &body)
        ));
    }
    args.extend([
        "--data-binary".into(),
        "@-".into(),
        "--url".into(),
        url.to_string(),
    ]);

    let mut child = tokio::process::Command::new("curl")
        .args(&args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("failed to run curl: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(body.as_bytes())
            .await
            .map_err(|e| format!("failed to send body: {e}"))?;
    }
    let out = tokio::time::timeout(Duration::from_secs(15), child.wait_with_output())
        .await
        .map_err(|_| "curl timed out".to_string())?
        .map_err(|e| format!("curl failed: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        Err(err
            .lines()
            .next()
            .unwrap_or("curl error")
            .trim()
            .to_string())
    }
}
