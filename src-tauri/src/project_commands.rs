//! 项目关联事务：扫描在阻塞池、进程操作在异步层、配置提交使用修订号。
use super::*;
use serde::Deserialize;

#[cfg(test)]
mod tests;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Association {
    id: String,
    target: String,
    revision: u64,
    #[serde(default)]
    merge: bool,
    directory_identity: Option<String>,
    target_id: Option<String>,
}

#[tauri::command]
pub async fn browse_project_directories(path: String) -> Result<projects::DirectoryListing> {
    tokio::task::spawn_blocking(move || projects::browse(&path))
        .await
        .map_err(|_| AppError::new("PROJECT", "目录读取任务异常"))?
}

/// 只合并项目字段，避免扫描或关闭进程期间覆盖其他设置操作。
fn commit(state: &AppState, next: &mut Settings, moved: Option<(&str, &str)>) -> Result<()> {
    let mut stored = state
        .settings
        .lock()
        .map_err(|_| AppError::new("LOCK", "配置锁异常"))?;
    if stored.project_revision != next.project_revision {
        return Err(AppError::new(
            "PROJECT_REVISION",
            "项目配置已更新，请重新检查",
        ));
    }
    let mut merged = stored.clone();
    merged.projects = next.projects.clone();
    merged.project_revision += 1;
    if let Some((old, new)) = moved {
        let merging = next.projects.iter().any(|p| {
            p.current_path == new
                && stored.projects.iter().any(|old_record| {
                    old_record.id != p.id && p.merged_ids.contains(&old_record.id)
                })
        });
        if merging {
            project_merge::preferences(&mut merged, old, new);
        } else {
            let replace = |path: &mut String| {
                if projects::key(path) == projects::key(old) {
                    *path = new.to_owned();
                }
            };
            merged.recent_projects.iter_mut().for_each(replace);
            merged.project_order.iter_mut().for_each(replace);
            for item in &mut merged.sidebar_items {
                if item.kind == "project" {
                    replace(&mut item.path);
                }
            }
            for model in &mut merged.project_models {
                replace(&mut model.project);
            }
        }
    }
    settings::save(&state.config_dir, &merged)?;
    *stored = merged.clone();
    *next = merged;
    Ok(())
}

async fn preview_location(
    settings: &Settings,
    id: &str,
    target: &str,
) -> Result<project_merge::Preview> {
    let settings = settings.clone();
    let id = id.to_owned();
    let target = target.to_owned();
    // 网络盘或系统目录查询可能阻塞，不能占用异步调度线程。
    tokio::task::spawn_blocking(move || project_merge::preview(&settings, &id, &target))
        .await
        .map_err(|_| AppError::new("PROJECT", "目录检查任务异常"))?
}

