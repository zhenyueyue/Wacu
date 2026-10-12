//! Local Claude Code lifecycle signals. Never stores prompts, tools or credentials.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeSet, HashMap},
    fs::{self, OpenOptions},
    hash::{DefaultHasher, Hash, Hasher},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub const HOOK_ARG: &str = "--claude-activity-hook";
const EVENTS: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PermissionRequest",
    "PostToolUse",
    "PostToolUseFailure",
    "Notification",
    "Stop",
    "StopFailure",
    "SessionEnd",
    "SubagentStart",
    "SubagentStop",
];

#[derive(Deserialize)]
struct Input {
    session_id: String,
    hook_event_name: String,
    #[serde(default)]
    notification_type: String,
    #[serde(default)]
    agent_id: String,
    #[serde(default)]
    source: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
struct Owner {
    pid: u32,
    birth: u64,
}

#[derive(Default, Deserialize, Serialize)]
struct Session {
    owner: Option<Owner>,
    working: bool,
    agents: BTreeSet<String>,
}

impl Session {
    fn set_working(&mut self, input: &Input, working: bool) {
        if input.agent_id.is_empty() {
            self.working = working;
        } else if working {
            self.agents.insert(key(&input.agent_id));
        } else {
            self.agents.remove(&key(&input.agent_id));
        }
    }
    fn apply(&mut self, input: &Input) {
        match input.hook_event_name.as_str() {
            "SessionStart" if input.source == "compact" || !input.agent_id.is_empty() => {}
            "SessionEnd" if !input.agent_id.is_empty() => self.set_working(input, false),
            "SessionStart" | "SessionEnd" => {
                self.working = false;
                self.agents.clear();
            }
            "UserPromptSubmit" | "PreToolUse" | "PostToolUse" | "PostToolUseFailure" => {
                self.set_working(input, true)
            }
            "PermissionRequest" | "Stop" | "StopFailure" => self.set_working(input, false),
            "Notification"
                if matches!(
                    input.notification_type.as_str(),
                    "permission_prompt"
                        | "idle_prompt"
                        | "elicitation_dialog"
                        | "agent_needs_input"
                ) =>
            {
                self.set_working(input, false)
            }
            "SubagentStart" if !input.agent_id.is_empty() => {
                self.agents.insert(key(&input.agent_id));
            }
            "SubagentStop" => {
                self.agents.remove(&key(&input.agent_id));
            }
            _ => {}
        }
    }
    fn active(&self) -> bool {
        self.working || !self.agents.is_empty()
    }
}

fn key(value: &str) -> String {
    let mut hash = DefaultHasher::new();
    value.hash(&mut hash);
    format!("{:016x}", hash.finish())
}

pub fn directory() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("Wacu/claude-activity")
}

fn process_owner(pid: u32) -> Option<Owner> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, FILETIME},
        System::Threading::{GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
    };
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return None;
        }
        let mut times: [FILETIME; 4] = std::mem::zeroed();
        let success = GetProcessTimes(
            handle,
            &mut times[0],
            &mut times[1],
            &mut times[2],
            &mut times[3],
        );
        CloseHandle(handle);
        if success == 0 || times[1].dwLowDateTime != 0 || times[1].dwHighDateTime != 0 {
            return None;
        }
        Some(Owner {
            pid,
            birth: (times[0].dwHighDateTime as u64) << 32 | times[0].dwLowDateTime as u64,
        })
    }
}

fn hook_owner() -> Option<Owner> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, INVALID_HANDLE_VALUE},
        System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
            TH32CS_SNAPPROCESS,
        },
    };
    let mut parents = HashMap::new();
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return None;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut valid = Process32FirstW(snapshot, &mut entry);
        while valid != 0 {
            let end = entry
                .szExeFile
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(entry.szExeFile.len());
            parents.insert(
                entry.th32ProcessID,
                (
                    entry.th32ParentProcessID,
                    String::from_utf16_lossy(&entry.szExeFile[..end]),
                ),
            );
            valid = Process32NextW(snapshot, &mut entry);
        }
        CloseHandle(snapshot);
    }
    let mut pid = parents.get(&std::process::id())?.0;
    let mut owner = None;
    for _ in 0..32 {
        let Some((parent, name)) = parents.get(&pid) else {
            break;
        };
        if name.eq_ignore_ascii_case("wacu.exe") {
            return None;
        }
        // Exec-form hooks are direct children of the local Code engine.
        // Skip wrappers so legacy shell-form hooks also keep a live owner.
        if owner.is_none()
            && ![
                "bash.exe",
                "sh.exe",
                "powershell.exe",
                "pwsh.exe",
                "cmd.exe",
            ]
            .iter()
            .any(|shell| name.eq_ignore_ascii_case(shell))
        {
            owner = process_owner(pid);
        }
        if *parent == 0 || *parent == pid {
            break;
        }
        pid = *parent;
    }
    owner
}

