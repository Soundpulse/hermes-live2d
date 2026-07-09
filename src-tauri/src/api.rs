use crate::config::AtriConfig;
use axum::{
    Router,
    body::Body,
    extract::{Path as AxumPath, Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tauri::{AppHandle, Emitter};
use tower_http::cors::CorsLayer;

// ── Request types ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpressionReq {
    #[serde(default)]
    pub id: Option<u32>,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MotionReq {
    pub group: String,
    #[serde(default)]
    pub index: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeakReq {
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub audio_url: Option<String>,
    #[serde(default)]
    pub audio_data: Option<String>,
    #[serde(default)]
    pub audio_format: Option<String>,
    #[serde(default)]
    pub expression: Option<u32>,
}

/// Payload emitted to the frontend — audio always arrives as a URL/path,
/// never as inline base64.
#[derive(Debug, Clone, Serialize)]
pub struct SpeakEvent {
    pub text: Option<String>,
    pub audio_url: Option<String>,
    pub expression: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BubbleReq {
    pub text: String,
    #[serde(default)]
    pub duration: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LipsyncReq {
    pub audio_url: String,
}

// ── Response type ────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct ApiResponse {
    pub ok: bool,
    pub message: String,
}

impl ApiResponse {
    fn success(msg: impl Into<String>) -> Json<Self> {
        Json(Self {
            ok: true,
            message: msg.into(),
        })
    }

    fn error(msg: impl Into<String>) -> (StatusCode, Json<Self>) {
        (
            StatusCode::BAD_REQUEST,
            Json(Self {
                ok: false,
                message: msg.into(),
            }),
        )
    }
}

// ── Expression list ──────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct ExpressionInfo {
    pub id: u32,
    pub name: String,
}

fn expression_list() -> Vec<ExpressionInfo> {
    let entries: Vec<(u32, &str)> = vec![
        (1, "exp1"),
        (2, "exp2"),
        (3, "exp3"),
        (4, "exp4"),
        (5, "exp5"),
        (6, "exp6"),
        (7, "exp7"),
        (8, "exp8"),
        (9, "exp9"),
        (10, "exp10"),
        (11, "exp11"),
        (12, "exp12"),
    ];
    entries
        .into_iter()
        .map(|(id, name)| ExpressionInfo {
            id,
            name: name.to_string(),
        })
        .collect()
}

// ── Handlers ─────────────────────────────────────────────────────

async fn status_handler() -> Json<ApiResponse> {
    ApiResponse::success("ATRI Live2D API is running")
}

async fn expressions_handler() -> Json<Vec<ExpressionInfo>> {
    Json(expression_list())
}

async fn expression_handler(
    State(app): State<AppHandle>,
    Json(payload): Json<ExpressionReq>,
) -> Result<Json<ApiResponse>, (StatusCode, Json<ApiResponse>)> {
    if payload.id.is_none() && payload.name.is_none() {
        return Err(ApiResponse::error("must provide id or name"));
    }
    app.emit("api:expression", &payload)
        .map_err(|e| ApiResponse::error(format!("emit failed: {e}")))?;
    Ok(ApiResponse::success("expression event emitted"))
}

async fn motion_handler(
    State(app): State<AppHandle>,
    Json(payload): Json<MotionReq>,
) -> Result<Json<ApiResponse>, (StatusCode, Json<ApiResponse>)> {
    app.emit("api:motion", &payload)
        .map_err(|e| ApiResponse::error(format!("emit failed: {e}")))?;
    Ok(ApiResponse::success("motion event emitted"))
}

/// Decode base64 audio and write it to `<dir>/speak.<ext>`, returning the path.
/// A single well-known filename is overwritten on every call, so uploaded
/// audio is discarded automatically by the next request.
fn write_speak_audio(
    dir: &std::path::Path,
    audio_data: &str,
    audio_format: Option<&str>,
) -> Result<std::path::PathBuf, String> {
    use base64::Engine;

    let ext = match audio_format.unwrap_or("mp3") {
        f @ ("mp3" | "wav" | "ogg" | "flac") => f,
        other => return Err(format!("unsupported audio_format: {other}")),
    };

    // Tolerate line-wrapped base64 (e.g. from `base64` without -w0)
    let cleaned: String = audio_data.chars().filter(|c| !c.is_whitespace()).collect();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(cleaned)
        .map_err(|e| format!("invalid base64 audio_data: {e}"))?;

    std::fs::create_dir_all(dir).map_err(|e| format!("failed to create {}: {e}", dir.display()))?;
    let path = dir.join(format!("speak.{ext}"));
    std::fs::write(&path, bytes).map_err(|e| format!("failed to write audio: {e}"))?;
    Ok(path)
}

async fn speak_handler(
    State(app): State<AppHandle>,
    Json(payload): Json<SpeakReq>,
) -> Result<Json<ApiResponse>, (StatusCode, Json<ApiResponse>)> {
    // Inline audio takes precedence over audio_url
    let audio_url = match &payload.audio_data {
        Some(data) => {
            let dir = crate::config::atri_dir().join("tmp");
            let path = write_speak_audio(&dir, data, payload.audio_format.as_deref())
                .map_err(ApiResponse::error)?;
            Some(path.to_string_lossy().into_owned())
        }
        None => payload.audio_url.clone(),
    };

    let event = SpeakEvent {
        text: payload.text.clone(),
        audio_url,
        expression: payload.expression,
    };
    app.emit("api:speak", &event)
        .map_err(|e| ApiResponse::error(format!("emit failed: {e}")))?;
    Ok(ApiResponse::success("speak event emitted"))
}

async fn bubble_handler(
    State(app): State<AppHandle>,
    Json(payload): Json<BubbleReq>,
) -> Result<Json<ApiResponse>, (StatusCode, Json<ApiResponse>)> {
    app.emit("api:bubble", &payload)
        .map_err(|e| ApiResponse::error(format!("emit failed: {e}")))?;
    Ok(ApiResponse::success("bubble event emitted"))
}

async fn lipsync_start_handler(
    State(app): State<AppHandle>,
    Json(payload): Json<LipsyncReq>,
) -> Result<Json<ApiResponse>, (StatusCode, Json<ApiResponse>)> {
    app.emit("api:lipsync:start", &payload)
        .map_err(|e| ApiResponse::error(format!("emit failed: {e}")))?;
    Ok(ApiResponse::success("lipsync start event emitted"))
}

async fn lipsync_stop_handler(
    State(app): State<AppHandle>,
) -> Result<Json<ApiResponse>, (StatusCode, Json<ApiResponse>)> {
    app.emit("api:lipsync:stop", ())
        .map_err(|e| ApiResponse::error(format!("emit failed: {e}")))?;
    Ok(ApiResponse::success("lipsync stop event emitted"))
}

// ── Audio file serving ──────────────────────────────────────────

#[derive(Deserialize)]
struct AudioQuery {
    path: String,
}

async fn audio_handler(Query(q): Query<AudioQuery>) -> Response {
    let path = std::path::Path::new(&q.path);
    if !path.exists() {
        return (StatusCode::NOT_FOUND, "file not found").into_response();
    }

    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, format!("read error: {e}")).into_response(),
    };

    let content_type = match path.extension().and_then(|e| e.to_str()) {
        Some("wav") => "audio/wav",
        Some("mp3") => "audio/mpeg",
        Some("ogg") => "audio/ogg",
        Some("flac") => "audio/flac",
        _ => "application/octet-stream",
    };

    Response::builder()
        .header(header::CONTENT_TYPE, content_type)
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .body(Body::from(bytes))
        .unwrap()
}

// ── Model file serving from ~/.atri/model/ ──────────────────────

async fn model_handler(AxumPath(path): AxumPath<String>) -> Response {
    let model_dir = crate::config::atri_dir().join("model");
    let file_path = model_dir.join(&path);

    // Prevent path traversal
    if !file_path.starts_with(&model_dir) {
        return (StatusCode::FORBIDDEN, "forbidden").into_response();
    }

    if !file_path.exists() {
        return (StatusCode::NOT_FOUND, "file not found").into_response();
    }

    let bytes = match std::fs::read(&file_path) {
        Ok(b) => b,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, format!("read error: {e}")).into_response(),
    };

    let content_type = match file_path.extension().and_then(|e| e.to_str()) {
        Some("json") => "application/json",
        Some("moc3") => "application/octet-stream",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("wav") => "audio/wav",
        Some("mp3") => "audio/mpeg",
        _ => "application/octet-stream",
    };

    Response::builder()
        .header(header::CONTENT_TYPE, content_type)
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .body(Body::from(bytes))
        .unwrap()
}

