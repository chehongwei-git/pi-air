//! 目录关联的纯数据规则；自动与人工入口共用核验，文件内容始终归 Pi 管理。
use crate::{
    projects::{self, ProjectRecord},
    types::*,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, fs, path::Path};

#[cfg(test)]
mod tests;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingMerge {
    pub target_id: Option<String>,
    pub directory_identity: String,
    pub automatic: bool,
    /// retiring 落盘后禁止取消，崩溃恢复只能继续，避免已退休队列回到旧目录。
    pub retiring: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub id: String,
    pub target: String,
    pub target_id: Option<String>,
    pub directory_identity: String,
    pub revision: u64,
    pub source_name: String,
    pub target_name: Option<String>,
    pub source_sessions: usize,
    pub target_sessions: usize,
    pub automatic: bool,
    pub alias: Option<String>,
    pub model: Option<ProjectModelPreference>,
    pub archived: bool,
}

pub fn resolve<'a>(settings: &'a Settings, id: &str) -> Result<&'a ProjectRecord> {
    settings
        .projects
        .iter()
        .find(|p| p.id == id || p.merged_ids.iter().any(|alias| alias == id))
        .ok_or_else(|| AppError::new("PROJECT", "项目记录不存在，请重新检查"))
}

/// Windows 的短路径与长路径可指向同一目录；仅在合并边界核对，历史路径键仍保持稳定。
fn same_path(a: &str, b: &str) -> bool {
    projects::key(a) == projects::key(b)
        || fs::canonicalize(a)
            .ok()
            .zip(fs::canonicalize(b).ok())
            .is_some_and(|(a, b)| {
                projects::key(&a.to_string_lossy()) == projects::key(&b.to_string_lossy())
            })
}
fn item<'a>(settings: &'a Settings, path: &str) -> Option<&'a SidebarItem> {
    settings
        .sidebar_items
        .iter()
        .find(|i| i.kind == "project" && same_path(&i.path, path))
}
fn name(settings: &Settings, path: &str) -> String {
    item(settings, path)
        .and_then(|i| i.name.clone())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| {
            Path::new(path)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        })
}
fn archived(item: Option<&SidebarItem>) -> bool {
    item.and_then(|i| i.state.as_deref())
        .is_some_and(|s| s == "archived" || s == "deleted")
}

/// 预览仅读取目录元数据和设置，不登记新项目、不修改历史。
pub fn preview(settings: &Settings, id: &str, target: &str) -> Result<Preview> {
    let source = resolve(settings, id)?;
    let path = fs::canonicalize(target).map_err(|e| {
        AppError::new(
            match e.kind() {
                std::io::ErrorKind::NotFound => "PROJECT_NOT_FOUND",
                std::io::ErrorKind::PermissionDenied => "PROJECT_PERMISSION",
                _ => "PROJECT_DIRECTORY",
            },
            e.to_string(),
        )
    })?;
    let directory_identity = projects::identity(&path)?;
    let target = projects::display_path(&path);
    let owners: Vec<_> = settings
        .projects
        .iter()
        .filter(|p| {
            p.id != source.id
                && (projects::key(&p.current_path) == projects::key(&target)
                    || p.directory_identity.as_ref() == Some(&directory_identity))
        })
        .collect();
    if owners.len() > 1 {
        return Err(AppError::new(
            "PROJECT_AMBIGUOUS",
            "存在多个目录归属，请检查项目记录",
        ));
    }
    let owner = owners.first().copied();
    if owner.is_some_and(|p| p.pending_path.is_some()) {
        return Err(AppError::new(
            "PROJECT_PENDING",
            "目标项目正在更新目录，请稍候",
        ));
    }
    // 同名和旧路径不构成证据；登记的双方与目标的当前身份必须同时匹配。
    let automatic = source.directory_identity.as_ref() == Some(&directory_identity)
        && owner.is_none_or(|p| {
            p.directory_identity.as_ref() == Some(&directory_identity)
                && projects::identity(Path::new(&p.current_path)).ok().as_ref()
                    == Some(&directory_identity)
        });
    let old_item = item(settings, &source.current_path);
    let new_item = owner.and_then(|p| item(settings, &p.current_path));
    let model = [&source.current_path, &target].iter().find_map(|path| {
        settings
            .project_models
            .iter()
            .find(|m| same_path(&m.project, path))
            .cloned()
    });
    Ok(Preview {
        id: source.id.clone(),
        target,
        target_id: owner.map(|p| p.id.clone()),
        directory_identity,
        revision: settings.project_revision,
        source_name: name(settings, &source.current_path),
        target_name: owner.map(|p| name(settings, &p.current_path)),
        source_sessions: source.sessions.len(),
        target_sessions: owner.map_or(0, |p| p.sessions.len()),
        automatic,
        alias: old_item
            .and_then(|i| i.name.clone())
            .filter(|n| !n.is_empty())
            .or_else(|| new_item.and_then(|i| i.name.clone())),
        model,
        archived: archived(old_item) && (owner.is_none() || archived(new_item)),
    })
}

/// 人工确认的旧项目分组可接纳尚未登记的历史，但绝不抢占其他项目明确拥有的文件。
pub fn capture_manual(
    settings: &mut Settings,
    id: &str,
    sessions: &[SessionSummary],
) -> Result<()> {
    let source = resolve(settings, id)?;
    let source_id = source.id.clone();
    let old = source.current_path.clone();
    let mut owned: HashSet<_> = settings
        .projects
        .iter()
        .flat_map(|p| p.sessions.iter().map(|s| projects::key(s)))
        .collect();
    let pending: Vec<_> = sessions
        .iter()
        .filter(|s| crate::history::same_project(&s.cwd, &old))
        .filter(|s| owned.insert(projects::key(&s.path)))
        .map(|s| s.path.clone())
        .collect();
    settings
        .projects
        .iter_mut()
        .find(|p| p.id == source_id)
        .unwrap()
        .sessions
        .extend(pending);
    Ok(())
}

