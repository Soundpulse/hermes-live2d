//! Pure Sentry Mode logic: sample classification, focus accounting and the
//! violation escalation state machine. Nothing in here touches Tauri, the
//! network or the OS, so it is fully unit tested.

use serde::Serialize;

// ── Defaults ────────────────────────────────────────────────────

pub const DEFAULT_GRACE_SECONDS: u64 = 60;
pub const DEFAULT_INTERVAL_SECONDS: u64 = 10;
pub const MIN_INTERVAL_SECONDS: u64 = 2;
pub const MAX_INTERVAL_SECONDS: u64 = 300;
pub const MAX_GRACE_SECONDS: u64 = 3600;
pub const MAX_LEVEL: u8 = 3;
/// Keep the event log bounded for very long sessions.
pub const MAX_EVENTS: usize = 500;

/// Study and work websites. Matched against the browser tab host (suffix match).
pub const DEFAULT_ALLOW_DOMAINS: &[&str] = &[
    "github.com",
    "gitlab.com",
    "bitbucket.org",
    "stackoverflow.com",
    "stackexchange.com",
    "superuser.com",
    "serverfault.com",
    "aws.amazon.com",
    "amazonaws.com",
    "skillbuilder.aws",
    "aws.training",
    "tutorialsdojo.com",
    "whizlabs.com",
    "coursera.org",
    "udemy.com",
    "edx.org",
    "khanacademy.org",
    "pluralsight.com",
    "acloudguru.com",
    "notion.so",
    "notion.site",
    "docs.google.com",
    "drive.google.com",
    "scholar.google.com",
    "developer.mozilla.org",
    "learn.microsoft.com",
    "cloud.google.com",
    "docs.python.org",
    "readthedocs.io",
    "readthedocs.org",
    "docs.rs",
    "crates.io",
    "npmjs.com",
    "wikipedia.org",
    "arxiv.org",
    "overleaf.com",
    "leetcode.com",
    "chatgpt.com",
    "claude.ai",
    "localhost",
    "127.0.0.1",
];

/// Study and work apps. Matched against the frontmost app name (word match).
pub const DEFAULT_ALLOW_APPS: &[&str] = &[
    "code",
    "cursor",
    "xcode",
    "intellij idea",
    "pycharm",
    "webstorm",
    "goland",
    "rustrover",
    "android studio",
    "zed",
    "sublime text",
    "terminal",
    "iterm2",
    "warp",
    "ghostty",
    "alacritty",
    "kitty",
    "wezterm",
    "notion",
    "obsidian",
    "anki",
    "preview",
    "pdf expert",
    "skim",
    "goodnotes",
    "notes",
    "pages",
    "keynote",
    "numbers",
    "microsoft word",
    "microsoft excel",
    "microsoft powerpoint",
];

/// Distraction websites.
pub const DEFAULT_BLOCK_DOMAINS: &[&str] = &[
    "youtube.com",
    "youtu.be",
    "reddit.com",
    "instagram.com",
    "tiktok.com",
    "x.com",
    "twitter.com",
    "facebook.com",
    "threads.net",
    "twitch.tv",
    "netflix.com",
    "disneyplus.com",
    "bilibili.com",
    "b23.tv",
    "xiaohongshu.com",
    "xhslink.com",
    "9gag.com",
    "store.steampowered.com",
    "steamcommunity.com",
    "epicgames.com",
];

/// Distraction apps (games, launchers, streaming).
pub const DEFAULT_BLOCK_APPS: &[&str] = &[
    "steam",
    "steam helper",
    "epic games launcher",
    "battle.net",
    "league of legends",
    "riot client",
    "minecraft",
    "roblox",
    "genshin impact",
    "netflix",
    "tiktok",
    "rednote",
    "xiaohongshu",
    "bilibili",
];

/// Apps whose active tab we read through AppleScript.
pub const BROWSERS: &[&str] = &[
    "Safari",
    "Safari Technology Preview",
    "Google Chrome",
    "Google Chrome Canary",
    "Chromium",
    "Arc",
    "Brave Browser",
    "Microsoft Edge",
];

pub fn is_browser(app: &str) -> bool {
    BROWSERS.iter().any(|b| b.eq_ignore_ascii_case(app))
}

/// Browsers whose tab URL cannot be read through AppleScript. They are judged
/// by window title only, never by their own app name.
pub const TITLE_ONLY_BROWSERS: &[&str] = &[
    "Firefox",
    "Firefox Developer Edition",
    "Firefox Nightly",
    "LibreWolf",
    "Zen",
    "Zen Browser",
    "Opera",
    "Opera GX",
    "Vivaldi",
    "Orion",
];

pub fn is_title_only_browser(app: &str) -> bool {
    TITLE_ONLY_BROWSERS
        .iter()
        .any(|b| b.eq_ignore_ascii_case(app.trim()))
}

// ── Samples and labels ──────────────────────────────────────────

/// One observation of what the user is looking at.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Sample {
    pub app: String,
    pub title: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Label {
    Allowed,
    Blocked,
    Unknown,
}