/// 自动检查与手动关联共享事务；恢复目标先落盘，进程退休后再原子提交新路径。
#[tauri::command]
pub async fn refresh_project_locations(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    paths: Vec<String>,
    association: Option<Association>,
) -> Result<projects::ProjectLocations> {
    let _operation = state.project_operations.lock().await;
    let _lifecycle = state.lifecycle.lock().await;
    let original = state.config()?;
    // 网络重试或页面恢复不能再次合并已吸收的对象，也不重新写入相同等待事务。
    let association = association.filter(|request| {
        !project_merge::resolve(&original, &request.id).is_ok_and(|p| {
            let completed = request
                .target_id
                .as_ref()
                .is_some_and(|id| p.merged_ids.contains(id))
                && projects::key(&p.current_path) == projects::key(&request.target)
                && p.directory_identity == request.directory_identity;
            let waiting = p
                .pending_path
                .as_ref()
                .is_some_and(|target| projects::key(target) == projects::key(&request.target))
                && p.pending_merge.as_ref().is_some_and(|pending| {
                    Some(&pending.directory_identity) == request.directory_identity.as_ref()
                        && pending.target_id == request.target_id
                });
            completed || waiting
        })
    });
    if association
        .as_ref()
        .is_some_and(|request| request.revision != original.project_revision)
    {
        return Err(AppError::new(
            "PROJECT_REVISION",
            "项目目录已更新，请重新检查",
        ));
    }
    let scan_app = app.clone();
    let (mut next, sessions, changed) = tokio::task::spawn_blocking(move || {
        let state = scan_app.state::<AppState>();
        let mut settings = original;
        let sessions = state
            .catalog
            .lock()
            .map_err(|_| AppError::new("LOCK", "历史索引锁异常"))?
            .list(&settings, "", "")?;
        let mut paths = paths;
        paths.extend(settings.recent_projects.iter().cloned());
        paths.extend(settings.project_order.iter().cloned());
        paths.extend(
            settings
                .sidebar_items
                .iter()
                .filter(|i| i.kind == "project")
                .map(|i| i.path.clone()),
        );
        let owners = projects::session_owners(&settings);
        paths.extend(sessions.iter().map(|s| {
            owners
                .get(&projects::key(&s.path))
                .cloned()
                .unwrap_or_else(|| s.cwd.clone())
        }));
        let mut by_project: HashMap<String, Vec<&SessionSummary>> = HashMap::new();
        for session in &sessions {
            by_project
                .entry(projects::key(&session.cwd))
                .or_default()
                .push(session);
        }
        let known_ids: std::collections::HashSet<_> =
            settings.projects.iter().map(|p| p.id.clone()).collect();
        let mut owned: std::collections::HashSet<_> = owners.into_keys().collect();
        let mut changed = projects::register(&mut settings, &paths);
        for project in &mut settings.projects {
            let actual = projects::identity(std::path::Path::new(&project.current_path)).ok();
            // 首次升级可绑定既有历史；后续仅在目录身份一致时接纳尚未归属的新会话。
            if !known_ids.contains(&project.id)
                || (actual.is_some() && actual == project.directory_identity)
            {
                for session in by_project
                    .get(&projects::key(&project.current_path))
                    .into_iter()
                    .flatten()
                {
                    if owned.insert(projects::key(&session.path)) {
                        project.sessions.push(session.path.clone());
                        changed = true;
                    }
                }
            }
            if project.directory_identity.is_none() && actual.is_some() {
                project.directory_identity = actual;
                changed = true;
            }
        }
        Ok::<_, AppError>((settings, sessions, changed))
    })
    .await
    .map_err(|_| AppError::new("PROJECT", "项目扫描任务异常"))??;
    if changed {
        commit(&state, &mut next, None)?;
    }
    if let Some(request) = association {
        let checked = preview_location(&next, &request.id, &request.target).await?;
        if checked.target_id.is_some() && !request.merge {
            return Err(AppError::new(
                "PROJECT_CONFLICT",
                "此目录已有项目，可合并到当前项目",
            ));
        }
        if request
            .directory_identity
            .as_ref()
            .is_some_and(|id| *id != checked.directory_identity)
            || (request.merge && request.target_id != checked.target_id)
        {
            return Err(AppError::new(
                "PROJECT_REVISION",
                "目录信息已变化，请重新检查",
            ));
        }
        project_merge::capture_manual(&mut next, &checked.id, &sessions)?;
        let source = next
            .projects
            .iter_mut()
            .find(|p| p.id == checked.id)
            .unwrap();
        if source.pending_merge.as_ref().is_some_and(|p| p.retiring) {
            return Err(AppError::new("PROJECT_PENDING", "正在完成目录更新，请稍候"));
        }
        source.pending_path = Some(checked.target.clone());
        source.cancelled_target = None;
        source.pending_merge = Some(project_merge::PendingMerge {
            target_id: checked.target_id,
            directory_identity: checked.directory_identity,
            automatic: false,
            retiring: false,
        });
        commit(&state, &mut next, None)?;
    }
    let records = next.projects.clone();
    let mut candidates = state
        .project_watch
        .lock()
        .ok()
        .and_then(|watch| watch.as_ref().map(|watch| watch.candidates()))
        .unwrap_or_default();
    // 已登记的新路径也是有限且可核验的候选，支持监听错过但用户已新建项目的情况。
    candidates.extend(next.projects.iter().map(|p| PathBuf::from(&p.current_path)));
    let mut locations = Vec::new();
    for record in records {
        // 较早的记录可能已经吸收本轮快照中的目标，不能再次生成其状态或冻结连接。
        if !next.projects.iter().any(|p| p.id == record.id) {
            continue;
        }
        let checked = record.clone();
        let candidates = candidates.clone();
        let mut location = tokio::task::spawn_blocking(move || {
            let mut location = projects::inspect(&checked);
            if location.state == "unavailable" {
                if let Some(identity) = &checked.directory_identity {
                    let mut matches: Vec<_> = candidates
                        .iter()
                        .filter(|path| projects::identity(path).is_ok_and(|id| id == *identity))
                        .map(|p| p.to_string_lossy().into_owned())
                        .collect();
                    matches.sort();
                    matches.dedup();
                    if matches.len() == 1 {
                        location.candidate = matches.pop();
                        location.state = "relocating".into();
                        location.message = "目录已变更，当前任务结束后切换".into();
                    }
                }
            }
            location
        })
        .await
        .map_err(|_| AppError::new("PROJECT", "目录检查任务异常"))?;
        state
            .runtimes
            .block_project(&record.current_path, location.state != "available");
        if let Some(target) = location.candidate.clone() {
            let attempt: Result<()> = async {
                let prepared = preview_location(&next, &record.id, &target).await;
                match prepared {
                    Ok(preview) => {
                        let valid = project_merge::valid_pending(&record, &preview);
                        if !valid {
                            location.state = "unavailable".into();
                            location.candidate = None;
                            location.message = "需要关联目录".into();
                        } else {
                            let source = next
                                .projects
                                .iter_mut()
                                .find(|p| p.id == record.id)
                                .unwrap();
                            if source.pending_merge.is_none() {
                                source.pending_path = Some(preview.target.clone());
                                source.pending_merge = Some(project_merge::PendingMerge {
                                    target_id: preview.target_id.clone(),
                                    directory_identity: preview.directory_identity.clone(),
                                    // 旧版本已有 pending_path 是用户确认过的关联；其他候选必须保留自动证据。
                                    automatic: record.pending_path.is_none(),
                                    retiring: false,
                                });
                                commit(&state, &mut next, None)?;
                            }
                            if state.runtimes.can_relocate_project(&record.current_path) {
                                let live_sessions = state
                                    .runtimes
                                    .project_sessions(&record.current_path)
                                    .await?;
                                let source = next
                                    .projects
                                    .iter_mut()
                                    .find(|p| p.id == record.id)
                                    .unwrap();
                                let mut known: std::collections::HashSet<_> =
                                    source.sessions.iter().map(|p| projects::key(p)).collect();
                                source.sessions.extend(
                                    live_sessions
                                        .into_iter()
                                        .filter(|p| known.insert(projects::key(p))),
                                );
                                // 退休前再次读取目录身份；预览与等待期间的替换不能沿用旧证据。
                                let verified =
                                    preview_location(&next, &record.id, &preview.target).await?;
                                let pending_record =
                                    next.projects.iter().find(|p| p.id == record.id).unwrap();
                                if !project_merge::valid_pending(pending_record, &verified) {
                                    location.state = "unavailable".into();
                                    location.candidate = None;
                                    location.message = "需要关联目录".into();
                                    return Ok(());
                                }
                                next.projects
                                    .iter_mut()
                                    .find(|p| p.id == record.id)
                                    .unwrap()
                                    .pending_merge
                                    .as_mut()
                                    .unwrap()
                                    .retiring = true;
                                commit(&state, &mut next, None)?;
                                let mut relocated = next.clone();
                                project_merge::apply(&mut relocated, &preview, &sessions)?;
                                relocated.project_revision = next.project_revision;
                                if state
                                    .runtimes
                                    .relocate_project(
                                        &record.current_path,
                                        &preview.target,
                                        &relocated,
                                    )
                                    .await?
                                {
                                    let bindings = state.runtimes.queued_session_bindings();
                                    let mut normalized = Vec::new();
                                    for (path, runtime) in bindings {
                                        normalized.push((session_identity(&path)?, runtime));
                                    }
                                    state.sessions.lock().unwrap().extend(normalized);
                                    let final_check =
                                        preview_location(&next, &record.id, &preview.target)
                                            .await?;
                                    let pending_record =
                                        next.projects.iter().find(|p| p.id == record.id).unwrap();
                                    if !project_merge::valid_pending(pending_record, &final_check) {
                                        return Err(AppError::new(
                                            "PROJECT_REVISION",
                                            "目标目录已变化，更新暂未提交",
                                        ));
                                    }
                                    commit(
                                        &state,
                                        &mut relocated,
                                        Some((&record.current_path, &preview.target)),
                                    )?;
                                    next = relocated;
                                    state.runtimes.block_project(&record.current_path, false);
                                    state.runtimes.block_project(&preview.target, false);
                                    location.current_path = preview.target;
                                    location.state = "available".into();
                                    location.candidate = None;
                                    location.message = String::new();
                                }
                            }
                        }
                    }
                    Err(error) => {
                        location.state = "unavailable".into();
                        location.candidate = None;
                        location.message = if error.code == "PROJECT_AMBIGUOUS" {
                            error.message
                        } else {
                            "需要关联目录".into()
                        };
                    }
                }
                Ok(())
            }
            .await;
            if let Err(error) = attempt {
                // 自动操作失败留在对应项目；重新读取已落盘阶段，不能把副本误报为提交成功。
                next = state.config()?;
                location.state = "unavailable".into();
                location.message = "目录更新未完成，可重试关联".into();
                location.error = Some(error);
            }
        }
        locations.push(location);
    }
    locations.retain(|location| next.projects.iter().any(|p| p.id == location.id));
    state.runtimes.set_limit(next.max_concurrent_tasks as usize);
    {
        let mut watch = state
            .project_watch
            .lock()
            .map_err(|_| AppError::new("LOCK", "项目监听锁异常"))?;
        if watch.is_none() {
            *watch = projects::ProjectWatch::new(app.clone()).ok();
        }
        if let Some(watch) = watch.as_mut() {
            watch.sync(&next.projects);
        }
    }
    Ok(projects::ProjectLocations {
        settings: next,
        locations,
    })
}

