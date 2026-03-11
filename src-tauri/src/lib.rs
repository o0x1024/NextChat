mod core;

use tauri::{Manager, State};

use crate::core::{
    domain::{
        AIProviderConfig, AgentProfile, CreateAgentInput, CreateWorkGroupInput, DashboardState,
        SendHumanMessageInput, SkillDetail, SkillPack, SystemSettings, TaskCard, ToolRun,
        UpdateAgentInput, UpdateSkillDetailInput, UpdateWorkGroupInput, WorkGroup,
    },
    llm_rig::test_connection,
    logging,
    service::AppService,
    workflow::{OwnerBlockerResolution, RaiseTaskBlockerInput, TaskBlockerRecord},
};

struct SharedState {
    service: AppService,
}

#[tauri::command]
async fn get_dashboard_state(state: State<'_, SharedState>) -> Result<DashboardState, String> {
    command_result("tauri.get_dashboard_state", state.service.dashboard_state())
}

#[tauri::command]
async fn create_agent_profile(
    state: State<'_, SharedState>,
    input: CreateAgentInput,
) -> Result<AgentProfile, String> {
    command_result(
        "tauri.create_agent_profile",
        state.service.create_agent_profile(input),
    )
}

#[tauri::command]
async fn generate_agent_profile(
    state: State<'_, SharedState>,
    prompt: String,
) -> Result<CreateAgentInput, String> {
    command_result(
        "tauri.generate_agent_profile",
        state.service.generate_agent_profile(&prompt).await,
    )
}

#[tauri::command]
async fn generate_agent_profiles(
    state: State<'_, SharedState>,
    prompt: String,
) -> Result<Vec<CreateAgentInput>, String> {
    command_result(
        "tauri.generate_agent_profiles",
        state.service.generate_agent_profiles(&prompt).await,
    )
}

#[tauri::command]
async fn update_agent_profile(
    state: State<'_, SharedState>,
    input: UpdateAgentInput,
) -> Result<AgentProfile, String> {
    command_result(
        "tauri.update_agent_profile",
        state.service.update_agent_profile(input),
    )
}

#[tauri::command]
async fn delete_agent_profile(state: State<'_, SharedState>, id: String) -> Result<(), String> {
    command_result(
        "tauri.delete_agent_profile",
        state.service.delete_agent_profile(&id),
    )
}

#[tauri::command]
async fn create_work_group(
    state: State<'_, SharedState>,
    input: CreateWorkGroupInput,
) -> Result<WorkGroup, String> {
    command_result(
        "tauri.create_work_group",
        state.service.create_work_group(input),
    )
}

#[tauri::command]
async fn delete_work_group(
    state: State<'_, SharedState>,
    work_group_id: String,
) -> Result<(), String> {
    command_result(
        "tauri.delete_work_group",
        state.service.delete_work_group(&work_group_id),
    )
}

#[tauri::command]
async fn clear_work_group_history(
    state: State<'_, SharedState>,
    work_group_id: String,
) -> Result<(), String> {
    command_result(
        "tauri.clear_work_group_history",
        state.service.clear_work_group_history(&work_group_id),
    )
}

#[tauri::command]
async fn update_work_group(
    state: State<'_, SharedState>,
    input: UpdateWorkGroupInput,
) -> Result<WorkGroup, String> {
    command_result(
        "tauri.update_work_group",
        state.service.update_work_group(input),
    )
}

#[tauri::command]
async fn add_agent_to_work_group(
    state: State<'_, SharedState>,
    work_group_id: String,
    agent_id: String,
) -> Result<WorkGroup, String> {
    command_result(
        "tauri.add_agent_to_work_group",
        state
            .service
            .add_agent_to_work_group(&work_group_id, &agent_id),
    )
}

#[tauri::command]
async fn remove_agent_from_work_group(
    state: State<'_, SharedState>,
    work_group_id: String,
    agent_id: String,
) -> Result<WorkGroup, String> {
    command_result(
        "tauri.remove_agent_from_work_group",
        state
            .service
            .remove_agent_from_work_group(&work_group_id, &agent_id),
    )
}

#[tauri::command]
async fn send_human_message(
    app: tauri::AppHandle,
    state: State<'_, SharedState>,
    input: SendHumanMessageInput,
) -> Result<(), String> {
    command_result(
        "tauri.send_human_message",
        state.service.send_human_message(app, input).map(|_| ()),
    )
}

