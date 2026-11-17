//! 桌面项目身份与目录关联；历史 JSONL 始终由 Pi 独占写入。
use crate::{history, types::*};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRecord {
    pub id: String,
    pub current_path: String,
    pub historical_paths: Vec<String>,
    pub directory_identity: Option<String>,
    #[serde(default)]
    pub pending_path: Option<String>,
    /// 只绑定迁移时已经存在的会话，避免旧路径重新使用后串入新项目。
    pub sessions: Vec<String>,
    /// 被吸收的项目 ID 只解析对象身份，不把历史目录变成永久路径重定向。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub merged_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_merge: Option<crate::project_merge::PendingMerge>,
    /// 取消等待后不在下一次周期检查中重新安排同一目标；人工选择可重新启用。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancelled_target: Option<String>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    pub id: String,
    pub current_path: String,
    pub state: String,
    pub candidate: Option<String>,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<AppError>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectLocations {
    pub settings: Settings,
    pub locations: Vec<Location>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryListing {
    pub path: String,
    pub parent: Option<String>,
    pub directories: Vec<String>,
}

/// 原生目录标识不依赖目录名称；不能获取时不猜测匹配。
pub fn identity(path: &Path) -> std::io::Result<String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = fs::metadata(path)?;
        if !metadata.is_dir() {
            return Err(std::io::Error::other("所选路径不是目录"));
        }
        Ok(format!("unix:{}:{}", metadata.dev(), metadata.ino()))
    }
    #[cfg(windows)]
    {
        use std::os::windows::{fs::OpenOptionsExt, io::AsRawHandle};
        use windows_sys::Win32::Storage::FileSystem::{
            GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, FILE_FLAG_BACKUP_SEMANTICS,
        };
        if !fs::metadata(path)?.is_dir() {
            return Err(std::io::Error::other("所选路径不是目录"));
        }
        let file = fs::OpenOptions::new()
            .access_mode(0)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .open(path)?;
        let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        // 句柄由 File 保持存活，结构体与 Win32 ABI 对齐。
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(format!(
            "windows:{}:{}:{}",
            info.dwVolumeSerialNumber, info.nFileIndexHigh, info.nFileIndexLow
        ))
    }
}

/// 展示路径移除 Windows 扩展前缀；UNC 仍保留网络共享语义。
pub fn display_path(path: &Path) -> String {
    let value = path.to_string_lossy().into_owned();
    #[cfg(windows)]
    {
        if let Some(unc) = value.strip_prefix(r"\\?\UNC\") {
            return format!(r"\\{unc}");
        }
        value.strip_prefix(r"\\?\").unwrap_or(&value).to_owned()
    }
    #[cfg(not(windows))]
    value
}

/// 字符串路径键用于已不存在的历史目录，不随 canonicalize 的成功与否改变。
pub fn key(path: &str) -> String {
    let value = path.replace('\\', "/");
    let value = value.strip_prefix("//?/").unwrap_or(&value);
    let value = if cfg!(windows)
        && value
            .get(..4)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("unc/"))
    {
        format!("//{}", &value[4..])
    } else {
        value.to_owned()
    };
    let value = value.trim_end_matches('/');
    if cfg!(windows) {
        value.to_lowercase()
    } else {
        value.into()
    }
}
pub fn record<'a>(settings: &'a Settings, path: &str) -> Option<&'a ProjectRecord> {
    settings
        .projects
        .iter()
        .find(|p| key(&p.current_path) == key(path))
}
/// 批量摘要解析先建索引，避免长历史逐条扫描所有项目的会话列表。
pub fn session_owners(settings: &Settings) -> std::collections::HashMap<String, String> {
    settings
        .projects
        .iter()
        .flat_map(|p| {
            p.sessions
                .iter()
                .map(|session| (key(session), p.current_path.clone()))
        })
        .collect()
}

pub fn session_project<'a>(settings: &'a Settings, session: &str, original: &'a str) -> &'a str {
    settings
        .projects
        .iter()
        .find(|p| p.sessions.iter().any(|s| key(s) == key(session)))
        .map_or(original, |p| p.current_path.as_str())
}

/// 发现的历史、最近项目及尚未落盘的草稿统一登记；失效目录也不丢弃。
pub fn register(settings: &mut Settings, paths: &[String]) -> bool {
    let mut changed = false;
    for path in paths {
        if path.is_empty() || record(settings, path).is_some() {
            continue;
        }
        settings.projects.push(ProjectRecord {
            id: uuid::Uuid::new_v4().to_string(),
            current_path: path.clone(),
            historical_paths: vec![],
            directory_identity: identity(Path::new(path)).ok(),
            pending_path: None,
            sessions: vec![],
            merged_ids: vec![],
            pending_merge: None,
            cancelled_target: None,
        });
        changed = true;
    }
    changed
}

