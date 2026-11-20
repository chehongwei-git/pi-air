//! 根据会话工具事件采集目标文件；不从整个工作区的变化猜测修改归属。
use super::{compare, FileChange, Snapshot, Turn, ENTRY_LIMIT, FILE_LIMIT, TOTAL_LIMIT};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::Read,
    path::{Component, Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock, Weak,
    },
};

type Writers = BTreeMap<PathBuf, Vec<Weak<AtomicBool>>>;
static WRITERS: OnceLock<Mutex<Writers>> = OnceLock::new();

struct Writer {
    path: PathBuf,
    conflict: Arc<AtomicBool>,
}
impl Drop for Writer {
    fn drop(&mut self) {
        let mut writers = WRITERS.get_or_init(Default::default).lock().unwrap();
        if let Some(owners) = writers.get_mut(&self.path) {
            let own = Arc::downgrade(&self.conflict);
            owners.retain(|owner| !owner.ptr_eq(&own) && owner.strong_count() > 0);
            if owners.is_empty() {
                writers.remove(&self.path);
            }
        }
    }
}

/// 租约持续到整轮结束，连同工具之间的交错写入一起标记；Drop 及时释放索引。
fn register(path: PathBuf) -> Writer {
    let flag = Arc::new(AtomicBool::new(false));
    let mut writers = WRITERS.get_or_init(Default::default).lock().unwrap();
    let owners = writers.entry(path.clone()).or_default();
    owners.retain(|owner| owner.strong_count() > 0);
    for other in owners.iter().filter_map(Weak::upgrade) {
        other.store(true, Ordering::SeqCst);
        flag.store(true, Ordering::SeqCst);
    }
    owners.push(Arc::downgrade(&flag));
    Writer {
        path,
        conflict: flag,
    }
}

#[derive(Clone, PartialEq)]
enum Observation {
    Missing,
    Content(Vec<u8>),
    Unavailable(String),
}
impl Observation {
    fn snapshot(&self, path: &str) -> Snapshot {
        let mut snapshot = Snapshot::default();
        match self {
            Self::Content(bytes) => {
                snapshot.files.insert(path.into(), bytes.clone());
            }
            Self::Unavailable(reason) => {
                snapshot.omitted.insert(path.into(), reason.clone());
            }
            Self::Missing => {}
        }
        snapshot
    }
}

/// 不跟随任何层级的链接；新文件允许不存在，但其最近存在的父目录必须在项目内。
fn target(root: &Path, raw: &str) -> Result<(PathBuf, String), String> {
    let canonical_root = root.canonicalize().map_err(|_| "项目目录无法读取")?;
    let input = Path::new(raw);
    let joined = if let Ok(relative) = input.strip_prefix(root) {
        canonical_root.join(relative)
    } else if input.is_absolute() {
        input.to_path_buf()
    } else {
        canonical_root.join(input)
    };
    let root = canonical_root;
    let mut normalized = PathBuf::new();
    for part in joined.components() {
        match part {
            Component::ParentDir => {
                normalized.pop();
            }
            Component::CurDir => {}
            _ => {
                normalized.push(part.as_os_str());
                if std::fs::symlink_metadata(&normalized)
                    .is_ok_and(|meta| meta.file_type().is_symlink())
                {
                    return Err("符号链接未比较".into());
                }
            }
        }
    }
    // 先规范化现存祖先再补回新文件后缀，兼容 Windows canonicalize 的 verbatim 前缀。
    let mut ancestor = normalized.as_path();
    let mut missing = Vec::new();
    loop {
        match std::fs::symlink_metadata(ancestor) {
            Ok(meta) if meta.file_type().is_symlink() => return Err("符号链接未比较".into()),
            Ok(_) => break,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                missing.push(
                    ancestor
                        .file_name()
                        .ok_or("目标路径无法读取")?
                        .to_os_string(),
                );
                ancestor = ancestor.parent().ok_or("目标路径无法读取")?;
            }
            Err(_) => return Err("目标路径无法读取".into()),
        }
    }
    // 祖先中存在链接也拒绝，不能通过 canonicalize 隐式授权链接目标。
    for parent in ancestor.ancestors() {
        if std::fs::symlink_metadata(parent).is_ok_and(|meta| meta.file_type().is_symlink()) {
            return Err("符号链接未比较".into());
        }
    }
    normalized = ancestor.canonicalize().map_err(|_| "目标路径无法读取")?;
    for suffix in missing.into_iter().rev() {
        normalized.push(suffix);
    }
    #[cfg(windows)]
    let inside = normalized
        .to_string_lossy()
        .to_lowercase()
        .starts_with(&format!(
            "{}\\",
            root.to_string_lossy().to_lowercase().trim_end_matches('\\')
        ));
    #[cfg(not(windows))]
    let inside = normalized.starts_with(&root) && normalized != root;
    if !inside {
        return Err("项目外目标未比较".into());
    }
    let display = normalized
        .components()
        .skip(root.components().count())
        .map(|part| part.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    Ok((normalized, display))
}

