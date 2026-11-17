use super::*;

fn setup() -> (tempfile::TempDir, Settings, String, String) {
    let root = tempfile::tempdir().unwrap();
    let old = root.path().join("旧 项目");
    let new = root.path().join("新 项目");
    fs::create_dir(&old).unwrap();
    let old = old.to_string_lossy().into_owned();
    let new = new.to_string_lossy().into_owned();
    let mut settings = Settings::default();
    projects::register(&mut settings, std::slice::from_ref(&old));
    fs::rename(&old, &new).unwrap();
    projects::register(&mut settings, std::slice::from_ref(&new));
    (root, settings, old, new)
}

#[test]
fn reliable_identity_merges_sessions_preferences_and_stable_ids() {
    let (_root, mut settings, old, new) = setup();
    let id = settings.projects[0].id.clone();
    let target_id = settings.projects[1].id.clone();
    settings.projects[0].sessions = vec!["old.jsonl".into(), "shared.jsonl".into()];
    settings.projects[1].sessions = vec!["new.jsonl".into(), "shared.jsonl".into()];
    settings.projects[1].merged_ids = vec!["previous-target".into()];
    settings.project_order = vec![new.clone(), "/unrelated".into(), old.clone()];
    settings.recent_projects = vec![old.clone(), new.clone()];
    settings.sidebar_items = vec![
        SidebarItem {
            kind: "project".into(),
            path: old.clone(),
            name: Some("旧别名".into()),
            state: Some("archived".into()),
        },
        SidebarItem {
            kind: "project".into(),
            path: new.clone(),
            name: Some("新别名".into()),
            state: None,
        },
        SidebarItem {
            kind: "session".into(),
            path: "old.jsonl".into(),
            name: Some("会话别名".into()),
            state: Some("archived".into()),
        },
    ];
    for (path, model) in [(&old, "old-model"), (&new, "new-model")] {
        settings.project_models.push(ProjectModelPreference {
            project: path.clone(),
            provider: "provider".into(),
            model: model.into(),
            thinking_level: "high".into(),
        });
    }
    let checked = preview(&settings, &id, &new).unwrap();
    assert!(checked.automatic);
    assert_eq!(checked.target_id.as_ref(), Some(&target_id));
    assert_eq!(checked.alias.as_deref(), Some("旧别名"));
    assert!(!checked.archived);
    apply(&mut settings, &checked, &[]).unwrap();
    assert_eq!(settings.projects.len(), 1);
    assert_eq!(resolve(&settings, &target_id).unwrap().id, id);
    assert_eq!(resolve(&settings, "previous-target").unwrap().id, id);
    assert_eq!(
        settings.projects[0].sessions,
        vec!["old.jsonl", "shared.jsonl", "new.jsonl"]
    );
    assert_eq!(
        settings.project_order,
        vec!["/unrelated".to_string(), checked.target.clone()]
    );
    assert_eq!(settings.recent_projects, vec![checked.target.clone()]);
    assert_eq!(settings.project_models.len(), 1);
    assert_eq!(settings.project_models[0].model, "old-model");
    assert_eq!(settings.sidebar_items.len(), 2);
    assert_eq!(settings.sidebar_items[0].name.as_deref(), Some("旧别名"));
    assert_eq!(settings.sidebar_items[0].state, None);
    assert_eq!(settings.sidebar_items[1].name.as_deref(), Some("会话别名"));
    assert_eq!(settings.sidebar_items[1].state.as_deref(), Some("archived"));
    fs::create_dir(&old).unwrap();
    assert_eq!(
        projects::session_project(&settings, "unknown.jsonl", &old),
        old
    );
    assert!(same_path(
        projects::session_project(&settings, "old.jsonl", &old),
        &new
    ));
    assert!(preview(&settings, &target_id, &new)
        .unwrap()
        .target_id
        .is_none());
}

#[test]
fn missing_identity_and_reused_target_require_manual_confirmation() {
    let (root, mut settings, old, new) = setup();
    let id = settings.projects[0].id.clone();
    settings.projects[0].directory_identity = None;
    assert!(!preview(&settings, &id, &new).unwrap().automatic);
    settings.projects[0].directory_identity = settings.projects[1].directory_identity.clone();
    fs::rename(&new, root.path().join("saved")).unwrap();
    fs::create_dir(&new).unwrap();
    let checked = preview(&settings, &id, &new).unwrap();
    assert!(!checked.automatic);
    // 显式人工操作仍能合并不同目录身份，不改变文件系统或猜测未来旧路径归属。
    apply(&mut settings, &checked, &[]).unwrap();
    assert!(Path::new(&new).is_dir());
    assert!(!Path::new(&old).exists());
}

