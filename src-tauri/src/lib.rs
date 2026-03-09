mod core;

use tauri::{Manager, State};

use crate::core::{
    domain::{
        AIProviderConfig, AgentProfile, CreateAgentInput, CreateWorkGroupInput, DashboardState,
        SendHumanMessageInput, SystemSettings, TaskCard, ToolRun, UpdateAgentInput, WorkGroup,
    },
    llm_rig::test_connection,
    service::AppService,
};

struct SharedState {
    service: AppService,
}

#[tauri::command]
async fn get_dashboard_state(state: State<'_, SharedState>) -> Result<DashboardState, String> {
    state
        .service
        .dashboard_state()
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn create_agent_profile(
    state: State<'_, SharedState>,
    input: CreateAgentInput,
) -> Result<AgentProfile, String> {
    state
        .service
        .create_agent_profile(input)
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn update_agent_profile(
    state: State<'_, SharedState>,
    input: UpdateAgentInput,
) -> Result<AgentProfile, String> {
    state
        .service
        .update_agent_profile(input)
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn delete_agent_profile(
    state: State<'_, SharedState>,
    id: String,
) -> Result<(), String> {
    state
        .service
        .delete_agent_profile(&id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn create_work_group(
    state: State<'_, SharedState>,
    input: CreateWorkGroupInput,
) -> Result<WorkGroup, String> {
    state
        .service
        .create_work_group(input)
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn add_agent_to_work_group(
    state: State<'_, SharedState>,
    work_group_id: String,
    agent_id: String,
) -> Result<WorkGroup, String> {
    state
        .service
        .add_agent_to_work_group(&work_group_id, &agent_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn remove_agent_from_work_group(
    state: State<'_, SharedState>,
    work_group_id: String,
    agent_id: String,
) -> Result<WorkGroup, String> {
    state
        .service
        .remove_agent_from_work_group(&work_group_id, &agent_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn send_human_message(
    app: tauri::AppHandle,
    state: State<'_, SharedState>,
    input: SendHumanMessageInput,
) -> Result<(), String> {
    state
        .service
        .send_human_message(app, input)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn list_task_cards(
    state: State<'_, SharedState>,
    work_group_id: Option<String>,
) -> Result<Vec<TaskCard>, String> {
    state
        .service
        .list_task_cards(work_group_id.as_deref())
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn approve_tool_run(
    app: tauri::AppHandle,
    state: State<'_, SharedState>,
    tool_run_id: String,
    approved: bool,
) -> Result<ToolRun, String> {
    state
        .service
        .approve_tool_run(app, &tool_run_id, approved)
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn cancel_task_card(
    app: tauri::AppHandle,
    state: State<'_, SharedState>,
    task_card_id: String,
) -> Result<(), String> {
    state
        .service
        .cancel_task_card(app, &task_card_id)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn pause_lease(
    app: tauri::AppHandle,
    state: State<'_, SharedState>,
    lease_id: String,
) -> Result<(), String> {
    state
        .service
        .pause_lease(app, &lease_id)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn resume_task_card(
    app: tauri::AppHandle,
    state: State<'_, SharedState>,
    task_card_id: String,
) -> Result<(), String> {
    state
        .service
        .resume_task_card(app, &task_card_id)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn get_audit_events(
    state: State<'_, SharedState>,
    limit: Option<usize>,
) -> Result<Vec<crate::core::domain::AuditEvent>, String> {
    state
        .service
        .get_audit_events(limit)
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn get_settings(
    state: State<'_, SharedState>,
) -> Result<SystemSettings, String> {
    state
        .service
        .get_settings()
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn update_settings(
    state: State<'_, SharedState>,
    settings: SystemSettings,
) -> Result<(), String> {
    state
        .service
        .update_settings(settings)
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn test_provider_connection(
    config: AIProviderConfig,
) -> Result<(), String> {
    test_connection(&config)
        .await
        .map_err(|error| error.to_string())
}

fn create_service(app: &tauri::App) -> anyhow::Result<AppService> {
    let workspace_root = std::env::current_dir()?;
    let app_data_dir = app
        .path()
        .app_data_dir()
        .unwrap_or(std::env::current_dir()?.join(".nextchat-data"));
    AppService::new(workspace_root, app_data_dir)
}

pub fn maybe_run_tool_worker_from_args() -> anyhow::Result<bool> {
    core::tool_worker::maybe_run_from_args(std::env::args_os())
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let service = create_service(app)?;
            app.manage(SharedState { service });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_dashboard_state,
            create_agent_profile,
            update_agent_profile,
            delete_agent_profile,
            create_work_group,
            add_agent_to_work_group,
            remove_agent_from_work_group,
            send_human_message,
            list_task_cards,
            approve_tool_run,
            cancel_task_card,
            pause_lease,
            resume_task_card,
            get_audit_events,
            get_settings,
            update_settings,
            test_provider_connection
        ])
        .run(tauri::generate_context!())
        .expect("failed to run tauri application");
}