fn observe(root: &Path, raw: &str, budget: &mut usize) -> Observation {
    let (path, _) = match target(root, raw) {
        Ok(target) => target,
        Err(reason) => return Observation::Unavailable(reason),
    };
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let mut file = match options.open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Observation::Missing,
        Err(_) => return Observation::Unavailable("文件无法读取".into()),
    };
    let Ok(before) = file.metadata() else {
        return Observation::Unavailable("属性无法读取".into());
    };
    if !before.is_file() || before.file_type().is_symlink() {
        return Observation::Unavailable("非普通文件未比较".into());
    }
    if before.len() > FILE_LIMIT {
        return Observation::Unavailable("超过单文件 1 MiB".into());
    }
    let remaining = TOTAL_LIMIT.saturating_sub(*budget);
    if before.len() as usize > remaining {
        return Observation::Unavailable("超过本轮目标快照预算".into());
    }
    let mut bytes = Vec::new();
    let result = (&mut file).take(before.len() + 1).read_to_end(&mut bytes);
    *budget += bytes.len();
    let stable = file.metadata().is_ok_and(|after| {
        before.len() == after.len()
            && after.len() == bytes.len() as u64
            && target(root, raw).is_ok_and(|(current, _)| current == path)
            && before.modified().ok() == after.modified().ok()
    });
    if result.is_ok() && stable {
        Observation::Content(bytes)
    } else {
        Observation::Unavailable("读取失败或读取期间变化".into())
    }
}

struct Target {
    display: String,
    raw: String,
    before: Observation,
    last: Observation,
    conflict: Writer,
    reason: Option<String>,
    calls: Vec<String>,
    changed: bool,
}
struct Call {
    path: String,
    before: Observation,
    expected: Option<Vec<u8>>,
}

