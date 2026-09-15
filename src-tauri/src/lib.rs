mod ai;
mod browser_bridge;
mod database;
mod diagnostics;
pub mod domain;
mod knowledge;
mod platform;
mod selection;
pub mod settings;
mod zhihu_search;

use tauri::Manager;

pub fn run() {
    let settings_state =
        settings::SettingsState::load_default().expect("failed to initialize ZhiForge settings");
    let diagnostic_log = diagnostics::DiagnosticLog::new_default()
        .expect("failed to initialize ZhiForge diagnostics");

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            platform::window::show_home(app);
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--autostart"]),
        ))
        .manage(ai::AiRuntime::new())
        .manage(diagnostic_log)
        .manage(settings_state)
        .invoke_handler(tauri::generate_handler![
            ai::start_ai_action,
            ai::start_routed_ai_action,
            ai::cancel_ai,
            ai::list_models,
            ai::model_capabilities,
            ai::load_api_key,
            ai::save_api_key,
            ai::load_provider_api_key,
            ai::save_provider_api_key,
            ai::migrate_legacy_api_key,
            diagnostics::diagnostics_log_path,
            database::knowledge_database_path,
            database::safety::database_safety_status,
            database::safety::database_backup_create,
            database::safety::database_backup_list,
            database::safety::database_backup_restore,
            database::safety::knowledge_export,
            database::safety::audit_log_list,
            browser_bridge::open_source_url,
            knowledge::knowledge_capture,
            knowledge::knowledge_list,
            knowledge::knowledge_get,
            knowledge::management::knowledge_library_list,
            knowledge::management::knowledge_management_detail,
            knowledge::management::knowledge_note_update,
            knowledge::management::topic_create,
            knowledge::management::topic_list,
            knowledge::management::topic_update,
            knowledge::management::topic_set_archived,
            knowledge::management::topic_assign,
            knowledge::management::topic_unassign,
            knowledge::management::tag_add,
            knowledge::management::tag_remove,
            knowledge::management::tag_list,
            knowledge::management::tag_update,
            knowledge::management::tag_delete_empty,
            knowledge::management::knowledge_relation_create,
            knowledge::management::knowledge_relation_list,
            knowledge::management::knowledge_relation_remove,
            knowledge::management::knowledge_archive,
            knowledge::management::knowledge_trash,
            knowledge::management::knowledge_restore,
            knowledge::management::knowledge_trash_list,
            knowledge::source::source_library_list,
            knowledge::source::source_get_detail,
            knowledge::graph::knowledge_graph,
            knowledge::health::knowledge_health,
            knowledge::inbox::knowledge_inbox_list,
            knowledge::inbox::knowledge_inbox_get,
            knowledge::inbox::knowledge_inbox_draft_update,
            knowledge::inbox::knowledge_inbox_accept,
            knowledge::inbox::knowledge_inbox_ignore,
            knowledge::inbox::knowledge_inbox_restore,
            knowledge::semantic::knowledge_expand_query,
            knowledge::semantic::knowledge_semantic_search,
            knowledge::librarian::librarian_analyze,
            knowledge::librarian::librarian_latest,
            knowledge::librarian::librarian_proposal_apply,
            knowledge::librarian::librarian_proposal_dismiss,
            knowledge::curation::curation_analyze,
            knowledge::curation::curation_latest,
            knowledge::curation::curation_candidate_apply,
            knowledge::curation::curation_candidate_dismiss,
            knowledge::merge::duplicate_merge_preview,
            knowledge::merge::duplicate_merge_apply,
            knowledge::merge::duplicate_merge_revert,
            knowledge::assessment::knowledge_attempt_create,
            knowledge::assessment::knowledge_attempt_list,
            knowledge::assessment::knowledge_attempt_judge,
            knowledge::review::knowledge_review_state,
            knowledge::review::knowledge_review_queue,
            knowledge::review::knowledge_review_session_start,
            knowledge::review::knowledge_review_session_current,
            knowledge::review::knowledge_review_session_abandon,
            knowledge::review::knowledge_review_session_history,
            knowledge::review::knowledge_review_plan_preview,
            knowledge::review::knowledge_review_plan_apply,
            knowledge::research::research_plan_analyze,
            knowledge::research::research_plan_latest,
            knowledge::research::research_plan_get,
            knowledge::research::research_plan_list,
            knowledge::research::research_gap_source_list,
            knowledge::research::research_gap_set_status,
            knowledge::research::research_gap_link_source,
            knowledge::task_center::knowledge_task_list,
            knowledge::today::knowledge_today,
            knowledge::internalization::knowledge_get_detail,
            knowledge::internalization::knowledge_generate_questions,
            knowledge::internalization::knowledge_internalize,
            zhihu_search::zhihu_access_secret_configured,
            zhihu_search::zhihu_save_access_secret,
            zhihu_search::zhihu_open_search,
            zhihu_search::zhihu_search,
            zhihu_search::zhihu_global_search,
            settings::settings_get,
            settings::settings_replace,
            settings::settings_migrate_legacy,
            selection::open_action,
            selection::hide_result,
            selection::set_result_pinned,
            selection::write_to_clipboard,
            selection::runtime_pause_selection,
            selection::runtime_resume_selection,
            selection::runtime_selection_paused,
            platform::autostart::autostart_is_enabled,
            platform::autostart::autostart_set_enabled,
        ])
        .setup(|app| {
            let database =
                database::DatabaseState::initialize(app.handle()).map_err(std::io::Error::other)?;
            app.manage(database);
            browser_bridge::start(app.handle());
            selection::start(app.handle().clone())?;
            platform::tray::setup(app)?;
            if !platform::window::launched_from_autostart() {
                platform::window::show_home(app.handle());
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("failed to run ZhiForge");
}