#[test]
fn previews_do_not_write_and_persisted_journal_retains_evidence_and_phase() {
    let (_root, mut settings, _old, new) = setup();
    let before = serde_json::to_value(&settings).unwrap();
    let checked = preview(&settings, &settings.projects[0].id, &new).unwrap();
    assert_eq!(serde_json::to_value(&settings).unwrap(), before);
    settings.projects[0].pending_path = Some(checked.target);
    settings.projects[0].pending_merge = Some(PendingMerge {
        target_id: checked.target_id,
        directory_identity: checked.directory_identity,
        automatic: true,
        retiring: true,
    });
    let restored: Settings =
        serde_json::from_value(serde_json::to_value(&settings).unwrap()).unwrap();
    let pending = restored.projects[0].pending_merge.as_ref().unwrap();
    assert!(pending.retiring && pending.automatic);
    assert_eq!(pending.target_id.as_ref(), Some(&settings.projects[1].id));
}

#[test]
fn ambiguous_owners_are_not_chosen_automatically() {
    let (_root, mut settings, _old, new) = setup();
    let mut duplicate = settings.projects[1].clone();
    duplicate.id = "other".into();
    settings.projects.push(duplicate);
    let error = preview(&settings, &settings.projects[0].id, &new)
        .err()
        .unwrap();
    assert_eq!(error.code, "PROJECT_AMBIGUOUS");
}

#[test]
fn fallback_preferences_and_both_archived_are_preserved() {
    let (_root, mut settings, old, new) = setup();
    for path in [&old, &new] {
        settings.sidebar_items.push(SidebarItem {
            kind: "project".into(),
            path: path.clone(),
            name: None,
            state: Some("archived".into()),
        });
    }
    settings.sidebar_items[1].name = Some("目标别名".into());
    settings.project_models.push(ProjectModelPreference {
        project: new.clone(),
        provider: "p".into(),
        model: "fallback".into(),
        thinking_level: "low".into(),
    });
    let checked = preview(&settings, &settings.projects[0].id, &new).unwrap();
    assert!(checked.archived);
    assert_eq!(checked.alias.as_deref(), Some("目标别名"));
    apply(&mut settings, &checked, &[]).unwrap();
    assert_eq!(settings.sidebar_items[0].state.as_deref(), Some("archived"));
    assert_eq!(settings.project_models[0].model, "fallback");
}

#[test]
fn cancel_wait_and_restart_revalidate_target_and_automatic_evidence() {
    let (_root, mut settings, _old, new) = setup();
    let checked = preview(&settings, &settings.projects[0].id, &new).unwrap();
    let record = &mut settings.projects[0];
    assert!(valid_pending(record, &checked));
    record.cancelled_target = Some(checked.target.clone());
    assert!(!valid_pending(record, &checked));
    record.pending_path = Some(checked.target.clone());
    record.pending_merge = Some(PendingMerge {
        target_id: checked.target_id.clone(),
        directory_identity: checked.directory_identity.clone(),
        automatic: true,
        retiring: false,
    });
    assert!(valid_pending(record, &checked));
    record.directory_identity = None;
    let changed = preview(&settings, &settings.projects[0].id, &new).unwrap();
    assert!(!valid_pending(&settings.projects[0], &changed));
    settings.projects[0]
        .pending_merge
        .as_mut()
        .unwrap()
        .automatic = false;
    assert!(valid_pending(&settings.projects[0], &changed));
    settings.projects[0]
        .pending_merge
        .as_mut()
        .unwrap()
        .directory_identity = "replaced".into();
    assert!(!valid_pending(&settings.projects[0], &changed));
}

#[test]
fn explicit_manual_group_claims_legacy_history_without_taking_owned_files() {
    let (_root, mut settings, old, _new) = setup();
    let id = settings.projects[0].id.clone();
    settings.projects[1].sessions.push("owned.jsonl".into());
    let sessions: Vec<SessionSummary> = ["legacy.jsonl", "owned.jsonl"].into_iter().map(|path| serde_json::from_value(serde_json::json!({
        "path":path,"id":path,"cwd":old,"name":"","preview":"","timestamp":"","modified":0,"lastPromptAt":"","warning":null
    })).unwrap()).collect();
    capture_manual(&mut settings, &id, &sessions).unwrap();
    capture_manual(&mut settings, &id, &sessions).unwrap();
    assert_eq!(settings.projects[0].sessions, vec!["legacy.jsonl"]);
    assert_eq!(settings.projects[1].sessions, vec!["owned.jsonl"]);
}

#[test]
fn target_with_its_own_pending_transaction_cannot_be_absorbed() {
    let (_root, mut settings, _old, new) = setup();
    settings.projects[1].pending_path = Some("/another-directory".into());
    assert_eq!(
        preview(&settings, &settings.projects[0].id, &new)
            .err()
            .unwrap()
            .code,
        "PROJECT_PENDING"
    );
}