impl Label {
    pub fn as_str(self) -> &'static str {
        match self {
            Label::Allowed => "allowed",
            Label::Blocked => "blocked",
            Label::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Classification {
    pub label: Label,
    /// Human readable rule that matched, e.g. `default block: youtube.com`.
    pub reason: Option<String>,
}

// ── Patterns ────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum Pattern {
    /// `youtube.com`, `docs.aws.amazon.com/cli`, `https://www.reddit.com/r/aws`.
    Domain {
        raw: String,
        host: String,
        path: Option<String>,
    },
    /// One or more words, e.g. `aws`, `visual studio code`.
    Keyword { raw: String, words: Vec<String> },
}

fn words(s: &str) -> Vec<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

/// True when `needle` appears as a contiguous run of words inside `hay`.
fn contains_words(hay: &[String], needle: &[String]) -> bool {
    if needle.is_empty() || needle.len() > hay.len() {
        return false;
    }
    hay.windows(needle.len()).any(|w| w == needle)
}

fn strip_scheme(s: &str) -> &str {
    s.split_once("://").map(|(_, rest)| rest).unwrap_or(s)
}

fn strip_www(s: &str) -> &str {
    s.strip_prefix("www.").unwrap_or(s)
}

/// Split a URL into `(host, rest)` with scheme, `www.`, userinfo and port removed.
pub fn split_url(url: &str) -> Option<(String, String)> {
    let lower = url.trim().to_lowercase();
    let rest = strip_scheme(&lower);
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(end);
    let authority = authority.rsplit('@').next().unwrap_or(authority);
    let host = authority.split(':').next().unwrap_or(authority);
    let host = strip_www(host).to_string();
    if host.is_empty() {
        return None;
    }
    Some((host, tail.to_string()))
}

impl Pattern {
    fn parse(input: &str) -> Option<Pattern> {
        let raw = input.trim().to_lowercase();
        if raw.is_empty() {
            return None;
        }
        let looks_like_domain = !raw.contains(char::is_whitespace)
            && (raw.contains("://") || raw.contains('.') || raw == "localhost");
        if looks_like_domain {
            let (host, tail) = split_url(&raw)?;
            let path = Some(tail.trim_end_matches('/').to_string()).filter(|p| !p.is_empty());
            return Some(Pattern::Domain { raw, host, path });
        }
        let w = words(&raw);
        if w.is_empty() {
            return None;
        }
        Some(Pattern::Keyword { raw, words: w })
    }

    fn raw(&self) -> &str {
        match self {
            Pattern::Domain { raw, .. } | Pattern::Keyword { raw, .. } => raw,
        }
    }

    fn matches_host(&self, host: &str, tail: &str) -> bool {
        match self {
            Pattern::Domain { host: h, path, .. } => {
                let host_ok = host == h || host.ends_with(&format!(".{h}"));
                host_ok
                    && match path {
                        Some(p) => tail.starts_with(p.as_str()),
                        None => true,
                    }
            }
            Pattern::Keyword { words: w, .. } => {
                // Single word keywords match a host label, e.g. `youtube` in `m.youtube.com`.
                w.len() == 1 && host.split('.').any(|label| label == w[0])
            }
        }
    }

    fn matches_app(&self, app: &str) -> bool {
        match self {
            Pattern::Domain { raw, .. } => app.trim().eq_ignore_ascii_case(raw),
            Pattern::Keyword { words: w, .. } => contains_words(&words(app), w),
        }
    }

    /// Title matching. Domains match by their main label (e.g. `youtube` for
    /// `youtube.com`) so a browser tab can still be judged when its URL is
    /// unavailable. Very short labels such as `x` are skipped.
    fn matches_title(&self, title: &str) -> bool {
        let title_words = words(title);
        match self {
            Pattern::Domain {
                host, path: None, ..
            } => {
                let parts: Vec<&str> = host.split('.').collect();
                let stem = if parts.len() >= 2 {
                    parts[parts.len() - 2]
                } else {
                    parts[0]
                };
                stem.len() >= 4 && contains_words(&title_words, &[stem.to_string()])
            }
            Pattern::Domain { .. } => false,
            Pattern::Keyword { words: w, .. } => contains_words(&title_words, w),
        }
    }
}

// ── Classifier ──────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
enum Source {
    Request,
    Default,
}

#[derive(Debug, Clone)]
struct RuleSet {
    source: Source,
    label: Label,
    /// Patterns judged against a tab URL or a window title.
    web: Vec<Pattern>,
    /// Patterns judged against the frontmost app name.
    apps: Vec<Pattern>,
}

#[derive(Debug, Clone)]
pub struct Classifier {
    /// Ordered by precedence: request block, request allow, default block, default allow.
    rules: Vec<RuleSet>,
}

fn parse_all<S: AsRef<str>>(items: &[S]) -> Vec<Pattern> {
    items
        .iter()
        .filter_map(|s| Pattern::parse(s.as_ref()))
        .collect()
}

impl Classifier {
    /// `allow` and `block` come from the `/sentry/start` request and take
    /// precedence over the built-in defaults, so a request can allow YouTube
    /// lectures or block a normally allowed site.
    pub fn new<S: AsRef<str>>(allow: &[S], block: &[S]) -> Self {
        let request_allow = parse_all(allow);
        let request_block = parse_all(block);
        let rules = vec![
            RuleSet {
                source: Source::Request,
                label: Label::Blocked,
                web: request_block.clone(),
                apps: request_block,
            },
            RuleSet {
                source: Source::Request,
                label: Label::Allowed,
                web: request_allow.clone(),
                apps: request_allow,
            },
            RuleSet {
                source: Source::Default,
                label: Label::Blocked,
                web: parse_all(DEFAULT_BLOCK_DOMAINS),
                apps: parse_all(DEFAULT_BLOCK_APPS),
            },
            RuleSet {
                source: Source::Default,
                label: Label::Allowed,
                web: parse_all(DEFAULT_ALLOW_DOMAINS),
                apps: parse_all(DEFAULT_ALLOW_APPS),
            },
        ];
        Self { rules }
    }

