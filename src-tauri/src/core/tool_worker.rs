use std::{
    ffi::OsString,
    io::{Read, Write},
    path::PathBuf,
    process::{Command, Stdio},
};

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellWorkerRequest {
    pub workspace_root: String,
    pub command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellWorkerResponse {
    pub command: String,
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

pub fn maybe_run_from_args<I>(args: I) -> Result<bool>
where
    I: IntoIterator<Item = OsString>,
{
    let args = args.into_iter().collect::<Vec<_>>();
    if args.len() < 3 || args.get(1).and_then(|arg| arg.to_str()) != Some("--tool-worker") {
        return Ok(false);
    }

    match args.get(2).and_then(|arg| arg.to_str()) {
        Some("shell-exec") => {
            let mut input = String::new();
            std::io::stdin()
                .read_to_string(&mut input)
                .context("failed to read shell worker stdin")?;
            let request: ShellWorkerRequest =
                serde_json::from_str(&input).context("invalid shell worker request")?;
            let response = run_shell_worker_request(&request)?;
            std::io::stdout()
                .write_all(serde_json::to_string(&response)?.as_bytes())
                .context("failed to write shell worker stdout")?;
            Ok(true)
        }
        Some(kind) => bail!("unsupported tool worker mode '{kind}'"),
        None => bail!("missing tool worker mode"),
    }
}

pub fn run_shell_worker_request(request: &ShellWorkerRequest) -> Result<ShellWorkerResponse> {
    if request.command.trim().is_empty() {
        bail!("shell command is empty");
    }

    let workspace_root = PathBuf::from(&request.workspace_root);
    if !workspace_root.exists() {
        bail!(
            "workspace root does not exist: {}",
            workspace_root.display()
        );
    }

    let output = if cfg!(target_os = "windows") {
        Command::new("cmd")
            .arg("/C")
            .arg(&request.command)
            .current_dir(&workspace_root)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
    } else {
        Command::new("/bin/sh")
            .arg("-lc")
            .arg(&request.command)
            .current_dir(&workspace_root)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
    }
    .with_context(|| format!("failed to spawn shell command '{}'", request.command))?;

    Ok(ShellWorkerResponse {
        command: request.command.clone(),
        status: output.status.code().unwrap_or_default(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

pub fn resolve_worker_binary() -> Result<PathBuf> {
    if let Ok(value) = std::env::var("NEXTCHAT_TOOL_WORKER_BIN") {
        return Ok(PathBuf::from(value));
    }

    let current_exe = std::env::current_exe().context("failed to resolve current executable")?;
    if looks_like_main_binary(&current_exe) {
        return Ok(current_exe);
    }

    let mut candidates = Vec::new();
    if let Some(parent) = current_exe.parent() {
        candidates.push(parent.join(binary_name()));
        if let Some(grand_parent) = parent.parent() {
            candidates.push(grand_parent.join(binary_name()));
        }
    }

    for candidate in candidates {
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    Err(anyhow!(
        "failed to resolve tool worker binary from '{}'",
        current_exe.display()
    ))
}

fn looks_like_main_binary(path: &std::path::Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| name == binary_name())
        .unwrap_or(false)
}

fn binary_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "nextchat-desktop.exe"
    } else {
        "nextchat-desktop"
    }
}

#[cfg(test)]
mod tests {
    use super::{run_shell_worker_request, ShellWorkerRequest};
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn unique_root(prefix: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("nextchat-worker-{prefix}-{nanos}"))
    }

    #[test]
    fn shell_worker_runs_command_in_workspace() {
        let workspace_root = unique_root("workspace");
        fs::create_dir_all(&workspace_root).expect("workspace");
        fs::write(workspace_root.join("worker.txt"), "hello from worker").expect("file");

        let response = run_shell_worker_request(&ShellWorkerRequest {
            workspace_root: workspace_root.display().to_string(),
            command: if cfg!(target_os = "windows") {
                "type worker.txt".into()
            } else {
                "cat worker.txt".into()
            },
        })
        .expect("worker response");

        assert_eq!(response.status, 0);
        assert!(response.stdout.contains("hello from worker"));
    }
}
