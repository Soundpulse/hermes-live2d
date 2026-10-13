use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SparkyConfig {
    #[serde(default = "default_api_port")]
    pub api_port: u16,
    #[serde(default)]
    pub model_dir: Option<String>,
    /// Shared token that `/sentry/*` requires as `Authorization: Bearer`.
    /// `SPARKY_API_TOKEN` in the environment overrides it. With neither set,
    /// the Sentry endpoints are disabled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_token: Option<String>,
}

fn default_api_port() -> u16 {
    3210
}

impl Default for SparkyConfig {
    fn default() -> Self {
        Self {
            api_port: default_api_port(),
            model_dir: None,
            api_token: None,
        }
    }
}

pub fn sparky_dir() -> PathBuf {
    dirs::home_dir()
        .expect("cannot determine home directory")
        .join(".sparky")
}

// ── Window state persistence ────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowState {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

pub fn load_window_state() -> Option<WindowState> {
    let path = sparky_dir().join("window_state.json");
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

pub fn save_window_state(state: &WindowState) {
    let path = sparky_dir().join("window_state.json");
    if let Ok(json) = serde_json::to_string_pretty(state) {
        let _ = std::fs::write(path, json);
    }
}

// ── App config ──────────────────────────────────────────────────

pub fn load_config() -> SparkyConfig {
    let dir = sparky_dir();
    let config_path = dir.join("config.json");

    if !dir.exists() {
        std::fs::create_dir_all(&dir).expect("failed to create ~/.sparky");
    }

    if config_path.exists() {
        let content = std::fs::read_to_string(&config_path).unwrap_or_default();
        serde_json::from_str(&content).unwrap_or_else(|e| {
            eprintln!("warn: failed to parse config.json: {e}, using defaults");
            SparkyConfig::default()
        })
    } else {
        let config = SparkyConfig::default();
        let json = serde_json::to_string_pretty(&config).unwrap();
        let _ = std::fs::write(&config_path, json);
        println!("Created default config at {}", config_path.display());
        config
    }
}
