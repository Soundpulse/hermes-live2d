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

async fn speak_handler(
    State(app): State<AppHandle>,
    Json(payload): Json<SpeakReq>,
) -> Result<Json<ApiResponse>, (StatusCode, Json<ApiResponse>)> {
    app.emit("api:speak", &payload)
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