// ── Router & Server ──────────────────────────────────────────────

pub fn create_router(app_handle: AppHandle) -> Router {
    Router::new()
        .route("/status", get(status_handler))
        .route("/expressions", get(expressions_handler))
        .route("/expression", post(expression_handler))
        .route("/motion", post(motion_handler))
        .route("/speak", post(speak_handler))
        .route("/bubble", post(bubble_handler))
        .route("/lipsync/start", post(lipsync_start_handler))
        .route("/lipsync/stop", post(lipsync_stop_handler))
        .route("/audio", get(audio_handler))
        .route("/model/{*path}", get(model_handler))
        .layer(CorsLayer::permissive())
        .with_state(app_handle)
}

#[cfg(test)]
mod tests {
    use super::write_speak_audio;
    use base64::Engine;

    fn test_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("atri-speak-test-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn writes_decoded_audio_with_default_mp3_ext() {
        let dir = test_dir("default");
        let data = base64::engine::general_purpose::STANDARD.encode(b"fake-mp3-bytes");
        let path = write_speak_audio(&dir, &data, None).unwrap();
        assert_eq!(path, dir.join("speak.mp3"));
        assert_eq!(std::fs::read(&path).unwrap(), b"fake-mp3-bytes");
    }

    #[test]
    fn respects_audio_format_and_overwrites() {
        let dir = test_dir("wav");
        let first = base64::engine::general_purpose::STANDARD.encode(b"first");
        let second = base64::engine::general_purpose::STANDARD.encode(b"second");
        write_speak_audio(&dir, &first, Some("wav")).unwrap();
        let path = write_speak_audio(&dir, &second, Some("wav")).unwrap();
        assert_eq!(path, dir.join("speak.wav"));
        assert_eq!(std::fs::read(&path).unwrap(), b"second");
    }

    #[test]
    fn tolerates_line_wrapped_base64() {
        let dir = test_dir("wrapped");
        let data = base64::engine::general_purpose::STANDARD.encode(vec![0u8; 100]);
        let wrapped: String = data
            .as_bytes()
            .chunks(20)
            .map(|c| std::str::from_utf8(c).unwrap())
            .collect::<Vec<_>>()
            .join("\n");
        let path = write_speak_audio(&dir, &wrapped, None).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), vec![0u8; 100]);
    }

    #[test]
    fn rejects_unknown_format_and_bad_base64() {
        let dir = test_dir("errors");
        let data = base64::engine::general_purpose::STANDARD.encode(b"x");
        assert!(write_speak_audio(&dir, &data, Some("exe")).is_err());
        assert!(write_speak_audio(&dir, "not-base64!!!", None).is_err());
    }
}

pub async fn start_server(app_handle: AppHandle, config: AtriConfig) {
    let port = config.api_port;
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let router = create_router(app_handle);

    println!("ATRI API server listening on http://{addr}");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("failed to bind ATRI API server");
    axum::serve(listener, router)
        .await
        .expect("ATRI API server error");
}