pub fn inspect(project: &ProjectRecord) -> Location {
    let path = Path::new(&project.current_path);
    let mut result = Location {
        id: project.id.clone(),
        current_path: project.current_path.clone(),
        state: "available".into(),
        candidate: None,
        message: String::new(),
        error: None,
    };
    if let Some(candidate) = &project.pending_path {
        result.state = "relocating".into();
        result.candidate = Some(candidate.clone());
        result.message = "目录已变更，当前任务结束后切换".into();
        return result;
    }
    match identity(path) {
        Ok(id)
            if project
                .directory_identity
                .as_ref()
                .is_none_or(|saved| *saved == id) =>
        {
            // Windows 和常见 macOS 卷会接受旧大小写，canonicalize 也可能保留输入拼写；
            // 通过父目录的真实条目取展示名称，再用目录身份确认，避免只改大小写时漏更新。
            if let (Some(parent), Some(name)) = (path.parent(), path.file_name()) {
                let name = name.to_string_lossy().to_lowercase();
                if let Ok(entries) = fs::read_dir(parent) {
                    if let Some(actual) = entries.filter_map(|e| e.ok()).find(|e| {
                        e.file_name().to_string_lossy().to_lowercase() == name
                            && identity(&e.path()).is_ok_and(|actual| actual == id)
                    }) {
                        let actual = display_path(&actual.path());
                        if actual.replace('\\', "/") != project.current_path.replace('\\', "/") {
                            result.state = "relocating".into();
                            result.candidate = Some(actual);
                            result.message = "目录名称已变更，当前任务结束后切换".into();
                        }
                    }
                }
            }
            return result;
        }
        Ok(_) => result.message = "原路径已由另一个目录使用，请重新关联目录".into(),
        Err(error) => {
            result.message = match error.kind() {
                std::io::ErrorKind::PermissionDenied => "没有访问项目目录的权限",
                std::io::ErrorKind::NotFound => "项目目录不存在或磁盘未连接",
                _ => "项目目录暂时不可访问",
            }
            .into();
            if error.kind() != std::io::ErrorKind::NotFound {
                result.state = "unavailable".into();
                return result;
            }
        }
    }
    result.state = "unavailable".into();
    // 只检查直接兄弟目录，不递归扫描用户磁盘；符号链接不作为自动迁移候选。
    if let (Some(saved), Some(parent)) = (&project.directory_identity, path.parent()) {
        if let Ok(entries) = fs::read_dir(parent) {
            let matches: Vec<_> = entries
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
                .filter(|e| identity(&e.path()).is_ok_and(|id| id == *saved))
                .map(|e| e.path().to_string_lossy().into_owned())
                .collect();
            if matches.len() == 1 {
                result.state = "relocating".into();
                result.candidate = matches.into_iter().next();
                result.message = "目录已变更，当前任务结束后切换".into();
            }
        }
    }
    result
}

/// 一次配置提交同步全部路径偏好；显式会话归属不依赖未来的旧路径内容。
pub fn relocate(
    settings: &mut Settings,
    id: &str,
    target: &str,
    sessions: &[SessionSummary],
) -> Result<()> {
    let target = display_path(&fs::canonicalize(target)?);
    let directory_identity = identity(Path::new(&target))?;
    if settings.projects.iter().any(|p| {
        p.id != id
            && (key(&p.current_path) == key(&target)
                || p.directory_identity.as_ref() == Some(&directory_identity))
    }) {
        return Err(AppError::new(
            "PROJECT_CONFLICT",
            "该目录已属于另一个项目，请选择其他目录",
        ));
    }
    let project = settings
        .projects
        .iter_mut()
        .find(|p| p.id == id)
        .ok_or_else(|| AppError::new("PROJECT", "项目记录不存在，请重新检查"))?;
    let old = project.current_path.clone();
    let can_capture_current =
        identity(Path::new(&old)).ok().as_ref() == project.directory_identity.as_ref();
    for session in sessions
        .iter()
        .filter(|s| can_capture_current && history::same_project(&s.cwd, &old))
    {
        if !project
            .sessions
            .iter()
            .any(|s| key(s) == key(&session.path))
        {
            project.sessions.push(session.path.clone());
        }
    }
    if !project.historical_paths.contains(&old) {
        project.historical_paths.push(old.clone());
    }
    project.current_path = target.clone();
    project.directory_identity = Some(directory_identity);
    project.pending_path = None;
    let replace = |value: &mut String| {
        if key(value) == key(&old) {
            *value = target.clone();
        }
    };
    settings.recent_projects.iter_mut().for_each(replace);
    settings.project_order.iter_mut().for_each(replace);
    for item in &mut settings.sidebar_items {
        if item.kind == "project" {
            replace(&mut item.path);
        }
    }
    for preference in &mut settings.project_models {
        replace(&mut preference.project);
    }
    settings.project_revision += 1;
    Ok(())
}