pub(super) struct Capture {
    pub title: String,
    targets: BTreeMap<String, Target>,
    active: BTreeMap<String, Call>,
    budget: usize,
    incomplete: bool,
    calls: usize,
}
impl Capture {
    pub fn new(title: &str) -> Self {
        Self {
            title: title.chars().take(80).collect(),
            targets: BTreeMap::new(),
            active: BTreeMap::new(),
            budget: 0,
            incomplete: false,
            calls: 0,
        }
    }
    /// 只消费结构化工具事件；未知工具不解析 shell 命令来猜测写入文件。
    pub fn event(&mut self, root: &Path, event: &Value) {
        let Some(id) = event["toolCallId"].as_str() else {
            return;
        };
        match event["type"].as_str() {
            Some("tool_execution_start") => {
                let name = event["toolName"].as_str().unwrap_or("");
                if !matches!(name, "edit" | "write") {
                    if !matches!(name, "read" | "grep" | "find" | "ls") {
                        self.incomplete = true;
                    }
                    return;
                }
                let Some(raw) = event["args"]["path"]
                    .as_str()
                    .or_else(|| event["args"]["file_path"].as_str())
                else {
                    self.incomplete = true;
                    return;
                };
                let (identity, path) =
                    target(root, raw).unwrap_or_else(|_| (root.join(raw), raw.into()));
                // Windows 的普通文件身份不区分大小写；显示路径保留工具参数的原始大小写。
                #[cfg(windows)]
                let identity = PathBuf::from(identity.to_string_lossy().to_lowercase());
                #[cfg(windows)]
                let key = path.to_lowercase();
                #[cfg(not(windows))]
                let key = path.clone();
                self.calls += 1;
                if self.calls > ENTRY_LIMIT
                    || self.active.len() >= ENTRY_LIMIT
                    || (!self.targets.contains_key(&key) && self.targets.len() >= ENTRY_LIMIT)
                {
                    self.incomplete = true;
                    return;
                }
                let before = observe(root, raw, &mut self.budget);
                let entry = self.targets.entry(key.clone()).or_insert_with(|| Target {
                    display: path.clone(),
                    raw: raw.into(),
                    before: before.clone(),
                    last: before.clone(),
                    conflict: register(identity),
                    reason: None,
                    calls: Vec::new(),
                    changed: false,
                });
                if entry.last != before {
                    entry.reason = Some("工具调用之间文件发生外部变化".into());
                }
                if self.active.values().any(|call| call.path == key) {
                    entry.reason = Some("同文件工具执行重叠".into());
                }
                entry.calls.push(id.into());
                let expected = if name == "write" {
                    event["args"]["content"]
                        .as_str()
                        .map(|s| Sha256::digest(s.as_bytes()).to_vec())
                } else {
                    None
                };
                self.active.insert(
                    id.into(),
                    Call {
                        path: key,
                        before,
                        expected,
                    },
                );
            }
            Some("tool_execution_end") => {
                let Some(call) = self.active.remove(id) else {
                    if matches!(event["toolName"].as_str(), Some("edit" | "write")) {
                        self.incomplete = true;
                    }
                    return;
                };
                let entry = self.targets.get_mut(&call.path).unwrap();
                let after = observe(root, &entry.raw, &mut self.budget);
                let changed = call.before != after;
                let tool_diff = event["result"]["details"]["diff"]
                    .as_str()
                    .is_some_and(|diff| {
                        diff.lines()
                            .any(|line| line.starts_with('+') || line.starts_with('-'))
                    });
                entry.changed |= changed || tool_diff;
                if event["isError"] == true && changed {
                    entry.reason = Some("工具失败后目标变化，归属未确认".into());
                }
                if tool_diff && !changed {
                    entry.reason = Some("快照未捕获工具报告的修改".into());
                }
                if call.expected.is_some() && !changed && event["isError"] != true {
                    entry.reason = Some("写入前快照未捕获差异，无法确认原始内容".into());
                }
                if let Some(expected) = call.expected.as_ref().filter(|_| event["isError"] != true)
                {
                    let matches = match &after {
                        Observation::Content(bytes) => Sha256::digest(bytes).to_vec() == *expected,
                        _ => false,
                    };
                    if !matches {
                        entry.reason = Some("文件内容与工具写入内容不一致".into());
                    }
                }
                entry.last = after;
            }
            _ => {}
        }
    }
    pub fn finish(mut self, state: String) -> Turn {
        for call in self.active.values() {
            let entry = self.targets.get_mut(&call.path).unwrap();
            entry.reason = Some("工具未结束，修改归属未确认".into());
            entry.changed = true;
        }
        let mut files = Vec::new();
        for (_, entry) in self.targets {
            let path = entry.display.clone();
            let unavailable = match (&entry.before, &entry.last) {
                (Observation::Unavailable(reason), _) | (_, Observation::Unavailable(reason)) => {
                    Some(reason.clone())
                }
                _ => None,
            };
            let reason = entry
                .reason
                .or_else(|| {
                    entry
                        .conflict
                        .conflict
                        .load(Ordering::SeqCst)
                        .then(|| "其他会话涉及同一文件，修改归属未确认".into())
                })
                .or(unavailable);
            if !entry.changed && reason.is_none() {
                continue;
            }
            if let Some(reason) = reason {
                files.push(FileChange {
                    path,
                    kind: "modified".into(),
                    added: None,
                    removed: None,
                    diff: String::new(),
                    binary: false,
                    diff_accuracy: None,
                    attribution: Some("unconfirmed".into()),
                    reason: Some(reason),
                    call_ids: entry.calls,
                });
            } else if let Some(mut change) = compare(
                entry.before.snapshot(&path),
                entry.last.snapshot(&path),
                String::new(),
                state.clone(),
            )
            .files
            .pop()
            {
                change.attribution = Some("observed".into());
                change.call_ids = entry.calls;
                files.push(change);
            }
        }
        let incomplete = self.incomplete
            || files
                .iter()
                .any(|file| file.attribution.as_deref() == Some("unconfirmed"));
        let mut warnings = vec![
            "仅记录本会话 edit/write 的目标；快照不是原子写入证据，同文件外部编辑可能影响结果。"
                .into(),
        ];
        if self.incomplete {
            warnings.push("其他工具可能存在未记录修改，或工具目标超过采集上限。".into());
        }
        Turn {
            id: uuid::Uuid::new_v4().to_string(),
            title: self.title,
            state,
            files,
            warnings,
            capture_version: Some(3),
            coverage: Some(
                if incomplete {
                    "partial"
                } else {
                    "tool-targets"
                }
                .into(),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn start(capture: &mut Capture, root: &Path, id: &str, path: &str) {
        capture.event(root, &json!({"type":"tool_execution_start", "toolCallId":id, "toolName":"edit", "args":{"path":path}}));
    }
    fn end(capture: &mut Capture, root: &Path, id: &str, failed: bool) {
        capture.event(
            root,
            &json!({"type":"tool_execution_end", "toolCallId":id, "isError":failed}),
        );
    }
    fn modify(capture: &mut Capture, root: &Path, id: &str, path: &str, text: &str) {
        start(capture, root, id, path);
        std::fs::write(root.join(path), text).unwrap();
        end(capture, root, id, false);
    }

    #[test]
    fn separate_sessions_and_external_files_do_not_mix() {
        let root = tempfile::tempdir().unwrap();
        let mut a = Capture::new("a");
        let mut b = Capture::new("b");
        modify(&mut a, root.path(), "a", "a", "one\n");
        modify(&mut b, root.path(), "b", "b", "two\n");
        std::fs::write(root.path().join("external"), "outside").unwrap();
        let a = a.finish("done".into());
        let b = b.finish("done".into());
        assert_eq!(
            a.files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
            ["a"]
        );
        assert_eq!(
            b.files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
            ["b"]
        );
        assert_eq!(a.files[0].attribution.as_deref(), Some("observed"));
        assert_eq!(a.files[0].added, Some(1));
    }

    #[test]
    fn conflicting_sessions_and_intervening_edits_are_unconfirmed() {
        let root = tempfile::tempdir().unwrap();
        let mut a = Capture::new("a");
        let mut b = Capture::new("b");
        modify(&mut a, root.path(), "a", "same", "one");
        modify(&mut b, root.path(), "b", "same", "two");
        for turn in [a.finish("done".into()), b.finish("done".into())] {
            assert_eq!(turn.coverage.as_deref(), Some("partial"));
            assert_eq!(turn.files[0].attribution.as_deref(), Some("unconfirmed"));
            assert!(turn.files[0].diff.is_empty());
            assert_eq!(turn.files[0].added, None);
        }
        let mut capture = Capture::new("external same file");
        modify(&mut capture, root.path(), "first", "same", "three");
        std::fs::write(root.path().join("same"), "external").unwrap();
        modify(&mut capture, root.path(), "second", "same", "four");
        assert!(capture.finish("done".into()).files[0]
            .reason
            .as_ref()
            .unwrap()
            .contains("外部变化"));
    }

    #[test]
    fn net_diff_uses_tool_edges_and_reverts_disappear() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("build")).unwrap();
        let path = "build/source";
        std::fs::write(root.path().join(path), "local\nold\n").unwrap();
        let mut capture = Capture::new("net");
        modify(
            &mut capture,
            root.path(),
            "first",
            path,
            "local\nintermediate\n",
        );
        modify(&mut capture, root.path(), "second", path, "local\nnew\n");
        let turn = capture.finish("done".into());
        assert_eq!(
            (turn.files[0].added, turn.files[0].removed),
            (Some(1), Some(1))
        );
        assert_eq!(turn.files[0].path, path);
        assert_eq!(turn.files[0].call_ids, ["first", "second"]);
        assert!(!turn.files[0].diff.contains("intermediate"));
        let mut capture = Capture::new("revert");
        modify(&mut capture, root.path(), "third", path, "temporary");
        modify(&mut capture, root.path(), "fourth", path, "local\nnew\n");
        assert!(capture.finish("done".into()).files.is_empty());
    }

    #[test]
    fn unknown_tools_failed_writes_and_unfinished_calls_are_explicit() {
        let root = tempfile::tempdir().unwrap();
        let mut capture = Capture::new("script");
        capture.event(root.path(), &json!({"type":"tool_execution_start","toolCallId":"shell","toolName":"bash","args":{"command":"irrelevant"}}));
        std::fs::write(root.path().join("script-output"), "output").unwrap();
        let turn = capture.finish("done".into());
        assert!(turn.files.is_empty());
        assert_eq!(turn.coverage.as_deref(), Some("partial"));
        let mut capture = Capture::new("read");
        capture.event(
            root.path(),
            &json!({"type":"tool_execution_start","toolCallId":"read","toolName":"read"}),
        );
        assert_eq!(
            capture.finish("done".into()).coverage.as_deref(),
            Some("tool-targets")
        );
        let mut capture = Capture::new("failed");
        start(&mut capture, root.path(), "failed", "partial");
        std::fs::write(root.path().join("partial"), "partial write").unwrap();
        end(&mut capture, root.path(), "failed", true);
        start(&mut capture, root.path(), "unfinished", "unfinished");
        let turn = capture.finish("stopped".into());
        assert_eq!(turn.files.len(), 2);
        assert!(
            turn.files
                .iter()
                .all(|file| file.attribution.as_deref() == Some("unconfirmed")
                    && file.added.is_none())
        );
        let mut capture = Capture::new("unchanged failed");
        start(&mut capture, root.path(), "failed", "missing");
        end(&mut capture, root.path(), "failed", true);
        assert!(capture.finish("error".into()).files.is_empty());
    }

    #[test]
    fn oversized_outside_and_budget_targets_remain_visible() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let mut capture = Capture::new("limits");
        std::fs::write(
            root.path().join("large"),
            vec![b'a'; FILE_LIMIT as usize + 1],
        )
        .unwrap();
        start(&mut capture, root.path(), "large", "large");
        end(&mut capture, root.path(), "large", false);
        let outside_path = outside.path().join("new");
        start(
            &mut capture,
            root.path(),
            "outside",
            outside_path.to_str().unwrap(),
        );
        end(&mut capture, root.path(), "outside", false);
        std::fs::write(root.path().join("budget"), "x").unwrap();
        capture.budget = TOTAL_LIMIT;
        start(&mut capture, root.path(), "budget", "budget");
        end(&mut capture, root.path(), "budget", false);
        let turn = capture.finish("done".into());
        assert_eq!(turn.files.len(), 3);
        assert!(turn
            .files
            .iter()
            .all(|file| file.reason.is_some() && file.added.is_none()));
        assert!(!outside_path.exists());
    }

