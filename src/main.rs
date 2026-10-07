#![cfg_attr(windows, windows_subsystem = "windows")]

use std::env;

mod cache_pdb;
mod constants;
mod explorer_modinfo;
mod fetch_pdb;
mod inject;
mod parse_pdb;
mod startup;
mod ui;

fn prog() -> String {
    env::current_exe()
        .unwrap()
        .file_name()
        .unwrap()
        .to_os_string()
        .into_string()
        .unwrap()
}

fn help() {
    let message = format!(
        include_str!("../help.txt"),
        env!("CARGO_PKG_VERSION"),
        prog()
    );
    ui::show_message("UWD2 Help", &message, false);
}

fn run_operation(enabled: bool) -> Result<bool, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        inject::set_enabled(enabled)
    }))
    .unwrap_or_else(|_| {
        Err(
            "UWD2 could not complete the Explorer operation. Check permissions and network access."
                .into(),
        )
    })
}

fn print_operation(enabled: bool) {
    match run_operation(enabled) {
        Ok(true) => ui::show_message("UWD2", "UWD2 is on.", false),
        Ok(false) => ui::show_message("UWD2", "UWD2 is off.", false),
        Err(err) => ui::show_message("UWD2", &err, true),
    }
}

fn print_status() {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(inject::is_enabled))
        .unwrap_or_else(|_| {
            Err("UWD2 could not read Explorer status. Check permissions and network access.".into())
        });
    match result {
        Ok(true) => ui::show_message("UWD2 Status", "UWD2 is on.", false),
        Ok(false) => ui::show_message("UWD2 Status", "UWD2 is off.", false),
        Err(err) => ui::show_message("UWD2", &err, true),
    }
}

fn main() {
    let args = env::args().collect::<Vec<String>>();
    match args.get(1).map(String::as_str) {
        None | Some("ui") => {
            if let Err(err) = ui::run() {
                ui::show_message("UWD2", &err, true);
            }
        }
        Some("inject") | Some("enable") => print_operation(true),
        Some("disable") => print_operation(false),
        Some("status") => print_status(),
        Some("--startup") => {
            if let Err(err) = run_operation(true) {
                let directory = constants::config_dir();
                if std::fs::create_dir_all(&directory).is_ok() {
                    let _ = std::fs::write(directory.join("last-startup-error.txt"), err);
                }
            }
        }
        Some("help") | Some("--help") | Some("-h") => help(),
        Some("about") => ui::show_message(
            "About UWD2",
            &format!(include_str!("../about.txt"), env!("CARGO_PKG_VERSION")),
            false,
        ),
        Some(err) => ui::show_message(
            "UWD2",
            &format!(
                "Invalid argument `{err}`. Run `{} help` to see all commands.",
                prog()
            ),
            true,
        ),
    }
}