/// 内嵌选择器仅列目录，不读取文件正文；错误由当前面板展示。
pub fn browse(path: &str) -> Result<DirectoryListing> {
    let path = fs::canonicalize(path)?;
    if !path.is_dir() {
        return Err(AppError::new("PROJECT", "所选路径不是目录"));
    }
    let mut directories = Vec::new();
    for entry in fs::read_dir(&path)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            directories.push(display_path(&entry.path()));
        }
    }
    directories.sort_by_key(|a| key(a));
    Ok(DirectoryListing {
        parent: path.parent().map(display_path),
        path: display_path(&path),
        directories,
    })
}

/// 共享父目录监听只发出失效信号；事件不携带未经核验的新路径。
pub struct ProjectWatch {
    watcher: notify::RecommendedWatcher,
    parents: std::collections::HashSet<std::path::PathBuf>,
    candidates: std::sync::Arc<std::sync::Mutex<Vec<std::path::PathBuf>>>,
}
impl ProjectWatch {
    pub fn new(app: tauri::AppHandle) -> notify::Result<Self> {
        use tauri::Emitter;
        let candidates = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let pending = candidates.clone();
        let watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            if let Ok(event) = event {
                if let Ok(mut paths) = pending.lock() {
                    paths.extend(event.paths);
                    let excess = paths.len().saturating_sub(128);
                    paths.drain(..excess);
                }
                let _ = app.emit("project-locations-changed", ());
            }
        })?;
        Ok(Self {
            watcher,
            parents: Default::default(),
            candidates,
        })
    }
    pub fn candidates(&self) -> Vec<std::path::PathBuf> {
        self.candidates
            .lock()
            .map(|p| p.clone())
            .unwrap_or_default()
    }
    pub fn sync(&mut self, projects: &[ProjectRecord]) {
        use notify::Watcher;
        let parents: std::collections::HashSet<_> = projects
            .iter()
            .filter_map(|p| Path::new(&p.current_path).parent().map(Path::to_path_buf))
            .collect();
        for parent in self.parents.difference(&parents) {
            let _ = self.watcher.unwatch(parent);
        }
        self.parents.retain(|p| parents.contains(p));
        for parent in parents
            .difference(&self.parents)
            .cloned()
            .collect::<Vec<_>>()
        {
            if self
                .watcher
                .watch(&parent, notify::RecursiveMode::NonRecursive)
                .is_ok()
            {
                self.parents.insert(parent);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ipc_project_record_matches_shared_typescript_fixture() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/project-record.json")).unwrap();
        let record: ProjectRecord = serde_json::from_value(fixture.clone()).unwrap();
        assert_eq!(serde_json::to_value(record).unwrap(), fixture);
    }
    #[test]
    fn rename_identity_and_preferences_survive_without_rewriting_history() {
        let root = tempfile::tempdir().unwrap();
        let old = root.path().join("旧 项目");
        let new = root.path().join("新 项目");
        fs::create_dir(&old).unwrap();
        let old = old.to_string_lossy().into_owned();
        let mut settings = Settings {
            recent_projects: vec![old.clone()],
            project_order: vec![old.clone()],
            ..Settings::default()
        };
        register(&mut settings, std::slice::from_ref(&old));
        let id = settings.projects[0].id.clone();
        fs::rename(&old, &new).unwrap();
        let location = inspect(&settings.projects[0]);
        assert_eq!(location.state, "relocating");
        relocate(
            &mut settings,
            &id,
            location.candidate.as_deref().unwrap(),
            &[],
        )
        .unwrap();
        assert_eq!(settings.projects[0].id, id);
        assert!(history::same_project(
            &settings.project_order[0],
            new.to_str().unwrap()
        ));
        assert_eq!(inspect(&settings.projects[0]).state, "available");
        fs::create_dir(&old).unwrap();
        assert_eq!(session_project(&settings, "new.jsonl", &old), old);
    }
    #[test]
    fn explicit_session_ownership_survives_repeated_moves_and_reused_old_paths() {
        let root = tempfile::tempdir().unwrap();
        let old = root.path().join("old");
        fs::create_dir(&old).unwrap();
        let mut settings = Settings::default();
        let old = old.to_string_lossy().into_owned();
        register(&mut settings, std::slice::from_ref(&old));
        let id = settings.projects[0].id.clone();
        let session: SessionSummary = serde_json::from_value(serde_json::json!({
            "path":"history.jsonl", "id":"session", "cwd":old, "name":"", "preview":"", "timestamp":"", "modified":0, "lastPromptAt":"", "warning":null
        })).unwrap();
        settings.sidebar_items.push(SidebarItem {
            kind: "project".into(),
            path: old.clone(),
            name: Some("别名".into()),
            state: Some("archived".into()),
        });
        let new = root.path().join("new");
        fs::rename(&old, &new).unwrap();
        settings.projects[0].sessions.push(session.path.clone());
        relocate(
            &mut settings,
            &id,
            new.to_str().unwrap(),
            std::slice::from_ref(&session),
        )
        .unwrap();
        fs::create_dir(&old).unwrap();
        assert!(history::same_project(
            session_project(&settings, "history.jsonl", &old),
            new.to_str().unwrap()
        ));
        assert_eq!(session_project(&settings, "new-session.jsonl", &old), old);
        assert_eq!(settings.sidebar_items[0].name.as_deref(), Some("别名"));
        assert_eq!(settings.sidebar_items[0].state.as_deref(), Some("archived"));
        let last = root.path().join("last");
        fs::rename(&new, &last).unwrap();
        relocate(&mut settings, &id, last.to_str().unwrap(), &[session]).unwrap();
        assert!(history::same_project(
            session_project(&settings, "history.jsonl", &old),
            last.to_str().unwrap()
        ));
        assert_eq!(settings.projects[0].sessions.len(), 1);
    }
    #[test]
    fn case_only_rename_updates_the_display_path() {
        let root = tempfile::tempdir().unwrap();
        let old = root.path().join("Project");
        let new = root.path().join("project");
        fs::create_dir(&old).unwrap();
        let mut settings = Settings::default();
        register(&mut settings, &[old.to_string_lossy().into()]);
        fs::rename(&old, &new).unwrap();
        let location = inspect(&settings.projects[0]);
        assert_eq!(location.state, "relocating");
        assert_eq!(
            Path::new(location.candidate.as_deref().unwrap())
                .file_name()
                .unwrap(),
            "project"
        );
    }
    #[test]
    fn conflicting_target_and_non_directory_leave_registry_unchanged() {
        let root = tempfile::tempdir().unwrap();
        let a = root.path().join("a");
        let b = root.path().join("b");
        fs::create_dir(&a).unwrap();
        fs::create_dir(&b).unwrap();
        let mut settings = Settings::default();
        register(
            &mut settings,
            &[a.to_string_lossy().into(), b.to_string_lossy().into()],
        );
        let id = settings.projects[0].id.clone();
        assert!(relocate(&mut settings, &id, b.to_str().unwrap(), &[]).is_err());
        let file = root.path().join("file");
        fs::write(&file, "content").unwrap();
        assert!(relocate(&mut settings, &id, file.to_str().unwrap(), &[]).is_err());
        assert_eq!(settings.project_revision, 0);
        assert!(history::same_project(
            &settings.projects[0].current_path,
            a.to_str().unwrap()
        ));
        assert_eq!(
            browse(root.path().to_str().unwrap())
                .unwrap()
                .directories
                .len(),
            2
        );
    }
    #[test]
    fn copies_and_unknown_identity_are_never_automatically_associated() {
        let root = tempfile::tempdir().unwrap();
        let old = root.path().join("old");
        fs::create_dir(&old).unwrap();
        let mut settings = Settings::default();
        register(&mut settings, &[old.to_string_lossy().into()]);
        fs::rename(&old, root.path().join("new")).unwrap();
        fs::create_dir(&old).unwrap();
        assert_eq!(inspect(&settings.projects[0]).state, "relocating");
        settings.projects[0].directory_identity = None;
        fs::remove_dir(&old).unwrap();
        assert_eq!(inspect(&settings.projects[0]).state, "unavailable");
    }
}
