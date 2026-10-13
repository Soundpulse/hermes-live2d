//! Reads what is on screen right now: frontmost app, its window title and,
//! for supported browsers, the active tab URL and title. Uses `osascript`
//! and `lsappinfo` only. No screenshots, no keylogging.

use super::core::{
    is_browser, parse_lsappinfo_name, parse_two_lines, permission_hint, Sample, BROWSERS,
};
use std::time::Duration;
use tokio::process::Command;

const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

pub struct ProbeResult {
    pub sample: Sample,
    /// Permission or tooling problem worth surfacing in `/sentry/status`.
    pub error: Option<String>,
}

const FRONT_APP_SCRIPT: &str = r#"tell application "System Events"
	set frontProc to first application process whose frontmost is true
	set appName to name of frontProc
	set winTitle to ""
	try
		set winTitle to name of front window of frontProc
	end try
end tell
return appName & linefeed & winTitle"#;

/// Run a command with a timeout, returning stdout or stderr as the error.
async fn run(program: &str, args: &[&str]) -> Result<String, String> {
    let child = Command::new(program).args(args).kill_on_drop(true).output();
    match tokio::time::timeout(PROBE_TIMEOUT, child).await {
        Err(_) => Err(format!("{program} timed out")),
        Ok(Err(e)) => Err(format!("failed to run {program}: {e}")),
        Ok(Ok(out)) if out.status.success() => {
            Ok(String::from_utf8_lossy(&out.stdout).into_owned())
        }
        Ok(Ok(out)) => Err(String::from_utf8_lossy(&out.stderr).into_owned()),
    }
}

async fn osascript(script: &str) -> Result<String, String> {
    run("osascript", &["-e", script]).await
}

/// Frontmost app name without any permission prompt (no window title).
async fn lsappinfo_front_app() -> Option<String> {
    let asn = run("lsappinfo", &["front"]).await.ok()?;
    let asn = asn.trim();
    if asn.is_empty() {
        return None;
    }
    let info = run("lsappinfo", &["info", "-only", "name", asn])
        .await
        .ok()?;
    parse_lsappinfo_name(&info)
}

fn browser_script(app: &str) -> String {
    // `app` comes from the fixed BROWSERS list, never from user input.
    if app.starts_with("Safari") {
        format!(
            r#"tell application "{app}" to return (URL of current tab of front window) & linefeed & (name of current tab of front window)"#
        )
    } else {
        format!(
            r#"tell application "{app}" to return (URL of active tab of front window) & linefeed & (title of active tab of front window)"#
        )
    }
}

pub async fn sample() -> ProbeResult {
    if !cfg!(target_os = "macos") {
        return ProbeResult {
            sample: Sample {
                app: "unknown".into(),
                ..Default::default()
            },
            error: Some("Sentry Mode probing is only supported on macOS".into()),
        };
    }

    let mut errors: Vec<String> = Vec::new();

    let (app, mut title) = match osascript(FRONT_APP_SCRIPT).await {
        Ok(out) => parse_two_lines(&out),
        Err(stderr) => {
            errors.push(permission_hint(&stderr, "System Events"));
            (lsappinfo_front_app().await, None)
        }
    };
    let app = app.unwrap_or_else(|| "unknown".to_string());

    let mut url = None;
    if is_browser(&app) {
        let canonical = BROWSERS
            .iter()
            .find(|b| b.eq_ignore_ascii_case(&app))
            .copied()
            .unwrap_or("Safari");
        match osascript(&browser_script(canonical)).await {
            Ok(out) => {
                let (tab_url, tab_title) = parse_two_lines(&out);
                url = tab_url;
                if tab_title.is_some() {
                    title = tab_title;
                }
            }
            // No window open is normal, only report real failures.
            Err(stderr) if !stderr.contains("-1719") && !stderr.contains("-1728") => {
                errors.push(permission_hint(&stderr, canonical));
            }
            Err(_) => {}
        }
    }

    ProbeResult {
        sample: Sample { app, title, url },
        error: if errors.is_empty() {
            None
        } else {
            Some(errors.join(" "))
        },
    }
}
