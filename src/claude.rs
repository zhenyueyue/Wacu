//! Read-only Claude Code subscription usage. Tokens never enter UI snapshots.
use crate::child_process::ProcessJob;
use crate::quota::QuotaWindow;
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc,
    thread,
    time::Instant,
};
use std::{path::PathBuf, time::Duration};

pub const REFRESH_INTERVAL: Duration = Duration::from_secs(300);

#[derive(Clone, Debug, Serialize)]
pub struct Window {
    pub id: String,
    pub label: String,
    pub quota: QuotaWindow,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ExtraUsage {
    pub is_enabled: Option<bool>,
    pub monthly_limit: Option<f64>,
    pub used_credits: Option<f64>,
    pub utilization: Option<f64>,
    pub currency: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Snapshot {
    pub collected_at: i64,
    #[serde(skip)]
    pub account: Option<Account>,
    pub windows: Vec<Window>,
    pub extra_usage: Option<ExtraUsage>,
}

impl Snapshot {
    pub fn main_window(&self) -> Option<&QuotaWindow> {
        ["five_hour", "seven_day"].into_iter().find_map(|id| {
            self.windows
                .iter()
                .find(|w| w.id == id && w.quota.percent().is_some())
                .map(|w| &w.quota)
        })
    }

    pub fn details(&self) -> String {
        let mut details = self
            .windows
            .iter()
            .map(|w| {
                format!(
                    "{}\n{}\n重置 {}",
                    w.label,
                    w.quota.summary(),
                    w.quota.reset()
                )
            })
            .collect::<Vec<_>>();
        if let Some(extra) = &self.extra_usage {
            if extra.is_enabled == Some(true) {
                let percent = QuotaWindow {
                    used_percent: extra.utilization,
                    window_duration_mins: None,
                    resets_at: None,
                };
                details.push(format!("额外用量\n{}", percent.summary()));
            }
        }
        let source = self
            .account
            .as_ref()
            .map(|account| {
                format!(
                    "来源：Claude Code CLI\n账户：{}\n范围：该账户共享订阅额度",
                    account.label()
                )
            })
            .unwrap_or_else(|| "来源：Claude Code CLI\n账户：未验证（离线样例）".into());
        format!("{source}\n\n{}", details.join("\n\n"))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FetchError {
    AccountChanged,
    MissingCli,
    Unauthorized,
    Unsupported,
    Timeout,
    Connection,
    Protocol,
    RateLimited(u64),
}

impl FetchError {
    pub fn message(self) -> &'static str {
        match self {
            Self::AccountChanged => "Claude CLI 账户已切换，旧额度已清空，请刷新",
            Self::MissingCli => "未找到 Claude Code，请安装 CLI 或指定程序路径",
            Self::Unauthorized => "Claude 登录已过期，请在 Claude Code 中重新登录",
            Self::Unsupported => "当前账户未提供 Claude 订阅额度",
            Self::Timeout => "Claude 额度读取超时，请稍后刷新",
            Self::Connection => "无法读取 Claude 额度，请检查网络或代理",
            Self::Protocol => "Claude 本地接口返回格式异常",
            Self::RateLimited(_) => "Claude 额度查询被限流，稍后自动重试",
        }
    }

    pub fn retry_after(self) -> Duration {
        match self {
            Self::RateLimited(seconds) => Duration::from_secs(seconds.max(900)),
            _ => REFRESH_INTERVAL,
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
                    FetchError::MissingCli
                        | FetchError::Unauthorized
                        | FetchError::Unsupported
                        | FetchError::AccountChanged
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
            .unwrap_or_else(|| "正在读取 Claude 额度…".into())
    }
}

pub fn parse_snapshot(value: Value) -> Result<Snapshot, FetchError> {
    let object = value.as_object().ok_or(FetchError::Protocol)?;
    let mut windows = Vec::new();
    for (key, value) in object {
        if value.is_null() || !(key == "five_hour" || key.starts_with("seven_day")) {
            continue;
        }
        #[derive(Deserialize)]
        struct RawWindow {
            utilization: Option<f64>,
            resets_at: Option<String>,
        }
        let raw: RawWindow =
            serde_json::from_value(value.clone()).map_err(|_| FetchError::Protocol)?;
        let label = match key.as_str() {
            "five_hour" => "5小时额度".into(),
            "seven_day" => "每周额度（全部模型）".into(),
            "seven_day_sonnet" => "Sonnet 每周额度".into(),
            "seven_day_opus" => "Opus 每周额度".into(),
            _ => format!("每周额度 · {key}"),
        };
        windows.push(Window {
            id: key.clone(),
            label,
            quota: QuotaWindow {
                used_percent: raw.utilization,
                window_duration_mins: Some(if key == "five_hour" { 300 } else { 10080 }),
                resets_at: raw
                    .resets_at
                    .and_then(|date| DateTime::parse_from_rfc3339(&date).ok())
                    .map(|date| date.timestamp()),
            },
        });
    }
    if let Some(limits) = object.get("limits").and_then(Value::as_array) {
        for entry in limits.iter().filter(|entry| {
            entry.get("kind").and_then(Value::as_str) == Some("weekly_scoped")
                && entry.get("group").and_then(Value::as_str) == Some("weekly")
                && entry.pointer("/scope/surface").is_none_or(Value::is_null)
        }) {
            let name = entry
                .pointer("/scope/model/display_name")
                .or_else(|| entry.pointer("/scope/model/id"))
                .and_then(Value::as_str)
                .unwrap_or("模型");
            let percent = entry.get("percent").and_then(Value::as_f64);
            if percent.is_none() {
                continue;
            }
            let id = format!("seven_day_{}", name.trim().to_lowercase());
            windows.retain(|window| window.id != id);
            windows.push(Window {
                id,
                label: format!("{name} 每周额度"),
                quota: QuotaWindow {
                    used_percent: percent,
                    window_duration_mins: Some(10080),
                    resets_at: entry
                        .get("resets_at")
                        .and_then(Value::as_str)
                        .and_then(|date| DateTime::parse_from_rfc3339(date).ok())
                        .map(|date| date.timestamp()),
                },
            });
        }
    }
    if windows.is_empty() {
        return Err(FetchError::Unsupported);
    }
    windows.sort_by_key(|w| {
        (
            match w.id.as_str() {
                "five_hour" => 0,
                "seven_day" => 1,
                _ => 2,
            },
            w.id.clone(),
        )
    });
    let extra_usage = object
        .get("extra_usage")
        .filter(|v| !v.is_null())
        .map(|v| serde_json::from_value(v.clone()).map_err(|_| FetchError::Protocol))
        .transpose()?;
    Ok(Snapshot {
        collected_at: Local::now().timestamp(),
        account: None,
        windows,
        extra_usage,
    })
}

const REQUEST_ID: &str = "wacu-claude-usage";

fn discover_cli() -> Result<PathBuf, FetchError> {
    if let Some(path) = std::env::var_os("WACU_CLAUDE_PATH") {
        let path = PathBuf::from(path);
        return path.is_file().then_some(path).ok_or(FetchError::MissingCli);
    }
    if let Some(paths) = std::env::var_os("PATH") {
        for directory in std::env::split_paths(&paths) {
            for relative in [
                "claude.exe",
                "node_modules/@anthropic-ai/claude-code/bin/claude.exe",
            ] {
                let path = directory.join(relative);
                if path.is_file() {
                    return Ok(path);
                }
            }
        }
    }
    if let Some(home) = std::env::var_os("USERPROFILE") {
        for relative in [".local/bin/claude.exe", ".claude/local/claude.exe"] {
            let path = PathBuf::from(&home).join(relative);
            if path.is_file() {
                return Ok(path);
            }
        }
    }
    Err(FetchError::MissingCli)
}

struct Engine {
    child: Child,
    input: ChildStdin,
    lines: mpsc::Receiver<Result<String, std::io::Error>>,
    _job: ProcessJob,
}

impl Drop for Engine {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Engine {
    fn start(executable: &PathBuf, args: &[&str]) -> Result<Self, FetchError> {
        let base = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("Wacu/claude-engine");
        std::fs::create_dir_all(&base).map_err(|_| FetchError::Connection)?;
        let mut command = Command::new(executable);
        command
            .args(args)
            .current_dir(base)
            .env("CLAUDE_CODE_ENTRYPOINT", "wacu")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
        let mut child = command.spawn().map_err(|_| FetchError::Connection)?;
        let job = match ProcessJob::attach(&child) {
            Ok(job) => job,
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(FetchError::Connection);
            }
        };
        let input = child.stdin.take().ok_or(FetchError::Protocol)?;
        let stdout = child.stdout.take().ok_or(FetchError::Protocol)?;
        let (sender, lines) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            input,
            lines,
            _job: job,
        })
    }

    fn line(&self, deadline: Instant) -> Result<Option<String>, FetchError> {
        match self
            .lines
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        {
            Ok(Ok(line)) if line.len() <= 8 * 1024 * 1024 => Ok(Some(line)),
            Ok(Ok(_)) => Err(FetchError::Protocol),
            Ok(Err(_)) => Err(FetchError::Connection),
            Err(mpsc::RecvTimeoutError::Disconnected) => Ok(None),
            Err(_) => Err(FetchError::Timeout),
        }
    }
}

fn parse_control(value: Value) -> Result<Option<Snapshot>, FetchError> {
    if value.get("type").and_then(Value::as_str) != Some("control_response")
        || value
            .pointer("/response/request_id")
            .and_then(Value::as_str)
            != Some(REQUEST_ID)
    {
        return Ok(None);
    }
    let envelope = value.get("response").ok_or(FetchError::Protocol)?;
    if envelope.get("subtype").and_then(Value::as_str) == Some("error") {
        let message = envelope
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_lowercase();
        return Err(
            if message.contains("429") || message.contains("rate limit") {
                FetchError::RateLimited(900)
            } else if message.contains("401")
                || message.contains("not logged")
                || message.contains("unauthorized")
            {
                FetchError::Unauthorized
            } else {
                FetchError::Protocol
            },
        );
    }
    let payload = envelope.get("response").ok_or(FetchError::Protocol)?;
    if payload
        .get("rate_limits_available")
        .and_then(Value::as_bool)
        != Some(true)
    {
        return Err(FetchError::Unsupported);
    }
    parse_snapshot(
        payload
            .get("rate_limits")
            .cloned()
            .ok_or(FetchError::Protocol)?,
    )
    .map(Some)
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    logged_in: bool,
    auth_method: Option<String>,
    api_provider: Option<String>,
    email: Option<String>,
    org_id: Option<String>,
    subscription_type: Option<String>,
}

impl Account {
    fn same_identity(&self, other: &Self) -> bool {
        self.email == other.email
            && self.org_id == other.org_id
            && self.auth_method == other.auth_method
            && self.api_provider == other.api_provider
    }

    fn label(&self) -> String {
        let email = self.email.as_deref().unwrap_or("");
        let masked = email
            .split_once('@')
            .map(|(name, domain)| format!("{}***@{domain}", name.chars().next().unwrap_or('*')))
            .unwrap_or_else(|| "未提供邮箱".into());
        format!(
            "{masked} · {}",
            self.subscription_type.as_deref().unwrap_or("未提供套餐")
        )
    }

    fn validate(&self) -> Result<(), FetchError> {
        if !self.logged_in {
            return Err(FetchError::Unauthorized);
        }
        if self.auth_method.as_deref() != Some("claude.ai")
            || self.api_provider.as_deref() != Some("firstParty")
        {
            return Err(FetchError::Unsupported);
        }
        if self.email.as_deref().is_none_or(str::is_empty)
            || self.org_id.as_deref().is_none_or(str::is_empty)
        {
            return Err(FetchError::Protocol);
        }
        Ok(())
    }
}

fn read_account(executable: &PathBuf, deadline: Instant) -> Result<Account, FetchError> {
    let engine = Engine::start(executable, &["auth", "status", "--json"])?;
    let mut text = String::new();
    while let Some(line) = engine.line(deadline)? {
        if text.len() + line.len() > 65536 {
            return Err(FetchError::Protocol);
        }
        text.push_str(&line);
    }
    serde_json::from_str(&text).map_err(|_| FetchError::Protocol)
}

pub fn fetch() -> Result<Snapshot, FetchError> {
    fetch_for(None)
}

pub fn fetch_for(previous: Option<&Account>) -> Result<Snapshot, FetchError> {
    let executable = discover_cli()?;
    let deadline = Instant::now() + Duration::from_secs(40);
    let before = read_account(&executable, deadline)?;
    before.validate()?;
    let changed = previous.is_some_and(|previous| !previous.same_identity(&before));
    let result = fetch_usage(&executable, deadline);
    let after = read_account(&executable, deadline);
    bind_account(result, before, after, changed)
}

fn bind_account(
    result: Result<Snapshot, FetchError>,
    before: Account,
    after: Result<Account, FetchError>,
    changed: bool,
) -> Result<Snapshot, FetchError> {
    let after = match after {
        Ok(after) => after,
        Err(error) => {
            return Err(if changed {
                FetchError::AccountChanged
            } else {
                error
            });
        }
    };
    if !before.same_identity(&after) || before.logged_in != after.logged_in {
        return Err(FetchError::AccountChanged);
    }
    if changed && result.is_err() {
        return Err(FetchError::AccountChanged);
    }
    result.map(|mut snapshot| {
        snapshot.account = Some(before);
        snapshot
    })
}

fn fetch_usage(executable: &PathBuf, deadline: Instant) -> Result<Snapshot, FetchError> {
    let mut engine = Engine::start(
        executable,
        &[
            "-p",
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
            "--verbose",
            "--safe-mode",
            "--settings",
            "{\"disableAllHooks\":true}",
            "--no-session-persistence",
            "--tools",
            "",
            "--strict-mcp-config",
            "--mcp-config",
            "{\"mcpServers\":{}}",
        ],
    )?;
    let request = serde_json::json!({"type":"control_request", "request_id":REQUEST_ID,
        "request":{"subtype":"get_usage", "skip_behaviors":true}});
    writeln!(engine.input, "{request}")
        .and_then(|_| engine.input.flush())
        .map_err(|_| FetchError::Connection)?;
    while let Some(line) = engine.line(deadline)? {
        let value: Value = serde_json::from_str(&line).map_err(|_| FetchError::Protocol)?;
        match parse_control(value) {
            Ok(Some(snapshot)) => return Ok(snapshot),
            Ok(None) => {}
            Err(error) => return Err(error),
        }
    }
    Err(FetchError::Connection)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn account(email: &str, org: &str) -> Account {
        serde_json::from_value(
            json!({"loggedIn":true,"authMethod":"claude.ai","apiProvider":"firstParty",
            "email":email,"orgId":org,"subscriptionType":"pro"}),
        )
        .unwrap()
    }

    #[test]
    fn identity_binding_rejects_account_and_organization_switches() {
        let before = account("alice@example.test", "org-a");
        let result = || parse_snapshot(json!({"five_hour":{"utilization":25}}));
        for after in [
            account("bob@example.test", "org-a"),
            account("alice@example.test", "org-b"),
        ] {
            assert_eq!(
                bind_account(result(), before.clone(), Ok(after), false).unwrap_err(),
                FetchError::AccountChanged
            );
        }
        assert_eq!(
            bind_account(
                Err(FetchError::Connection),
                before.clone(),
                Ok(before.clone()),
                true
            )
            .unwrap_err(),
            FetchError::AccountChanged
        );
        let snapshot = bind_account(result(), before.clone(), Ok(before), false).unwrap();
        assert!(snapshot.details().contains("a***@example.test"));
        assert!(!snapshot.details().contains("alice@"));
        let diagnostic = serde_json::to_string(&snapshot).unwrap();
        assert!(!diagnostic.contains("org-a"));
        assert!(!diagnostic.contains("alice@"));
        let mut state = UsageState::default();
        state.apply(Ok(snapshot));
        state.apply(Err(FetchError::AccountChanged));
        assert!(state.snapshot.is_none());
    }

    #[test]
    fn api_key_and_unidentified_accounts_do_not_claim_subscription_quota() {
        let mut value = account("alice@example.test", "org-a");
        assert!(value.validate().is_ok());
        value.auth_method = Some("api_key".into());
        assert_eq!(value.validate(), Err(FetchError::Unsupported));
        value.auth_method = Some("claude.ai".into());
        value.org_id = None;
        assert_eq!(value.validate(), Err(FetchError::Protocol));
    }

    #[test]
    fn model_weekly_limits_replace_legacy_without_merging_surface_limits() {
        let snapshot = parse_snapshot(json!({"five_hour":{"utilization":10},"seven_day_sonnet":{"utilization":20},
            "limits":[
                {"kind":"weekly_scoped","group":"weekly","percent":35,"scope":{"model":{"display_name":"Sonnet"}}},
                {"kind":"weekly_scoped","group":"weekly","percent":99,"scope":{"surface":"claude_code","model":{"display_name":"Sonnet"}}},
                {"kind":"weekly_scoped","group":"daily","percent":50,"scope":{"model":{"display_name":"Opus"}}}
            ]})).unwrap();
        assert_eq!(snapshot.windows.len(), 2);
        assert_eq!(snapshot.windows[1].quota.percent(), Some(35.0));
        assert_eq!(snapshot.main_window().unwrap().percent(), Some(10.0));
    }

    #[test]
    fn control_protocol_filters_messages_and_parses_only_quota() {
        assert!(parse_control(json!({"type":"system"})).unwrap().is_none());
        let snapshot = parse_control(json!({"type":"control_response","response":{"request_id":REQUEST_ID,"subtype":"success",
            "response":{"rate_limits_available":true,"rate_limits":{"five_hour":{"utilization":12.5}},"private_field":"ignored"}}})).unwrap().unwrap();
        assert_eq!(snapshot.main_window().unwrap().percent(), Some(12.5));
        assert!(
            !serde_json::to_string(&snapshot)
                .unwrap()
                .contains("private_field")
        );
        assert_eq!(parse_control(json!({"type":"control_response","response":{"request_id":REQUEST_ID,"subtype":"success",
            "response":{"rate_limits_available":false}}})).unwrap_err(), FetchError::Unsupported);
        assert_eq!(parse_control(json!({"type":"control_response","response":{"request_id":REQUEST_ID,"subtype":"error","error":"429 Rate limited"}})).unwrap_err(), FetchError::RateLimited(900));
    }

    #[test]
    fn real_percentages_and_model_windows_are_not_multiplied() {
        let snapshot = parse_snapshot(
            json!({"five_hour":{"utilization":12.5,"resets_at":"2026-10-12T04:00:00+08:00"},
            "seven_day":{"utilization":0,"resets_at":null}, "seven_day_opus":null,
            "seven_day_sonnet":{"utilization":35}, "seven_day_future":{"utilization":50}}),
        )
        .unwrap();
        assert_eq!(snapshot.main_window().unwrap().percent(), Some(12.5));
        assert_eq!(snapshot.main_window().unwrap().resets_at, Some(1791748800));
        assert_eq!(snapshot.windows.len(), 4);
        assert!(snapshot.details().contains("Sonnet"));
    }

    #[test]
    fn nulls_and_empty_usage_remain_unknown() {
        let snapshot = parse_snapshot(json!({"five_hour":{"utilization":null},"seven_day":{"utilization":25,"resets_at":"bad"}})).unwrap();
        assert_eq!(snapshot.windows[0].quota.percent(), None);
        assert_eq!(snapshot.main_window().unwrap().percent(), Some(25.0));
        assert_eq!(snapshot.main_window().unwrap().reset(), "未提供重置时间");
        assert_eq!(
            parse_snapshot(json!({})).unwrap_err(),
            FetchError::Unsupported
        );
        assert_eq!(
            parse_snapshot(json!({"five_hour":{"utilization":"bad"}})).unwrap_err(),
            FetchError::Protocol
        );
    }

    #[test]
    fn stale_snapshot_survives_network_errors_and_clears_on_logout() {
        let mut state = UsageState::default();
        state.apply(parse_snapshot(json!({"five_hour":{"utilization":30}})));
        state.apply(Err(FetchError::RateLimited(900)));
        assert!(state.status().contains("保留上次数据"));
        state.apply(Err(FetchError::Unauthorized));
        assert!(state.snapshot.is_none());
    }
}