struct Lock(PathBuf);
impl Drop for Lock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn record(root: &Path, input: &Input, owner: Owner) -> std::io::Result<()> {
    if input.session_id.is_empty()
        || input.session_id.len() > 256
        || !EVENTS.contains(&input.hook_event_name.as_str())
    {
        return Ok(());
    }
    fs::create_dir_all(root)?;
    let path = root.join(format!("{}.json", key(&input.session_id)));
    let lock_path = path.with_extension("lock");
    let mut locked = false;
    for _ in 0..30 {
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
        {
            Ok(_) => {
                locked = true;
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                if fs::metadata(&lock_path)
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| t.elapsed().ok())
                    .is_some_and(|age| age > Duration::from_secs(10))
                {
                    let _ = fs::remove_file(&lock_path);
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(error) => return Err(error),
        }
    }
    if !locked {
        return Err(std::io::Error::new(
            std::io::ErrorKind::WouldBlock,
            "activity lock busy",
        ));
    }
    let _lock = Lock(lock_path);
    let mut session: Session = fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    if session.owner != Some(owner) {
        session = Session::default();
    }
    session.owner = Some(owner);
    session.apply(input);
    if input.hook_event_name == "SessionEnd" && input.agent_id.is_empty() {
        let _ = fs::remove_file(path);
        return Ok(());
    }
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    fs::write(&temporary, serde_json::to_vec(&session)?)?;
    fs::rename(temporary, path)
}

pub fn handle_hook() {
    if std::env::var_os("WACU_QUOTA_READER").is_some() {
        return;
    }
    let mut bytes = Vec::new();
    if std::io::stdin()
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .is_err()
        || bytes.len() > 1024 * 1024
    {
        return;
    }
    let Ok(input) = serde_json::from_slice::<Input>(&bytes) else {
        return;
    };
    let Some(owner) = hook_owner() else {
        return;
    };
    let _ = record(&argument_directory(), &input, owner);
}

pub fn argument_directory() -> PathBuf {
    let args: Vec<_> = std::env::args_os().collect();
    args.iter()
        .position(|a| a == "--activity-dir")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
        .unwrap_or_else(directory)
}

pub fn active_count() -> usize {
    count_active(&argument_directory())
}

fn count_active(root: &Path) -> usize {
    let Ok(entries) = fs::read_dir(root) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.path().extension().is_some_and(|s| s == "json")
                && entry.file_name() != "installed.json"
        })
        .filter_map(|entry| {
            let session: Session = serde_json::from_slice(&fs::read(entry.path()).ok()?).ok()?;
            let owner = session.owner?;
            if process_owner(owner.pid) != Some(owner) {
                let _ = fs::remove_file(entry.path());
                return None;
            }
            Some(session.active())
        })
        .filter(|active| *active)
        .count()
}