#[tauri::command]
async fn list_task_cards(
    state: State<'_, SharedState>,
    work_group_id: Option<String>,
) -> Result<Vec<TaskCard>, String> {
    command_result(
        "tauri.list_task_cards",
        state.service.list_task_cards(work_group_id.as_deref()),
    )
}

#[tauri::command]
async fn approve_tool_run(
    app: tauri::AppHandle,
    state: State<'_, SharedState>,
    tool_run_id: String,
    approved: bool,
) -> Result<ToolRun, String> {
    command_result(
        "tauri.approve_tool_run",
        state.service.approve_tool_run(app, &tool_run_id, approved),
    )
}

#[tauri::command]
async fn cancel_task_card(
    app: tauri::AppHandle,
    state: State<'_, SharedState>,
    task_card_id: String,
) -> Result<(), String> {
    command_result(
        "tauri.cancel_task_card",
        state
            .service
            .cancel_task_card(app, &task_card_id)
            .map(|_| ()),
    )
}

#[tauri::command]
async fn raise_task_blocker(
    app: tauri::AppHandle,
    state: State<'_, SharedState>,
    task_id: String,
    blocker: RaiseTaskBlockerInput,
) -> Result<TaskBlockerRecord, String> {
    command_result(
        "tauri.raise_task_blocker",
        state.service.raise_task_blocker(app, &task_id, blocker),
    )
}

#[tauri::command]
async fn resolve_owner_blocker(
    app: tauri::AppHandle,
    state: State<'_, SharedState>,
    blocker_id: String,
    resolution: OwnerBlockerResolution,
) -> Result<(), String> {
    command_result(
        "tauri.resolve_owner_blocker",
        state
            .service
            .resolve_owner_blocker(app, &blocker_id, resolution),
    )
}

#[tauri::command]
async fn pause_lease(
    app: tauri::AppHandle,
    state: State<'_, SharedState>,
    lease_id: String,
) -> Result<(), String> {
    command_result(
        "tauri.pause_lease",
        state.service.pause_lease(app, &lease_id).map(|_| ()),
    )
}

#[tauri::command]
async fn resume_task_card(
    app: tauri::AppHandle,
    state: State<'_, SharedState>,
    task_card_id: String,
) -> Result<(), String> {
    command_result(
        "tauri.resume_task_card",
        state
            .service
            .resume_task_card(app, &task_card_id)
            .map(|_| ()),
    )
}

#[tauri::command]
async fn get_audit_events(
    state: State<'_, SharedState>,
    limit: Option<usize>,
) -> Result<Vec<crate::core::domain::AuditEvent>, String> {
    command_result(
        "tauri.get_audit_events",
        state.service.get_audit_events(limit),
    )
}

#[tauri::command]
async fn get_settings(state: State<'_, SharedState>) -> Result<SystemSettings, String> {
    command_result("tauri.get_settings", state.service.get_settings())
}

#[tauri::command]
async fn update_settings(
    state: State<'_, SharedState>,
    settings: SystemSettings,
) -> Result<(), String> {
    command_result(
        "tauri.update_settings",
        state.service.update_settings(settings),
    )
}

#[tauri::command]
async fn test_provider_connection(config: AIProviderConfig) -> Result<(), String> {
    command_result(
        "tauri.test_provider_connection",
        test_connection(&config).await,
    )
}

#[tauri::command]
async fn refresh_provider_models(
    state: State<'_, SharedState>,
    config: AIProviderConfig,
) -> Result<AIProviderConfig, String> {
    command_result(
        "tauri.refresh_provider_models",
        state.service.refresh_provider_models(config).await,
    )
}

#[tauri::command]
async fn install_skill_from_local(
    state: State<'_, SharedState>,
    source_path: String,
) -> Result<Vec<SkillPack>, String> {
    command_result(
        "tauri.install_skill_from_local",
        state.service.install_skill_from_local_path(&source_path),
    )
}

#[tauri::command]
async fn install_skill_from_github(
    state: State<'_, SharedState>,
    source: String,
    skill_path: Option<String>,
) -> Result<Vec<SkillPack>, String> {
    command_result(
        "tauri.install_skill_from_github",
        state
            .service
            .install_skill_from_github(&source, skill_path.as_deref())
            .await,
    )
}

#[tauri::command]
async fn update_installed_skill(
    state: State<'_, SharedState>,
    skill_id: String,
    name: Option<String>,
    prompt_template: Option<String>,
) -> Result<SkillPack, String> {
    command_result(
        "tauri.update_installed_skill",
        state
            .service
            .update_installed_skill(&skill_id, name, prompt_template),
    )
}