    #[test]
    fn writes_validate_content_and_late_start_does_not_silently_drop_path() {
        let root = tempfile::tempdir().unwrap();
        let mut capture = Capture::new("overwrite");
        std::fs::write(root.path().join("a"), "old\n").unwrap();
        capture.event(root.path(), &json!({"type":"tool_execution_start","toolCallId":"w","toolName":"write","args":{"file_path":root.path().join("a"),"content":"new\n"}}));
        std::fs::write(root.path().join("a"), "new\n").unwrap();
        end(&mut capture, root.path(), "w", false);
        let turn = capture.finish("done".into());
        assert_eq!(
            (turn.files[0].added, turn.files[0].removed),
            (Some(1), Some(1))
        );
        let mut capture = Capture::new("late");
        capture.event(root.path(), &json!({"type":"tool_execution_start","toolCallId":"w","toolName":"write","args":{"path":"a","content":"new\n"}}));
        end(&mut capture, root.path(), "w", false);
        assert_eq!(
            capture.finish("done".into()).files[0]
                .attribution
                .as_deref(),
            Some("unconfirmed")
        );
    }

    #[test]
    fn successful_write_with_missing_or_replaced_output_is_unconfirmed() {
        let root = tempfile::tempdir().unwrap();
        for replacement in [Some("external"), None] {
            std::fs::write(root.path().join("a"), "old").unwrap();
            let mut capture = Capture::new("interference");
            capture.event(root.path(), &json!({"type":"tool_execution_start","toolCallId":"w","toolName":"write","args":{"path":"a","content":"intended"}}));
            match replacement {
                Some(text) => std::fs::write(root.path().join("a"), text).unwrap(),
                None => std::fs::remove_file(root.path().join("a")).unwrap(),
            }
            end(&mut capture, root.path(), "w", false);
            let turn = capture.finish("done".into());
            assert_eq!(turn.files.len(), 1);
            assert_eq!(turn.files[0].attribution.as_deref(), Some("unconfirmed"));
            assert_eq!(turn.files[0].added, None);
            assert_eq!(turn.files[0].removed, None);
            assert!(turn.files[0].diff.is_empty());
            assert!(turn.files[0].reason.as_ref().unwrap().contains("不一致"));
        }
        // 写入失败且磁盘未变化时，不把尝试过的路径伪装成代码变更。
        std::fs::write(root.path().join("a"), "old").unwrap();
        let mut capture = Capture::new("failed unchanged write");
        capture.event(root.path(), &json!({"type":"tool_execution_start","toolCallId":"w","toolName":"write","args":{"path":"a","content":"intended"}}));
        end(&mut capture, root.path(), "w", true);
        assert!(capture.finish("error".into()).files.is_empty());
    }

