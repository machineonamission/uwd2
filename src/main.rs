use std::env;

use crate::cache_pdb::get_rva;
use crate::explorer_modinfo::get_guid;

mod cache_pdb;
mod constants;
mod explorer_modinfo;
mod fetch_pdb;
mod inject;
mod parse_pdb;
mod scan_dll;
mod scheduled_task;
mod structural_scan;

fn prog() -> String {
    // modified from https://stackoverflow.com/a/58113997/9044183
    env::current_exe()
        .unwrap()
        .file_name()
        .unwrap()
        .to_os_string()
        .into_string()
        .unwrap()
}

fn help() {
    println!(
        include_str!("../help.txt"),
        env!("CARGO_PKG_VERSION"),
        prog()
    )
}

fn rva() -> u32 {
    let guid;
    unsafe {
        guid = get_guid();
    }
    let rva = get_rva(guid);
    println!("RVA is {rva:#x}");
    rva
}

fn inject() {
    use std::path::Path;
    use std::thread::sleep;
    use std::time::Duration;

    loop {
        let sys = sysinfo::System::new_with_specifics(
            sysinfo::RefreshKind::new().with_processes(sysinfo::ProcessRefreshKind::everything()),
        );
        if sys.processes().values().any(|p| {
            p.exe().map_or(false, |exe| exe == Path::new(r"C:\Windows\explorer.exe"))
        }) {
            break;
        }
        sleep(Duration::from_millis(500));
    }

    unsafe {
        inject::inject(rva());
        inject::refresh();
    }
}

fn patch_rva(hex: &str) {
    let rva = u32::from_str_radix(hex.trim_start_matches("0x").trim_start_matches("0X"), 16)
        .unwrap_or_else(|_| panic!("invalid hex RVA: {hex}"));
    println!("RVA is {rva:#x}");
    cache_pdb::seed_rva(rva);
    unsafe {
        inject::inject(rva);
        inject::refresh();
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    match args.get(1).map(String::as_str) {
        None | Some("inject") => inject(),
        Some("patch-rva") => match args.get(2) {
            Some(hex) => patch_rva(hex),
            None => eprintln!("Usage: {} patch-rva <hex-rva>  (e.g. patch-rva 1C4934)", prog()),
        },
        Some("install-task") => scheduled_task::install_task(),
        Some("remove-task")  => scheduled_task::remove_task(),
        Some("help") => help(),
        Some("about") => println!(include_str!("../about.txt"), env!("CARGO_PKG_VERSION")),
        Some(err) => eprintln!(
            "Invalid argument `{err}`. Run `{} help` to see all commands.",
            prog()
        ),
    }
}
