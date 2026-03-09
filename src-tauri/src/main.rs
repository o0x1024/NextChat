#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    match nextchat_desktop_lib::maybe_run_tool_worker_from_args() {
        Ok(true) => return,
        Ok(false) => {}
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
    nextchat_desktop_lib::run();
}