    #[test]
    fn overlapping_calls_and_missing_start_are_not_precise() {
        let root = tempfile::tempdir().unwrap();
        let mut capture = Capture::new("overlap");
        start(&mut capture, root.path(), "one", "a");
        start(&mut capture, root.path(), "two", "a");
        std::fs::write(root.path().join("a"), "new").unwrap();
        end(&mut capture, root.path(), "one", false);
        end(&mut capture, root.path(), "two", false);
        assert_eq!(
            capture.finish("done".into()).files[0]
                .attribution
                .as_deref(),
            Some("unconfirmed")
        );
        let mut capture = Capture::new("missing");
        capture.event(
            root.path(),
            &json!({"type":"tool_execution_end","toolCallId":"missing","toolName":"edit"}),
        );
        assert_eq!(
            capture.finish("done".into()).coverage.as_deref(),
            Some("partial")
        );
    }

    #[cfg(unix)]
    #[test]
    fn linked_directories_and_files_are_not_read() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret"), "secret").unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("link")).unwrap();
        let mut capture = Capture::new("link");
        start(&mut capture, root.path(), "link", "link/secret");
        end(&mut capture, root.path(), "link", false);
        let turn = capture.finish("done".into());
        assert!(turn.files[0].reason.as_ref().unwrap().contains("符号链接"));
        assert!(turn.files[0].diff.is_empty());
    }
}