pub struct Monitor {
    active: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
}
impl Monitor {
    pub fn new() -> Self {
        let active = Arc::new(AtomicUsize::new(if connected() {
            count_active(&directory())
        } else {
            0
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let output = active.clone();
        let stopping = stop.clone();
        std::thread::spawn(move || {
            while !stopping.load(Ordering::Relaxed) {
                output.store(
                    if connected() {
                        count_active(&directory())
                    } else {
                        0
                    },
                    Ordering::Relaxed,
                );
                std::thread::sleep(Duration::from_secs(2));
            }
        });
        Self { active, stop }
    }
    pub fn active(&self) -> bool {
        self.active.load(Ordering::Relaxed) > 0
    }
}
impl Drop for Monitor {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn settings_path() -> PathBuf {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("USERPROFILE").unwrap_or_default()).join(".claude")
        })
        .join("settings.json")
}

fn owned(handler: &Value) -> bool {
    handler
        .get("args")
        .and_then(Value::as_array)
        .is_some_and(|args| args.iter().any(|v| v.as_str() == Some(HOOK_ARG)))
        || handler
            .get("command")
            .and_then(Value::as_str)
            .is_some_and(|c| c.contains(HOOK_ARG))
}

fn configure(mut settings: Value, exe: &Path, install: bool) -> Result<Value, String> {
    let object = settings
        .as_object_mut()
        .ok_or("Claude settings must be an object")?;
    if install && object.get("disableAllHooks").and_then(Value::as_bool) == Some(true) {
        return Err("Claude 已禁用全部 hooks，请先启用后再连接工作状态".into());
    }
    let hooks = object
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or("Invalid Claude hooks configuration")?;
    for event in EVENTS {
        let groups = hooks
            .entry(*event)
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or("Invalid Claude hook event configuration")?;
        groups.retain_mut(|group| {
            if let Some(handlers) = group.get_mut("hooks").and_then(Value::as_array_mut) {
                let had_owned = handlers.iter().any(owned);
                handlers.retain(|h| !owned(h));
                return !had_owned || !handlers.is_empty();
            }
            true
        });
        if install {
            groups.push(
                json!({"hooks":[{"type":"command","command":exe,"args":[HOOK_ARG,"--activity-dir",directory()],"timeout":2}]}),
            );
        }
    }
    hooks.retain(|_, value| !value.as_array().is_some_and(|a| a.is_empty()));
    if !install && hooks.is_empty() {
        object.remove("hooks");
    }
    Ok(settings)
}

pub fn connected() -> bool {
    directory().join("installed.json").is_file()
}

pub fn install(exe: &Path, enable: bool) -> Result<Option<PathBuf>, Box<dyn std::error::Error>> {
    let path = settings_path();
    let original = match fs::read(&path) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    let settings = original
        .as_deref()
        .map(serde_json::from_slice)
        .transpose()?
        .unwrap_or_else(|| json!({}));
    let updated = configure(settings, exe, enable)?;
    let bytes = serde_json::to_vec_pretty(&updated)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let backup = if let Some(ref original) = original {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let backup = path.with_file_name(format!("settings.wacu-backup-{stamp}.json"));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&backup)?;
        file.write_all(&original)?;
        Some(backup)
    } else {
        None
    };
    let temporary = path.with_extension(format!("wacu-{}.tmp", std::process::id()));
    fs::write(&temporary, bytes)?;
    let latest = match fs::read(&path) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    if latest != original {
        let _ = fs::remove_file(temporary);
        return Err("Claude 设置在连接期间发生变化，请重新连接".into());
    }
    fs::rename(temporary, path)?;
    fs::create_dir_all(directory())?;
    if enable {
        fs::write(directory().join("installed.json"), b"{}")?;
    } else {
        let _ = fs::remove_file(directory().join("installed.json"));
    }
    Ok(backup)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input(session: &str, event: &str) -> Input {
        Input {
            session_id: session.into(),
            hook_event_name: event.into(),
            notification_type: String::new(),
            agent_id: String::new(),
            source: String::new(),
        }
    }
    #[test]
    fn lifecycle_distinguishes_open_working_waiting_and_finished() {
        let mut state = Session::default();
        state.apply(&input("a", "SessionStart"));
        assert!(!state.active());
        state.apply(&input("a", "UserPromptSubmit"));
        assert!(state.active());
        state.apply(&input("a", "PermissionRequest"));
        assert!(!state.active());
        state.apply(&input("a", "PostToolUse"));
        assert!(state.active());
        state.apply(&input("a", "Stop"));
        assert!(!state.active());
    }
    #[test]
    fn background_agents_keep_session_active_until_all_stop() {
        let mut state = Session::default();
        let mut event = input("a", "SubagentStart");
        event.agent_id = "agent1".into();
        state.apply(&event);
        state.apply(&input("a", "Stop"));
        assert!(state.active());
        event.hook_event_name = "PostToolUse".into();
        state.apply(&event);
        event.hook_event_name = "SubagentStop".into();
        state.apply(&event);
        assert!(!state.active());
        assert!(state.agents.is_empty());
    }
    #[test]
    fn installation_is_idempotent_and_preserves_other_hooks() {
        let original = json!({"env":{"CUSTOM":"unchanged"},"hooks":{"Stop":[{"hooks":[{"type":"command","command":"original-handler"}]}]}});
        let exe = Path::new("C:/App Folder/wacu.exe");
        let once = configure(original.clone(), exe, true).unwrap();
        assert_eq!(once, configure(once.clone(), exe, true).unwrap());
        assert_eq!(configure(once, exe, false).unwrap(), original);
        assert!(configure(json!({"disableAllHooks":true}), exe, true).is_err());
        let bare = json!({"env":{"CUSTOM":"unchanged"}});
        assert_eq!(
            configure(configure(bare.clone(), exe, true).unwrap(), exe, false).unwrap(),
            bare
        );
    }
    #[test]
    fn records_aggregate_sessions_and_reject_reused_or_exited_owners() {
        let root = std::env::current_dir()
            .unwrap()
            .join(format!("target/qa-activity-unit-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let owner = process_owner(std::process::id()).unwrap();
        record(&root, &input("a", "UserPromptSubmit"), owner).unwrap();
        record(&root, &input("b", "UserPromptSubmit"), owner).unwrap();
        assert_eq!(count_active(&root), 2);
        record(&root, &input("a", "Stop"), owner).unwrap();
        assert_eq!(count_active(&root), 1);
        let bytes = fs::read(root.join(format!("{}.json", key("b")))).unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("session_id"));
        record(
            &root,
            &input("b", "UserPromptSubmit"),
            Owner {
                birth: owner.birth + 1,
                ..owner
            },
        )
        .unwrap();
        assert_eq!(count_active(&root), 0);
        let workspace = fs::canonicalize(std::env::current_dir().unwrap()).unwrap();
        assert!(
            fs::canonicalize(&root)
                .unwrap()
                .starts_with(workspace.join("target"))
        );
        fs::remove_dir_all(root).unwrap();
    }
}
