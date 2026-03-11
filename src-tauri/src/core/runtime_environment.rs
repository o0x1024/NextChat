use crate::core::domain::WorkGroup;

pub(crate) fn shell_runtime_label() -> &'static str {
    if cfg!(target_os = "windows") {
        "cmd /C"
    } else {
        "/bin/sh -lc"
    }
}

pub(crate) fn runtime_environment_lines(work_group: &WorkGroup) -> Vec<String> {
    vec![
        format!("Platform: {} ({})", std::env::consts::OS, std::env::consts::ARCH),
        format!("Shell execution: {}", shell_runtime_label()),
        format!("Working directory: {}", work_group.working_directory),
        "Filesystem scope: stay inside the working directory unless explicit permissions allow more.".into(),
    ]
}

pub(crate) fn runtime_environment_block(work_group: &WorkGroup) -> String {
    runtime_environment_lines(work_group)
        .into_iter()
        .map(|line| format!("- {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}