/// 只读预览使用快照和阻塞池，输入中的路径不触发注册、保存或进程操作。
#[tauri::command]
pub async fn preview_project_merge(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: String,
    target: String,
) -> Result<project_merge::Preview> {
    let mut settings = state.config()?;
    tokio::task::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let sessions = state
            .catalog
            .lock()
            .map_err(|_| AppError::new("LOCK", "历史索引锁异常"))?
            .list(&settings, "", "")?;
        // 仅在预览副本中计数，不保存归属；人工确认时重新建立这份集合。
        project_merge::capture_manual(&mut settings, &id, &sessions)?;
        project_merge::preview(&settings, &id, &target)
    })
    .await
    .map_err(|_| AppError::new("PROJECT", "目录检查任务异常"))?
}

#[tauri::command]
pub async fn merge_projects(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    mut request: Association,
) -> Result<projects::ProjectLocations> {
    if request.directory_identity.is_none() {
        return Err(AppError::new("PROJECT_REVISION", "请先检查目标目录"));
    }
    request.merge = true;
    refresh_project_locations(app, state, vec![], Some(request)).await
}

/// 只允许取消尚未退休的事务；取消目标记入记录，防止周期检查立即重新排入。
#[tauri::command]
pub async fn cancel_project_merge(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: String,
    revision: u64,
) -> Result<projects::ProjectLocations> {
    {
        let _operation = state.project_operations.lock().await;
        let _lifecycle = state.lifecycle.lock().await;
        let mut next = state.config()?;
        if next.project_revision != revision {
            return Err(AppError::new(
                "PROJECT_REVISION",
                "项目信息已变化，请重新检查",
            ));
        }
        let source_id = project_merge::resolve(&next, &id)?.id.clone();
        let source = next
            .projects
            .iter_mut()
            .find(|p| p.id == source_id)
            .unwrap();
        if source.pending_merge.as_ref().is_some_and(|p| p.retiring) {
            return Err(AppError::new(
                "PROJECT_PENDING",
                "正在完成目录更新，暂时不能取消",
            ));
        }
        source.cancelled_target = source.pending_path.take();
        source.pending_merge = None;
        let old = source.current_path.clone();
        commit(&state, &mut next, None)?;
        state.runtimes.block_project(&old, false);
    }
    refresh_project_locations(app, state, vec![], None).await
}