    pub fn classify(&self, sample: &Sample) -> Classification {
        let browser = is_browser(&sample.app) || is_title_only_browser(&sample.app);
        // Title-only browsers never have a trustworthy URL, so always use the title.
        let url = if is_title_only_browser(&sample.app) {
            None
        } else {
            sample.url.as_deref().and_then(split_url)
        };
        let title = sample.title.as_deref().unwrap_or("");

        for set in &self.rules {
            let prefix = match (set.source, set.label) {
                (Source::Request, Label::Blocked) => "request block",
                (Source::Request, _) => "request allow",
                (Source::Default, Label::Blocked) => "default block",
                (Source::Default, _) => "default allow",
            };
            let found = if browser {
                match &url {
                    // Judge the tab, not the browser app itself.
                    Some((host, tail)) => set.web.iter().find(|p| {
                        p.matches_host(host, tail)
                            || (set.source == Source::Request
                                && matches!(p, Pattern::Keyword { .. })
                                && p.matches_title(title))
                    }),
                    // No URL (permission denied, title-only browser such as Firefox,
                    // empty tab): fall back to the title.
                    None => set.web.iter().find(|p| p.matches_title(title)),
                }
            } else {
                set.apps
                    .iter()
                    .find(|p| p.matches_app(&sample.app))
                    .or_else(|| {
                        // Request keywords also match window titles of native apps,
                        // e.g. allow `aws` covers a PDF titled `AWS SAA notes`.
                        if set.source == Source::Request {
                            set.web.iter().find(|p| {
                                matches!(p, Pattern::Keyword { .. }) && p.matches_title(title)
                            })
                        } else {
                            None
                        }
                    })
            };
            if let Some(p) = found {
                return Classification {
                    label: set.label,
                    reason: Some(format!("{prefix}: {}", p.raw())),
                };
            }
        }

        Classification {
            label: Label::Unknown,
            reason: None,
        }
    }
}

// ── Session engine ──────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DistractionEvent {
    /// `blocked` or `unknown`.
    pub label: Label,
    pub app: String,
    pub title: Option<String>,
    pub url: Option<String>,
    pub reason: Option<String>,
    pub started_at: String,
    pub ended_at: String,
    pub seconds: u64,
    #[serde(skip)]
    started_unix: u64,
    #[serde(skip)]
    key: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Violation {
    pub level: u8,
    pub app: String,
    pub title: Option<String>,
    pub url: Option<String>,
    pub reason: Option<String>,
    /// Continuous off-task seconds in the current streak.
    pub off_task_seconds: u64,
    pub ts: String,
    pub ts_unix: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionSummary {
    pub target: String,
    pub started_at: String,
    pub ended_at: String,
    pub duration_seconds: u64,
    pub focused_seconds: u64,
    pub off_task_seconds: u64,
    pub unknown_seconds: u64,
    pub violations: u32,
    pub max_level: u8,
    pub violation_log: Vec<Violation>,
    pub events: Vec<DistractionEvent>,
    pub events_truncated: bool,
}

#[derive(Debug, Clone)]
pub struct SessionConfig {
    pub target: String,
    pub grace_seconds: u64,
    pub interval_seconds: u64,
}

#[derive(Debug, Clone)]
pub struct Engine {
    pub config: SessionConfig,
    classifier: Classifier,
    started_unix: u64,
    last_unix: Option<u64>,
    last_label: Option<Label>,
    pub last_sample: Option<(Sample, Classification)>,
    pub focused_seconds: u64,
    pub off_task_seconds: u64,
    pub unknown_seconds: u64,
    /// Blocked seconds in the current streak (reset only by an allowed sample).
    pub streak_seconds: u64,
    /// Blocked seconds since the last violation fired in this streak (cooldown).
    since_fire_seconds: u64,
    pub violation_count: u32,
    pub violations: Vec<Violation>,
    events: Vec<DistractionEvent>,
    /// Index of the event the previous sample belongs to, if it is still open.
    open_event: Option<usize>,
    events_truncated: bool,
}

impl Engine {
    pub fn new(config: SessionConfig, classifier: Classifier, now_unix: u64) -> Self {
        Self {
            config,
            classifier,
            started_unix: now_unix,
            last_unix: None,
            last_label: None,
            last_sample: None,
            focused_seconds: 0,
            off_task_seconds: 0,
            unknown_seconds: 0,
            streak_seconds: 0,
            since_fire_seconds: 0,
            violation_count: 0,
            violations: Vec::new(),
            events: Vec::new(),
            open_event: None,
            events_truncated: false,
        }
    }

    pub fn started_unix(&self) -> u64 {
        self.started_unix
    }

    pub fn current_level(&self) -> u8 {
        (self.violation_count.min(MAX_LEVEL as u32)) as u8
    }

    /// Longest gap credited between two samples, so a sleeping Mac does not
    /// count hours of off-task time.
    fn max_gap(&self) -> u64 {
        (self.config.interval_seconds * 3).max(30)
    }

    /// Feed one sample. Returns a violation when the user has been continuously
    /// on blocked content for longer than the grace period.
    ///
    /// Time between the previous sample and `now` is credited to the previous
    /// label. Allowed samples reset the off-task streak. Unknown samples are
    /// neutral: they neither punish nor reset.
    pub fn ingest(&mut self, sample: Sample, now_unix: u64) -> (Classification, Option<Violation>) {
        let class = self.classifier.classify(&sample);

        if let (Some(prev_ts), Some(prev_label)) = (self.last_unix, self.last_label) {
            let dt = now_unix.saturating_sub(prev_ts).min(self.max_gap());
            match prev_label {
                Label::Allowed => self.focused_seconds += dt,
                Label::Unknown => self.unknown_seconds += dt,
                Label::Blocked => {
                    self.off_task_seconds += dt;
                    self.streak_seconds += dt;
                    self.since_fire_seconds += dt;
                }
            }
            if let Some(open) = self.open_event.and_then(|i| self.events.get_mut(i)) {
                open.seconds = now_unix.saturating_sub(open.started_unix);
                open.ended_at = rfc3339(now_unix);
            }
        }

        self.record_event(&sample, &class, now_unix);

        let mut violation = None;
        match class.label {
            Label::Allowed => {
                self.streak_seconds = 0;
                self.since_fire_seconds = 0;
            }
            Label::Unknown => {}
            Label::Blocked => {
                if self.since_fire_seconds >= self.config.grace_seconds {
                    self.violation_count += 1;
                    self.since_fire_seconds = 0;
                    let v = Violation {
                        level: self.current_level(),
                        app: sample.app.clone(),
                        title: sample.title.clone(),
                        url: sample.url.clone(),
                        reason: class.reason.clone(),
                        off_task_seconds: self.streak_seconds,
                        ts: rfc3339(now_unix),
                        ts_unix: now_unix,
                    };
                    self.violations.push(v.clone());
                    violation = Some(v);
                }
            }
        }

        self.last_unix = Some(now_unix);
        self.last_label = Some(class.label);
        self.last_sample = Some((sample, class.clone()));
        (class, violation)
    }

    /// Open a new event when the user lands on blocked or unknown content,
    /// merging consecutive samples of the same app and site.
    fn record_event(&mut self, sample: &Sample, class: &Classification, now_unix: u64) {
        if class.label == Label::Allowed {
            self.open_event = None;
            return;
        }
        let host = sample.url.as_deref().and_then(split_url).map(|(h, _)| h);
        let key = format!(
            "{}|{}|{}",
            class.label.as_str(),
            sample.app.to_lowercase(),
            host.unwrap_or_default()
        );
        if let Some(open) = self.open_event.and_then(|i| self.events.get_mut(i)) {
            if open.key == key {
                // Keep the most recent title/url so the log shows what was on screen.
                open.title = sample.title.clone();
                open.url = sample.url.clone();
                return;
            }
        }
        if self.events.len() >= MAX_EVENTS {
            self.events_truncated = true;
            self.open_event = None;
            return;
        }
        self.open_event = Some(self.events.len());
        self.events.push(DistractionEvent {
            label: class.label,
            app: sample.app.clone(),
            title: sample.title.clone(),
            url: sample.url.clone(),
            reason: class.reason.clone(),
            started_at: rfc3339(now_unix),
            ended_at: String::new(),
            seconds: 0,
            started_unix: now_unix,
            key,
        });
    }

    pub fn summary(&self, now_unix: u64) -> SessionSummary {
        // Close the final open segment at stop time.
        let mut engine = self.clone();
        if let (Some(prev_ts), Some(prev_label)) = (engine.last_unix, engine.last_label) {
            let dt = now_unix.saturating_sub(prev_ts).min(engine.max_gap());
            match prev_label {
                Label::Allowed => engine.focused_seconds += dt,
                Label::Unknown => engine.unknown_seconds += dt,
                Label::Blocked => engine.off_task_seconds += dt,
            }
            if let Some(open) = engine.open_event.and_then(|i| engine.events.get_mut(i)) {
                open.seconds = now_unix.saturating_sub(open.started_unix);
                open.ended_at = rfc3339(now_unix);
            }
        }
        for e in engine.events.iter_mut().filter(|e| e.ended_at.is_empty()) {
            e.ended_at = e.started_at.clone();
        }
        SessionSummary {
            target: engine.config.target.clone(),
            started_at: rfc3339(engine.started_unix),
            ended_at: rfc3339(now_unix),
            duration_seconds: now_unix.saturating_sub(engine.started_unix),
            focused_seconds: engine.focused_seconds,
            off_task_seconds: engine.off_task_seconds,
            unknown_seconds: engine.unknown_seconds,
            violations: engine.violation_count,
            max_level: engine.current_level(),
            violation_log: engine.violations.clone(),
            events: engine.events,
            events_truncated: engine.events_truncated,
        }
    }
}

// ── Local fallback roasts ───────────────────────────────────────

/// Cantonese bubble text used when no `callback_url` is configured (or the
/// callback fails), so the pet still scolds locally.
pub fn fallback_roast(level: u8, target: &str) -> String {
    let target = if target.trim().is_empty() {
        "溫書"
    } else {
        target.trim()
    };
    match level {
        0 | 1 => format!("喂！你話{target}㗎喎，碌乜嘢呀？返去{target}！"),
        2 => format!("又嚟？！第二次喇！{target}呢？你係咪唔想合格呀？即刻返去！"),
        _ => format!("第三次！我真係嬲㗎喇！再唔返去{target}，我就公開處刑你！"),
    }
}

// ── Time formatting ─────────────────────────────────────────────

/// Format unix seconds as an RFC 3339 UTC timestamp, e.g. `2026-10-13T08:30:00Z`.
pub fn rfc3339(unix: u64) -> String {
    let days = (unix / 86_400) as i64;
    let secs = unix % 86_400;
    // Civil-from-days (Howard Hinnant), valid for the Gregorian calendar.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + if month <= 2 { 1 } else { 0 };
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        secs / 3600,
        (secs % 3600) / 60,
        secs % 60
    )
}

// ── Probe output parsing ────────────────────────────────────────

/// Parse `osascript` output of the form `<line1>\n<line2...>` into two parts.
/// Empty strings become `None`.
pub fn parse_two_lines(output: &str) -> (Option<String>, Option<String>) {
    let trimmed = output.trim_end_matches(['\n', '\r']);
    let (first, second) = match trimmed.split_once('\n') {
        Some((a, b)) => (a, b),
        None => (trimmed, ""),
    };
    let clean = |s: &str| {
        let s = s.trim();
        if s.is_empty() || s == "missing value" {
            None
        } else {
            Some(s.to_string())
        }
    };
    (clean(first), clean(second))
}

/// Map an AppleScript error message to a short, actionable hint.
pub fn permission_hint(stderr: &str, app: &str) -> String {
    if stderr.contains("-1743") || stderr.to_lowercase().contains("not authorized") {
        format!(
            "Automation permission denied for {app}. Allow Sparky under System Settings > Privacy & Security > Automation."
        )
    } else if stderr.contains("-25211") || stderr.to_lowercase().contains("assistive access") {
        "Accessibility permission denied. Allow Sparky under System Settings > Privacy & Security > Accessibility.".to_string()
    } else {
        let line = stderr.lines().next().unwrap_or("").trim();
        format!("osascript failed for {app}: {line}")
    }
}

/// Parse `lsappinfo info -only name <asn>` output, e.g. `"LSDisplayName"="Safari"`.
pub fn parse_lsappinfo_name(output: &str) -> Option<String> {
    let line = output.lines().find(|l| l.contains('='))?;
    let value = line.split_once('=')?.1.trim().trim_matches('"').trim();
    if value.is_empty() || value == "NULL" {
        None
    } else {
        Some(value.to_string())
    }
}

/// Hermes generic webhook V2 signature: hex HMAC-SHA256 of `<timestamp>.<raw body>`.
pub fn sign_webhook_v2(secret: &str, timestamp: u64, body: &str) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC accepts keys of any length");
    mac.update(format!("{timestamp}.").as_bytes());
    mac.update(body.as_bytes());
    mac.finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Accept only plain http(s) URLs without whitespace or control characters,
/// since the value is handed to curl.
pub fn validate_callback_url(url: &str) -> Result<(), String> {
    let lower = url.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return Err("callback_url must start with http:// or https://".into());
    }
    if url.chars().any(|c| c.is_whitespace() || c.is_control()) || url.len() > 2048 {
        return Err("callback_url contains invalid characters".into());
    }
    let rest = strip_scheme(url);
    let authority = &rest[..rest.find(['/', '?', '#']).unwrap_or(rest.len())];
    if authority.contains('@') {
        return Err("callback_url must not contain credentials".into());
    }
    match split_url(url) {
        Some((host, _)) if !host.is_empty() => Ok(()),
        _ => Err("callback_url has no host".into()),
    }
}