/// 复核持久化的证据；取消过的自动目标不重新排队，人工提交可显式解除。
pub fn valid_pending(record: &ProjectRecord, preview: &Preview) -> bool {
    record.pending_merge.as_ref().map_or_else(
        || {
            record.pending_path.is_some()
                || (preview.automatic
                    && record
                        .cancelled_target
                        .as_ref()
                        .is_none_or(|p| projects::key(p) != projects::key(&preview.target)))
        },
        |pending| {
            pending.directory_identity == preview.directory_identity
                && pending.target_id == preview.target_id
                && (!pending.automatic || preview.automatic)
        },
    )
}

/// 偏好基于提交时最新设置合并，避免后台扫描覆盖用户刚修改的无关偏好。
pub fn preferences(settings: &mut Settings, old: &str, new: &str) {
    let old_key = projects::key(old);
    let new_key = projects::key(new);
    let belongs = |path: &str| same_path(path, old) || same_path(path, new);
    let old_item = item(settings, old).cloned();
    let new_item = item(settings, new).cloned();
    let mut chosen = old_item.clone().or_else(|| new_item.clone());
    if let Some(item) = &mut chosen {
        item.path = new.into();
        item.name = old_item
            .as_ref()
            .and_then(|i| i.name.clone())
            .filter(|n| !n.is_empty())
            .or_else(|| new_item.as_ref().and_then(|i| i.name.clone()));
        item.state = if archived(old_item.as_ref()) && archived(new_item.as_ref()) {
            Some("archived".into())
        } else {
            None
        };
    }
    let position = settings
        .sidebar_items
        .iter()
        .position(|i| i.kind == "project" && projects::key(&i.path) == old_key)
        .or_else(|| {
            settings
                .sidebar_items
                .iter()
                .position(|i| i.kind == "project" && belongs(&i.path))
        });
    let insertion = position.map(|n| {
        settings.sidebar_items[..n]
            .iter()
            .filter(|i| !(i.kind == "project" && belongs(&i.path)))
            .count()
    });
    settings
        .sidebar_items
        .retain(|i| !(i.kind == "project" && belongs(&i.path)));
    if let Some(item) = chosen {
        settings
            .sidebar_items
            .insert(insertion.unwrap_or(settings.sidebar_items.len()), item);
    }
    let model = settings
        .project_models
        .iter()
        .find(|m| projects::key(&m.project) == old_key)
        .or_else(|| {
            settings
                .project_models
                .iter()
                .find(|m| same_path(&m.project, new))
        })
        .cloned();
    settings.project_models.retain(|m| !belongs(&m.project));
    if let Some(mut model) = model {
        model.project = new.into();
        settings.project_models.push(model);
    }
    for list in [&mut settings.recent_projects, &mut settings.project_order] {
        // 先去掉目标入口，再改写旧入口，确保排序保留旧项目所在位置。
        let has_old = list.iter().any(|p| projects::key(p) == old_key);
        if has_old && old_key != new_key {
            list.retain(|p| !same_path(p, new));
        }
        let mut seen = HashSet::new();
        list.retain_mut(|path| {
            if belongs(path) {
                *path = new.into();
            }
            seen.insert(projects::key(path))
        });
    }
}

/// 在副本上准备最终状态；调用方必须先持久化恢复记录，再退休进程及原子提交。
pub fn apply(
    settings: &mut Settings,
    preview: &Preview,
    sessions: &[SessionSummary],
) -> Result<()> {
    if preview.target_id.is_none() {
        projects::relocate(settings, &preview.id, &preview.target, sessions)?;
        settings
            .projects
            .iter_mut()
            .find(|p| p.id == preview.id)
            .unwrap()
            .pending_merge = None;
        return Ok(());
    }
    let mut source = resolve(settings, &preview.id)?.clone();
    let old = source.current_path.clone();
    if let Some(target_id) = &preview.target_id {
        let target = resolve(settings, target_id)?.clone();
        source.merged_ids.push(target.id.clone());
        source.merged_ids.extend(target.merged_ids);
        source.historical_paths.extend(target.historical_paths);
        source.sessions.extend(target.sessions);
        settings.projects.retain(|p| p.id != target.id);
    }
    // 只补充仍有可靠原目录身份的会话，缺失旧目录的历史已由升级登记阶段明确归属。
    if source.directory_identity.is_some()
        && projects::identity(Path::new(&old)).ok() == source.directory_identity
    {
        source.sessions.extend(
            sessions
                .iter()
                .filter(|s| projects::key(&s.cwd) == projects::key(&old))
                .map(|s| s.path.clone()),
        );
    }
    source.historical_paths.push(old.clone());
    for values in [&mut source.sessions, &mut source.historical_paths] {
        let mut seen = HashSet::new();
        values.retain(|p| seen.insert(projects::key(p)));
    }
    source.merged_ids.sort();
    source.merged_ids.dedup();
    source.current_path = preview.target.clone();
    source.directory_identity = Some(preview.directory_identity.clone());
    source.pending_path = None;
    source.pending_merge = None;
    source.cancelled_target = None;
    *settings
        .projects
        .iter_mut()
        .find(|p| p.id == preview.id)
        .unwrap() = source;
    preferences(settings, &old, &preview.target);
    Ok(())
}
