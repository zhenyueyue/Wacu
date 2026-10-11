//! Read-only Codex account quota adapter. Credentials stay inside Codex.
use crate::child_process::ProcessJob;
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant},
};

const TIMEOUT: Duration = Duration::from_secs(30);
pub const REFRESH_INTERVAL: Duration = Duration::from_secs(120);

pub use crate::quota::QuotaWindow;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bucket {
    pub limit_id: Option<String>,
    pub limit_name: Option<String>,
    pub plan_type: Option<String>,
    pub primary: Option<QuotaWindow>,
    pub secondary: Option<QuotaWindow>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Snapshot {
    pub collected_at: i64,
    pub buckets: Vec<Bucket>,
}

impl Snapshot {
    pub fn main_bucket(&self) -> Option<&Bucket> {
        self.buckets
            .iter()
            .find(|b| b.limit_id.as_deref() == Some("codex"))
            .or_else(|| self.buckets.iter().find(|b| b.limit_id.is_none()))
    }

    pub fn main_window(&self) -> Option<&QuotaWindow> {
        let bucket = self.main_bucket()?;
        bucket
            .primary
            .as_ref()
            .filter(|w| w.percent().is_some())
            .or_else(|| bucket.secondary.as_ref().filter(|w| w.percent().is_some()))
    }

    pub fn details(&self) -> String {
        self.buckets
            .iter()
            .flat_map(|b| {
                let name = b
                    .limit_name
                    .as_deref()
                    .or(b.limit_id.as_deref())
                    .unwrap_or("Codex");
                [b.primary.as_ref(), b.secondary.as_ref()]
                    .into_iter()
                    .flatten()
                    .map(move |w| {
                        format!(
                            "{name} · {}\n{}\n重置 {}",
                            w.label(),
                            w.summary(),
                            w.reset()
                        )
                    })
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FetchError {
    MissingCli,
    Unauthorized,
    Unsupported,
    Timeout,
    Connection,
    Protocol,
}

impl FetchError {
    pub fn message(self) -> &'static str {
        match self {
            Self::MissingCli => "未找到 Codex，请安装 Codex CLI 或桌面应用",
            Self::Unauthorized => "请先在 Codex 中使用 ChatGPT 账户登录",
            Self::Unsupported => "当前账户未提供 Codex 订阅额度",
            Self::Timeout => "读取超时，请稍后刷新",
            Self::Connection => "无法读取额度，请检查网络后刷新",
            Self::Protocol => "Codex 接口不可用，请更新 Codex 后重试",
        }
    }
}

#[derive(Default)]
pub struct UsageState {
    pub snapshot: Option<Snapshot>,
    pub error: Option<FetchError>,
}

impl UsageState {
    pub fn apply(&mut self, result: Result<Snapshot, FetchError>) {
        match result {
            Ok(snapshot) => {
                self.snapshot = Some(snapshot);
                self.error = None;
            }
            Err(error) => {
                if matches!(
                    error,
                    FetchError::Unauthorized | FetchError::Unsupported | FetchError::MissingCli
                ) {
                    self.snapshot = None;
                }
                self.error = Some(error);
            }
        }
    }

    pub fn status(&self) -> String {
        if let Some(error) = self.error {
            return format!(
                "{}{}",
                error.message(),
                if self.snapshot.is_some() {
                    "（保留上次数据）"
                } else {
                    ""
                }
            );
        }
        self.snapshot
            .as_ref()
            .map(|s| {
                let time = DateTime::from_timestamp_secs(s.collected_at)
                    .unwrap()
                    .with_timezone(&Local);
                format!("真实额度 · 更新于 {}", time.format("%H:%M"))
            })
            .unwrap_or_else(|| "正在读取 Codex 额度…".into())
    }
}

pub fn parse_snapshot(value: Value) -> Result<Snapshot, FetchError> {
    let mut buckets = Vec::new();
    if let Some(map) = value.get("rateLimitsByLimitId").and_then(Value::as_object) {
        for (id, value) in map {
            let mut bucket: Bucket =
                serde_json::from_value(value.clone()).map_err(|_| FetchError::Protocol)?;
            bucket.limit_id = Some(id.clone());
            buckets.push(bucket);
        }
    }
    if !buckets
        .iter()
        .any(|b| b.limit_id.as_deref() == Some("codex"))
    {
        if let Some(legacy) = value.get("rateLimits").filter(|v| !v.is_null()) {
            let bucket: Bucket =
                serde_json::from_value(legacy.clone()).map_err(|_| FetchError::Protocol)?;
            if bucket.limit_id.is_none() || bucket.limit_id.as_deref() == Some("codex") {
                buckets.insert(0, bucket);
            }
        }
    }
    if buckets
        .iter()
        .all(|b| b.primary.is_none() && b.secondary.is_none())
    {
        return Err(FetchError::Unsupported);
    }
    Ok(Snapshot {
        collected_at: Local::now().timestamp(),
        buckets,
    })
}

fn is_executable(path: &Path) -> bool {
    path.is_file()
        && path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
}

pub fn discover_cli() -> Result<PathBuf, FetchError> {
    if let Some(path) = std::env::var_os("WACU_CODEX_PATH") {
        let path = PathBuf::from(path);
        return is_executable(&path)
            .then_some(path)
            .ok_or(FetchError::MissingCli);
    }
    if let Some(paths) = std::env::var_os("PATH") {
        for directory in std::env::split_paths(&paths) {
            let path = directory.join("codex.exe");
            if is_executable(&path) {
                return Ok(path);
            }
            // npm's Windows launcher is a .cmd; invoke its native binary directly.
            for platform in [
                "@openai/codex-win32-x64",
                "@openai/codex-win32-arm64",
                "@openai/codex",
            ] {
                for target in ["x86_64-pc-windows-msvc", "aarch64-pc-windows-msvc"] {
                    let path = directory
                        .join("node_modules")
                        .join(platform)
                        .join("vendor")
                        .join(target)
                        .join("codex/codex.exe");
                    if is_executable(&path) {
                        return Ok(path);
                    }
                }
            }
        }
    }
    if let Some(base) = std::env::var_os("LOCALAPPDATA") {
        let directory = PathBuf::from(base).join("OpenAI/Codex/bin");
        let mut candidates = std::fs::read_dir(directory)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path().join("codex.exe"))
            .filter(|p| is_executable(p))
            .collect::<Vec<_>>();
        candidates.sort_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok());
        if let Some(path) = candidates.pop() {
            return Ok(path);
        }
    }
    Err(FetchError::MissingCli)
}

struct Session {
    child: Child,
    input: ChildStdin,
    output: Receiver<Result<String, std::io::Error>>,
    deadline: Instant,
    #[cfg(windows)]
    _job: Option<ProcessJob>,
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Session {
    fn send(&mut self, value: Value) -> Result<(), FetchError> {
        writeln!(self.input, "{value}")
            .and_then(|_| self.input.flush())
            .map_err(|_| FetchError::Connection)
    }

    fn request(&mut self, id: u64, method: &str, params: Value) -> Result<Value, FetchError> {
        self.send(json!({"id":id,"method":method,"params":params}))?;
        loop {
            let timeout = self.deadline.saturating_duration_since(Instant::now());
            let line = self
                .output
                .recv_timeout(timeout)
                .map_err(|error| match error {
                    mpsc::RecvTimeoutError::Timeout => FetchError::Timeout,
                    _ => FetchError::Connection,
                })?
                .map_err(|_| FetchError::Connection)?;
            let response: Value = serde_json::from_str(&line).map_err(|_| FetchError::Protocol)?;
            if response.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            if let Some(error) = response.get("error") {
                // Never expose arbitrary upstream text (which can contain account data).
                let message = error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_lowercase();
                return Err(
                    if message.contains("401")
                        || message.contains("unauthorized")
                        || message.contains("not logged")
                    {
                        FetchError::Unauthorized
                    } else if error.get("code").and_then(Value::as_i64) == Some(-32601) {
                        FetchError::Protocol
                    } else {
                        FetchError::Connection
                    },
                );
            }
            return response.get("result").cloned().ok_or(FetchError::Protocol);
        }
    }
}

pub fn fetch() -> Result<Snapshot, FetchError> {
    let mut command = Command::new(discover_cli()?);
    command
        .args(["app-server", "--listen", "stdio://"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn().map_err(|_| FetchError::Connection)?;
    let input = child.stdin.take().ok_or(FetchError::Protocol)?;
    let stdout = child.stdout.take().ok_or(FetchError::Protocol)?;
    let (sender, output) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            if sender.send(line).is_err() {
                break;
            }
        }
    });
    let mut session = Session {
        child,
        input,
        output,
        deadline: Instant::now() + TIMEOUT,
        #[cfg(windows)]
        _job: None,
    };
    #[cfg(windows)]
    {
        session._job =
            Some(ProcessJob::attach(&session.child).map_err(|_| FetchError::Connection)?);
    }
    session.request(
        1,
        "initialize",
        json!({"clientInfo":{"name":"wacu","title":"Wacu","version":env!("CARGO_PKG_VERSION")}}),
    )?;
    session.send(json!({"method":"initialized","params":{}}))?;
    let account = session.request(2, "account/read", json!({"refreshToken":false}))?;
    match account.pointer("/account/type").and_then(Value::as_str) {
        Some("chatgpt") => {}
        None => return Err(FetchError::Unauthorized),
        _ => return Err(FetchError::Unsupported),
    }
    parse_snapshot(session.request(3, "account/rateLimits/read", Value::Null)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multi_bucket_wins_and_keeps_other_model_limits() {
        let snapshot = parse_snapshot(json!({"rateLimits":{"primary":{"usedPercent":99}},
            "rateLimitsByLimitId":{"codex":{"primary":{"usedPercent":12.5,"windowDurationMins":300,"resetsAt":1730947200},
            "secondary":{"usedPercent":40,"windowDurationMins":10080}}, "other":{"primary":{"usedPercent":90}}}})).unwrap();
        let window = snapshot.main_window().unwrap();
        assert_eq!(window.percent(), Some(12.5));
        assert_eq!(window.label(), "5小时额度");
        assert!(window.summary().contains("剩余 87.5%"));
        assert_eq!(snapshot.buckets.len(), 2);
        assert!(snapshot.details().contains("每周额度"));
    }

    #[test]
    fn missing_usage_is_unknown_and_secondary_can_supply_ring() {
        let snapshot = parse_snapshot(json!({"rateLimits":{"primary":{"usedPercent":null},
            "secondary":{"usedPercent":110,"windowDurationMins":10080}}}))
        .unwrap();
        assert_eq!(
            snapshot
                .main_bucket()
                .unwrap()
                .primary
                .as_ref()
                .unwrap()
                .percent(),
            None
        );
        assert_eq!(snapshot.main_window().unwrap().percent(), Some(100.0));
        assert_eq!(snapshot.main_window().unwrap().reset(), "未提供重置时间");
        assert!(parse_snapshot(json!({"rateLimits":null})).is_err());
        assert!(parse_snapshot(json!({"rateLimits":{"primary":{"usedPercent":"bad"}}})).is_err());
    }

    #[test]
    fn another_bucket_does_not_impersonate_codex() {
        let snapshot =
            parse_snapshot(json!({"rateLimitsByLimitId":{"other":{"primary":{"usedPercent":25}}}}))
                .unwrap();
        assert!(snapshot.main_window().is_none());
    }

    #[test]
    fn connection_failure_keeps_stale_data_but_logout_clears_it() {
        let mut state = UsageState::default();
        state.apply(parse_snapshot(
            json!({"rateLimits":{"primary":{"usedPercent":0}}}),
        ));
        state.apply(Err(FetchError::Timeout));
        assert!(state.snapshot.is_some());
        assert!(state.status().contains("保留上次数据"));
        state.apply(Err(FetchError::Unauthorized));
        assert!(state.snapshot.is_none());
    }
}