/// Origins of the pet's own webview: the bundled app (`tauri://localhost` on
/// macOS, `http(s)://tauri.localhost` elsewhere) and the Vite dev server.
/// Every other browser origin is refused by the Sentry endpoints.
pub const APP_ORIGINS: &[&str] = &[
    "tauri://localhost",
    "http://tauri.localhost",
    "https://tauri.localhost",
    "http://localhost:1420",
    "http://127.0.0.1:1420",
];

pub fn is_app_origin(origin: &str) -> bool {
    APP_ORIGINS
        .iter()
        .any(|o| o.eq_ignore_ascii_case(origin.trim()))
}

// ── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn web(app: &str, url: &str, title: &str) -> Sample {
        Sample {
            app: app.into(),
            url: Some(url.into()),
            title: Some(title.into()),
        }
    }

    fn native(app: &str, title: &str) -> Sample {
        Sample {
            app: app.into(),
            url: None,
            title: Some(title.into()),
        }
    }

    fn defaults() -> Classifier {
        Classifier::new::<&str>(&[], &[])
    }

    fn engine(grace: u64, interval: u64, allow: &[&str], block: &[&str]) -> Engine {
        Engine::new(
            SessionConfig {
                target: "溫書 AWS exam".into(),
                grace_seconds: grace,
                interval_seconds: interval,
            },
            Classifier::new(allow, block),
            1_000,
        )
    }

    #[test]
    fn default_block_sites_are_blocked() {
        let c = defaults();
        for url in [
            "https://www.youtube.com/watch?v=abc",
            "https://m.youtube.com/",
            "https://old.reddit.com/r/aws",
            "https://x.com/home",
            "https://twitter.com/foo",
            "https://www.bilibili.com/video/BV1",
            "https://www.xiaohongshu.com/explore",
            "https://www.twitch.tv/somebody",
            "https://www.netflix.com/browse",
            "https://store.steampowered.com/app/1",
        ] {
            let r = c.classify(&web("Google Chrome", url, "whatever"));
            assert_eq!(r.label, Label::Blocked, "{url}");
            assert!(r.reason.unwrap().starts_with("default block"));
        }
    }

    #[test]
    fn default_allow_sites_and_apps_are_allowed() {
        let c = defaults();
        for url in [
            "https://docs.aws.amazon.com/lambda/latest/dg/welcome.html",
            "https://github.com/Soundpulse/hermes-live2d",
            "https://stackoverflow.com/questions/1",
            "https://www.coursera.org/learn/aws",
            "https://www.notion.so/page",
            "https://docs.google.com/document/d/1",
            "http://localhost:5173/",
        ] {
            assert_eq!(
                c.classify(&web("Safari", url, "t")).label,
                Label::Allowed,
                "{url}"
            );
        }
        for app in [
            "Code",
            "Visual Studio Code",
            "Terminal",
            "iTerm2",
            "Xcode",
            "Notion",
        ] {
            assert_eq!(
                c.classify(&native(app, "main.rs")).label,
                Label::Allowed,
                "{app}"
            );
        }
    }

    #[test]
    fn lookalike_hosts_do_not_match() {
        let c = defaults();
        // `notyoutube.com` must not match `youtube.com`; `box.com` must not match `x.com`.
        assert_eq!(
            c.classify(&web("Arc", "https://notyoutube.com/", "t"))
                .label,
            Label::Unknown
        );
        assert_eq!(
            c.classify(&web("Arc", "https://box.com/", "t")).label,
            Label::Unknown
        );
    }

    #[test]
    fn unknown_is_neutral() {
        let c = defaults();
        assert_eq!(
            c.classify(&native("Finder", "Downloads")).label,
            Label::Unknown
        );
        assert_eq!(
            c.classify(&native("Discord", "#general")).label,
            Label::Unknown
        );
        assert_eq!(
            c.classify(&web("Brave Browser", "https://news.ycombinator.com/", "HN"))
                .label,
            Label::Unknown
        );
    }

    #[test]
    fn browser_app_name_never_decides_when_url_present() {
        // A request allow of `chrome` must not whitelist every tab in Chrome.
        let c = Classifier::new(&["chrome"], &[]);
        assert_eq!(
            c.classify(&web("Google Chrome", "https://www.youtube.com/", "cats"))
                .label,
            Label::Blocked
        );
    }

    #[test]
    fn game_apps_are_blocked() {
        let c = defaults();
        assert_eq!(
            c.classify(&native("Steam", "Library")).label,
            Label::Blocked
        );
        assert_eq!(c.classify(&native("Battle.net", "")).label, Label::Blocked);
        assert_eq!(
            c.classify(&native("League of Legends", "")).label,
            Label::Blocked
        );
    }

    #[test]
    fn request_rules_override_defaults() {
        let c = Classifier::new(&["youtube.com/watch?v=lecture"], &["github.com"]);
        let lecture = c.classify(&web(
            "Safari",
            "https://www.youtube.com/watch?v=lecture1",
            "AWS",
        ));
        assert_eq!(lecture.label, Label::Allowed);
        assert_eq!(
            lecture.reason.as_deref(),
            Some("request allow: youtube.com/watch?v=lecture")
        );
        // Other YouTube videos stay blocked.
        assert_eq!(
            c.classify(&web(
                "Safari",
                "https://www.youtube.com/watch?v=cats",
                "cats"
            ))
            .label,
            Label::Blocked
        );
        // Request block beats default allow.
        let gh = c.classify(&web("Safari", "https://github.com/trending", "t"));
        assert_eq!(gh.label, Label::Blocked);
        assert_eq!(gh.reason.as_deref(), Some("request block: github.com"));
    }

    #[test]
    fn request_block_beats_request_allow() {
        let c = Classifier::new(&["reddit.com"], &["reddit.com/r/funny"]);
        assert_eq!(
            c.classify(&web("Arc", "https://www.reddit.com/r/aws/", "t"))
                .label,
            Label::Allowed
        );
        assert_eq!(
            c.classify(&web("Arc", "https://www.reddit.com/r/funny/", "t"))
                .label,
            Label::Blocked
        );
    }

    #[test]
    fn request_keywords_match_titles_and_apps() {
        let c = Classifier::new(&["aws"], &["slack"]);
        assert_eq!(
            c.classify(&web(
                "Safari",
                "https://www.youtube.com/watch?v=1",
                "AWS SAA full course - YouTube"
            ))
            .label,
            Label::Allowed
        );
        assert_eq!(
            c.classify(&native("Preview", "AWS notes.pdf")).label,
            Label::Allowed
        );
        assert_eq!(
            c.classify(&native("Slack", "general")).label,
            Label::Blocked
        );
    }

    #[test]
    fn browser_without_url_falls_back_to_title() {
        let c = defaults();
        let mut s = web("Safari", "", "Funny cats - YouTube");
        s.url = None;
        assert_eq!(c.classify(&s).label, Label::Blocked);
        // Short stems like `x` are never matched against titles.
        s.title = Some("x marks the spot".into());
        assert_eq!(c.classify(&s).label, Label::Unknown);
    }

    #[test]
    fn title_only_browsers_are_judged_by_title() {
        let c = defaults();
        for app in ["Firefox", "firefox", "Zen", "Vivaldi", "Opera"] {
            let r = c.classify(&native(app, "Funny cats - YouTube"));
            assert_eq!(r.label, Label::Blocked, "{app}");
            assert_eq!(r.reason.as_deref(), Some("default block: youtube.com"));
        }
        // The browser's own app name never decides, even against a request allow.
        let c = Classifier::new(&["firefox"], &[]);
        assert_eq!(
            c.classify(&native("Firefox", "Funny cats - YouTube")).label,
            Label::Blocked
        );
        // Request keywords still match the title.
        let c = Classifier::new(&["aws"], &[]);
        assert_eq!(
            c.classify(&native("Firefox", "AWS SAA practice exam - YouTube"))
                .label,
            Label::Allowed
        );
        // Unlisted titles stay neutral.
        assert_eq!(
            defaults().classify(&native("Firefox", "Hacker News")).label,
            Label::Unknown
        );
        // `is_browser` (used by the probe for AppleScript tab reads) is unchanged.
        assert!(!is_browser("Firefox"));
        assert!(is_title_only_browser("FIREFOX"));
    }

    #[test]
    fn split_url_handles_ports_userinfo_and_case() {
        assert_eq!(
            split_url("HTTPS://User@WWW.Example.COM:8443/a?b#c"),
            Some(("example.com".into(), "/a?b#c".into()))
        );
        assert_eq!(
            split_url("about:blank").map(|(h, _)| h),
            Some("about".into())
        );
        assert_eq!(split_url(""), None);
    }

    #[test]
    fn no_violation_within_grace() {
        let mut e = engine(60, 10, &[], &[]);
        let yt = web("Safari", "https://youtube.com/watch", "cats");
        for i in 0..6 {
            let (_, v) = e.ingest(yt.clone(), 1_000 + i * 10);
            assert!(v.is_none(), "fired early at sample {i}");
        }
        assert_eq!(e.streak_seconds, 50);
    }

    #[test]
    fn violation_fires_after_grace_and_escalates_with_cooldown() {
        let mut e = engine(60, 10, &[], &[]);
        let yt = web("Safari", "https://youtube.com/watch", "cats");
        let mut fired = Vec::new();
        for i in 0..=20u64 {
            if let (_, Some(v)) = e.ingest(yt.clone(), 1_000 + i * 10) {
                fired.push((i, v.level, v.off_task_seconds));
            }
        }
        // 60s of continuous blocked time per violation; levels escalate within one streak.
        assert_eq!(fired, vec![(6, 1, 60), (12, 2, 120), (18, 3, 180)]);
    }

    #[test]
    fn level_caps_at_three() {
        let mut e = engine(10, 10, &[], &[]);
        let yt = web("Safari", "https://youtube.com/", "t");
        let mut levels = Vec::new();
        for i in 0..=8u64 {
            if let (_, Some(v)) = e.ingest(yt.clone(), 1_000 + i * 10) {
                levels.push(v.level);
            }
        }
        assert_eq!(levels, vec![1, 2, 3, 3, 3, 3, 3, 3]);
        assert_eq!(e.violation_count, 8);
    }

    #[test]
    fn returning_on_task_resets_streak_but_not_level() {
        let mut e = engine(30, 10, &[], &[]);
        let yt = web("Safari", "https://youtube.com/", "t");
        let docs = web("Safari", "https://docs.aws.amazon.com/", "docs");
        let mut t = 1_000;
        let mut levels = Vec::new();
        for round in 0..2 {
            for _ in 0..4 {
                if let (_, Some(v)) = e.ingest(yt.clone(), t) {
                    levels.push(v.level);
                }
                t += 10;
            }
            // Back to work: streak resets, so the next round needs a full grace again.
            e.ingest(docs.clone(), t);
            assert_eq!(e.streak_seconds, 0, "round {round}");
            t += 10;
        }
        assert_eq!(levels, vec![1, 2]);
    }

    #[test]
    fn unknown_neither_punishes_nor_resets() {
        let mut e = engine(30, 10, &[], &[]);
        let yt = web("Safari", "https://youtube.com/", "t");
        let finder = native("Finder", "Downloads");
        e.ingest(yt.clone(), 1_000);
        e.ingest(yt.clone(), 1_010);
        // 100s in Finder: neutral, no violation, streak preserved.
        for i in 0..10 {
            let (_, v) = e.ingest(finder.clone(), 1_020 + i * 10);
            assert!(v.is_none());
        }
        assert_eq!(e.streak_seconds, 20);
        // Back on YouTube: only 10 more blocked seconds needed... after the next sample.
        let (_, v) = e.ingest(yt.clone(), 1_120);
        assert!(v.is_none());
        let (_, v) = e.ingest(yt.clone(), 1_130);
        assert_eq!(v.map(|v| v.level), Some(1));
        assert_eq!(e.unknown_seconds, 100);
    }

    #[test]
    fn sleep_gaps_are_clamped() {
        let mut e = engine(60, 10, &[], &[]);
        let yt = web("Safari", "https://youtube.com/", "t");
        e.ingest(yt.clone(), 1_000);
        // Mac slept for an hour: only max(3 * interval, 30) = 30s are credited.
        e.ingest(yt.clone(), 4_600);
        assert_eq!(e.off_task_seconds, 30);
    }

    #[test]
    fn summary_accounts_time_and_events() {
        let mut e = engine(60, 10, &[], &[]);
        let docs = web("Safari", "https://docs.aws.amazon.com/", "docs");
        let yt = web("Safari", "https://www.youtube.com/watch?v=1", "cats");
        let yt2 = web("Safari", "https://www.youtube.com/watch?v=2", "dogs");
        let finder = native("Finder", "Downloads");
        e.ingest(docs.clone(), 1_000);
        e.ingest(docs.clone(), 1_010);
        e.ingest(yt.clone(), 1_020);
        e.ingest(yt2.clone(), 1_030); // same site, merged into one event
        e.ingest(finder.clone(), 1_040);
        e.ingest(docs.clone(), 1_050);
        let s = e.summary(1_060);
        assert_eq!(s.duration_seconds, 60);
        assert_eq!(s.focused_seconds, 30);
        assert_eq!(s.off_task_seconds, 20);
        assert_eq!(s.unknown_seconds, 10);
        assert_eq!(s.events.len(), 2);
        assert_eq!(s.events[0].label, Label::Blocked);
        assert_eq!(s.events[0].seconds, 20);
        assert_eq!(s.events[0].title.as_deref(), Some("dogs"));
        assert_eq!(s.events[0].started_at, rfc3339(1_020));
        assert_eq!(s.events[0].ended_at, rfc3339(1_040));
        assert_eq!(s.events[1].label, Label::Unknown);
        assert_eq!(s.events[1].app, "Finder");
        assert_eq!(s.events[1].seconds, 10);
        let json = serde_json::to_value(&s).unwrap();
        assert_eq!(json["events"][1]["label"], "unknown");
        assert!(json["events"][0].get("key").is_none());
    }

    #[test]
    fn summary_closes_open_blocked_segment() {
        let mut e = engine(60, 10, &[], &[]);
        e.ingest(web("Safari", "https://reddit.com/", "r"), 1_000);
        let s = e.summary(1_015);
        assert_eq!(s.off_task_seconds, 15);
        assert_eq!(s.events[0].seconds, 15);
        assert_eq!(s.events[0].ended_at, rfc3339(1_015));
    }

    #[test]
    fn events_are_bounded() {
        let mut e = engine(10_000, 10, &[], &[]);
        for i in 0..(MAX_EVENTS as u64 + 50) {
            let s = native(&format!("App{i}"), "");
            e.ingest(s, 1_000 + i);
        }
        let s = e.summary(10_000);
        assert_eq!(s.events.len(), MAX_EVENTS);
        assert!(s.events_truncated);
    }

    #[test]
    fn rfc3339_formats_known_dates() {
        assert_eq!(rfc3339(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(rfc3339(1_791_885_600), "2026-10-13T10:00:00Z");
    }

    #[test]
    fn parses_osascript_output() {
        assert_eq!(
            parse_two_lines("Safari\nAWS docs - Lambda\n"),
            (Some("Safari".into()), Some("AWS docs - Lambda".into()))
        );
        assert_eq!(parse_two_lines("Finder\n"), (Some("Finder".into()), None));
        assert_eq!(parse_two_lines("missing value\n"), (None, None));
        assert_eq!(
            parse_two_lines("https://a.com\ntitle\nwith newline"),
            (
                Some("https://a.com".into()),
                Some("title\nwith newline".into())
            )
        );
    }

    #[test]
    fn permission_hints_are_actionable() {
        assert!(permission_hint(
            "execution error: Not authorized to send Apple events to Safari. (-1743)",
            "Safari"
        )
        .contains("Automation"));
        assert!(permission_hint(
            "System Events got an error: osascript is not allowed assistive access. (-25211)",
            "System Events"
        )
        .contains("Accessibility"));
    }

    #[test]
    fn parses_lsappinfo_output() {
        assert_eq!(
            parse_lsappinfo_name("\"LSDisplayName\"=\"Safari\"\n"),
            Some("Safari".into())
        );
        assert_eq!(
            parse_lsappinfo_name("\"LSDisplayName\"=\"Visual Studio Code\""),
            Some("Visual Studio Code".into())
        );
        assert_eq!(parse_lsappinfo_name(""), None);
        assert_eq!(parse_lsappinfo_name("\"LSDisplayName\"=NULL"), None);
    }

    #[test]
    fn signs_like_hermes_webhook_v2() {
        // Expected value computed independently with Python's hmac module.
        assert_eq!(
            sign_webhook_v2(
                "test-secret",
                1_700_000_000,
                r#"{"type":"sentry_violation"}"#
            ),
            "2ebe69a5d9c7ee124dc477a659921921ebd844415402ec21c819ee6152334877"
        );
    }

    #[test]
    fn validates_callback_urls() {
        assert!(validate_callback_url("https://hermes.tail1234.ts.net/webhooks/sentry").is_ok());
        assert!(validate_callback_url("http://100.64.0.1:8644/webhooks/sentry").is_ok());
        assert!(validate_callback_url("file:///etc/passwd").is_err());
        assert!(validate_callback_url("-K/etc/passwd").is_err());
        assert!(validate_callback_url("https://a.com/x y").is_err());
        assert!(validate_callback_url("https://").is_err());
        assert!(validate_callback_url("javascript:alert(1)").is_err());
        assert!(validate_callback_url("gopher://a.com/").is_err());
        assert!(validate_callback_url("https://trusted.example@evil.example/").is_err());
        assert!(validate_callback_url("https://a.com/path?x=a@b").is_ok());
    }

    #[test]
    fn only_app_origins_are_accepted() {
        for o in [
            "tauri://localhost",
            "http://tauri.localhost",
            "https://tauri.localhost",
            "http://localhost:1420",
        ] {
            assert!(is_app_origin(o), "{o}");
        }
        for o in [
            "https://evil.example",
            "http://localhost:3000",
            "http://127.0.0.1:3210",
            "null",
            "",
            "tauri://localhost.evil.example",
        ] {
            assert!(!is_app_origin(o), "{o}");
        }
    }

    #[test]
    fn fallback_roasts_escalate() {
        assert!(fallback_roast(1, "溫書").contains("碌乜嘢"));
        assert!(fallback_roast(2, "AWS").contains("第二次"));
        assert!(fallback_roast(3, "").contains("公開處刑"));
        assert!(fallback_roast(9, "x").contains("第三次"));
    }
}
