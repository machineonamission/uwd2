use std::mem::size_of;
use std::path::Path;

use windows::core::imp::CloseHandle;
use windows::core::PCSTR;
use windows::Win32::Foundation::{GetLastError, FALSE, HANDLE, HMODULE};
use windows::Win32::System::Diagnostics::Debug::{
    SymGetModuleInfo64, SymInitialize, SymLoadModuleEx, SymSetOptions, IMAGEHLP_MODULE64,
    SYMOPT_UNDNAME, SYM_LOAD_FLAGS,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleExA;
use windows::Win32::System::Threading::{OpenProcess, PROCESS_ALL_ACCESS};

use crate::constants::*;

pub unsafe fn get_guid() -> String {
    let modinfo = get_all_shell32_modinfos()
        .into_iter()
        .next()
        .expect("no explorer process found");
    let sig = modinfo.PdbSig70.to_u128();
    let age = modinfo.PdbAge;
    // format as hex as michael expects
    format!("{sig:032X}{age:X}")
}

pub unsafe fn get_shell32_offset() -> u64 {
    let modinfo = get_all_shell32_modinfos()
        .into_iter()
        .next()
        .expect("no explorer process found");
    modinfo.BaseOfImage
}

// Return handles for every running explorer.exe process (case-insensitive match on file name).
pub unsafe fn get_explorer_handles() -> Vec<HANDLE> {
    let sys = sysinfo::System::new_with_specifics(
        sysinfo::RefreshKind::new().with_processes(sysinfo::ProcessRefreshKind::everything()),
    );

    sys.processes()
        .values()
        .filter_map(|proc| {
            if let Some(p) = proc.exe() {
                if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                    if name.eq_ignore_ascii_case("explorer.exe") {
                        return Some(OpenProcess(PROCESS_ALL_ACCESS, FALSE, proc.pid().as_u32()).unwrap());
                    }
                }
            }
            None
        })
        .collect()
}

// Get shell32 IMAGEHLP_MODULE64 for a specific explorer process handle.
// This closes the provided process handle before returning.
pub unsafe fn get_shell32_modinfo(explorerhandle: HANDLE) -> IMAGEHLP_MODULE64 {
    SymInitialize(explorerhandle, PCSTR::null(), true).expect("initializing failed");
    SymSetOptions(SYMOPT_UNDNAME);
    let nullterminatedpath = format!("{}\0", SHELL32_PATH);
    let name = PCSTR::from_raw(nullterminatedpath.as_ptr());
    let mut module = HMODULE::default();
    GetModuleHandleExA(0, name, &mut module as *mut HMODULE).unwrap();
    let r = SymLoadModuleEx(
        explorerhandle,    // target process
        HANDLE::default(), // handle to image - not used
        name,              // name of image file
        PCSTR::null(),     // name of module - not required
        module.0 as u64,   // base address - not required
        0,                 // size of image - not required
        None,
        SYM_LOAD_FLAGS::default(),
    );
    if r == 0 {
        GetLastError();
    }
    let mut modinfo = IMAGEHLP_MODULE64 {
        SizeOfStruct: size_of::<IMAGEHLP_MODULE64>() as u32,
        ..Default::default()
    };
    SymGetModuleInfo64(
        explorerhandle,
        module.0 as u64,
        &mut modinfo as *mut IMAGEHLP_MODULE64,
    )
    .unwrap();
    // DO NOT close the process handle here; caller may want to use it (e.g. to WriteProcessMemory)
    modinfo
}

// Collect IMAGEHLP_MODULE64 for every running explorer.exe instance.
pub unsafe fn get_all_shell32_modinfos() -> Vec<IMAGEHLP_MODULE64> {
    let handles = get_explorer_handles();
    let mut out = Vec::new();
    for h in handles {
        let m = get_shell32_modinfo(h);
        // close the handle after we've collected info
        CloseHandle(h.0);
        out.push(m);
    }
    out
}
