//! 真正的原子配置提交验证：失败不改变内存或日志，成功保留并发修改的无关设置。
use super::*;
use std::fs;

fn state(config_dir: PathBuf, settings: Settings) -> AppState {
    AppState {
        config_dir,
        settings: Mutex::new(settings),
        catalog: Mutex::new(history::Catalog::default()),
        runtimes: Arc::new(supervisor::Supervisor::default()),
        sessions: Mutex::new(HashMap::new()),
        lifecycle: AsyncMutex::new(()),
        drops: Mutex::new(file_import::Drops::default()),
        file_reads: AsyncMutex::new(()),
        plugin_operations: AsyncMutex::new(()),
        project_operations: AsyncMutex::new(()),
        project_watch: Mutex::new(None),
    }
}

#[test]
fn failed_final_commit_retains_journal_then_retry_merges_latest_preferences_once() {
    let root = tempfile::tempdir().unwrap();
    let config = root.path().join("config");
    let old = root.path().join("old");
    let new = root.path().join("new");
    fs::create_dir(&old).unwrap();
    let old = old.to_string_lossy().into_owned();
    let new = new.to_string_lossy().into_owned();
    let mut settings = Settings::default();
    projects::register(&mut settings, std::slice::from_ref(&old));
    fs::rename(&old, &new).unwrap();
    projects::register(&mut settings, std::slice::from_ref(&new));
    let id = settings.projects[0].id.clone();
    let target_id = settings.projects[1].id.clone();
    settings.projects[0].sessions.push("original.jsonl".into());
    settings.projects[1].sessions.push("target.jsonl".into());
    settings.project_order = vec![old.clone(), new.clone()];
    settings.sidebar_items.push(SidebarItem {
        kind: "project".into(),
        path: old.clone(),
        name: Some("旧别名".into()),
        state: None,
    });
    let state = state(config.clone(), settings.clone());
    let checked = project_merge::preview(&settings, &id, &new).unwrap();
    settings.projects[0].pending_path = Some(checked.target.clone());
    settings.projects[0].pending_merge = Some(project_merge::PendingMerge {
        target_id: Some(target_id.clone()),
        directory_identity: checked.directory_identity.clone(),
        automatic: true,
        retiring: true,
    });
    commit(&state, &mut settings, None).unwrap();
    let journal = serde_json::to_value(state.config().unwrap()).unwrap();
    let mut final_settings = settings.clone();
    project_merge::apply(&mut final_settings, &checked, &[]).unwrap();
    final_settings.project_revision = settings.project_revision;
    // 用目录占住原子替换的目标，跨平台稳定制造真实写入失败，不依赖权限或模拟 save。
    fs::rename(config.join("settings.json"), config.join("saved.json")).unwrap();
    fs::create_dir(config.join("settings.json")).unwrap();
    assert!(commit(&state, &mut final_settings, Some((&old, &checked.target))).is_err());
    assert_eq!(
        serde_json::to_value(state.config().unwrap()).unwrap(),
        journal
    );
    let persisted: Settings =
        serde_json::from_slice(&fs::read(config.join("saved.json")).unwrap()).unwrap();
    assert!(
        persisted.projects[0]
            .pending_merge
            .as_ref()
            .unwrap()
            .retiring
    );
    assert_eq!(persisted.projects.len(), 2);
    fs::remove_dir(config.join("settings.json")).unwrap();
    fs::rename(config.join("saved.json"), config.join("settings.json")).unwrap();
    {
        let mut stored = state.settings.lock().unwrap();
        stored.theme = "dark".into();
        stored.sidebar_items[0].name = Some("等待时更新的别名".into());
    }
    commit(&state, &mut final_settings, Some((&old, &checked.target))).unwrap();
    let saved = settings::load(&config).unwrap();
    assert_eq!(saved.theme, "dark");
    assert_eq!(
        saved.sidebar_items[0].name.as_deref(),
        Some("等待时更新的别名")
    );
    assert_eq!(saved.projects.len(), 1);
    assert_eq!(
        saved.projects[0].sessions,
        vec!["original.jsonl", "target.jsonl"]
    );
    assert!(saved.projects[0].pending_merge.is_none());
    assert_eq!(project_merge::resolve(&saved, &target_id).unwrap().id, id);
    assert_eq!(saved.project_order, vec![checked.target]);
    // 同一个旧修订副本不能重复提交，也不能回滚新记录。
    assert!(commit(&state, &mut settings, None).is_err());
    assert_eq!(state.config().unwrap().projects.len(), 1);
}