#[tauri::command]
async fn set_installed_skill_enabled(
    state: State<'_, SharedState>,
    skill_id: String,
    enabled: bool,
) -> Result<SkillPack, String> {
    command_result(
        "tauri.set_installed_skill_enabled",
        state
            .service
            .set_installed_skill_enabled(&skill_id, enabled),
    )
}

#[tauri::command]
async fn delete_installed_skill(
    state: State<'_, SharedState>,
    skill_id: String,
) -> Result<(), String> {
    command_result(
        "tauri.delete_installed_skill",
        state.service.delete_installed_skill(&skill_id),
    )
}

#[tauri::command]
async fn get_installed_skill_detail(
    state: State<'_, SharedState>,
    skill_id: String,
) -> Result<SkillDetail, String> {
    command_result(
        "tauri.get_installed_skill_detail",
        state.service.get_installed_skill_detail(&skill_id),
    )
}

#[tauri::command]
async fn update_skill_detail(
    state: State<'_, SharedState>,
    input: UpdateSkillDetailInput,
) -> Result<SkillDetail, String> {
    command_result(
        "tauri.update_skill_detail",
        state.service.update_skill_detail(input),
    )
}

#[tauri::command]
async fn read_installed_skill_file(
    state: State<'_, SharedState>,
    skill_id: String,
    relative_path: String,
) -> Result<String, String> {
    command_result(
        "tauri.read_installed_skill_file",
        state
            .service
            .read_installed_skill_file(&skill_id, &relative_path),
    )
}

#[tauri::command]
async fn upsert_installed_skill_file(
    state: State<'_, SharedState>,
    skill_id: String,
    relative_path: String,
    content: String,
) -> Result<(), String> {
    command_result(
        "tauri.upsert_installed_skill_file",
        state
            .service
            .upsert_installed_skill_file(&skill_id, &relative_path, &content),
    )
}

#[tauri::command]
async fn delete_installed_skill_file(
    state: State<'_, SharedState>,
    skill_id: String,
    relative_path: String,
) -> Result<(), String> {
    command_result(
        "tauri.delete_installed_skill_file",
        state
            .service
            .delete_installed_skill_file(&skill_id, &relative_path),
    )
}

fn command_result<T>(target: &str, result: anyhow::Result<T>) -> Result<T, String> {
    result.map_err(|error| {
        let message = error.to_string();
        logging::error(target, &message);
        message
    })
}

fn resolve_app_data_dir(app: &tauri::App) -> anyhow::Result<std::path::PathBuf> {
    Ok(app
        .path()
        .app_data_dir()
        .unwrap_or(std::env::current_dir()?.join(".nextchat-data")))
}

fn create_service(app: &tauri::App) -> anyhow::Result<AppService> {
    let workspace_root = std::env::current_dir()?;
    let app_data_dir = resolve_app_data_dir(app)?;
    logging::init(workspace_root.join("logs"))?;
    AppService::new(workspace_root, app_data_dir)
}

pub fn maybe_run_tool_worker_from_args() -> anyhow::Result<bool> {
    core::tool_worker::maybe_run_from_args(std::env::args_os())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .setup(|app| {
            let service = create_service(app).map_err(|error| {
                logging::error("app.setup", error.to_string());
                error
            })?;
            app.manage(SharedState { service });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_dashboard_state,
            create_agent_profile,
            generate_agent_profile,
            generate_agent_profiles,
            update_agent_profile,
            delete_agent_profile,
            create_work_group,
            delete_work_group,
            clear_work_group_history,
            update_work_group,
            add_agent_to_work_group,
            remove_agent_from_work_group,
            send_human_message,
            list_task_cards,
            approve_tool_run,
            cancel_task_card,
            raise_task_blocker,
            resolve_owner_blocker,
            pause_lease,
            resume_task_card,
            get_audit_events,
            get_settings,
            update_settings,
            test_provider_connection,
            refresh_provider_models,
            install_skill_from_local,
            install_skill_from_github,
            update_installed_skill,
            set_installed_skill_enabled,
            delete_installed_skill,
            get_installed_skill_detail,
            update_skill_detail,
            read_installed_skill_file,
            upsert_installed_skill_file,
            delete_installed_skill_file
        ])
        .run(tauri::generate_context!())
        .expect("failed to run tauri application");
}
