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
    let modinfo = get_shell32_modinfo();
    let sig = modinfo.PdbSig70.to_u128();
    let age = modinfo.PdbAge;
    // format as hex as michael expects
    format!("{sig:032X}{age:X}")
}

pub unsafe fn get_shell32_offset() -> u64 {
    let modinfo = get_shell32_modinfo();
    modinfo.BaseOfImage
}

pub unsafe fn get_explorer_handle() -> HANDLE {
    get_explorer_handle_for(get_explorer_info().0)
}

pub unsafe fn get_explorer_handle_for(process_id: u32) -> HANDLE {
    OpenProcess(PROCESS_ALL_ACCESS, FALSE, process_id).unwrap()
}

pub fn get_explorer_info() -> (u32, u64) {
    let system = sysinfo::System::new_with_specifics(
        sysinfo::RefreshKind::new().with_processes(sysinfo::ProcessRefreshKind::everything()),
    );
    let process = system
        .processes()
        .values()
        .find(|proc| {
            proc.exe()
                .map(|p| p == Path::new(r"C:\Windows\explorer.exe"))
                == Some(true)
        })
        .unwrap();
    (process.pid().as_u32(), process.start_time())
}

pub unsafe fn get_shell32_modinfo() -> IMAGEHLP_MODULE64 {
    // get info of shell32.dll using running explorer.exe

    let explorerhandle = get_explorer_handle();

    // let currentprocess = GetCurrentProcess();
    SymInitialize(explorerhandle, PCSTR::null(), true).expect("initializing failed");
    SymSetOptions(SYMOPT_UNDNAME);
    let nullterminatedpath = format!("{}\0", SHELL32_PATH);
    // dbg!(&nullterminatedpath);
    let name = PCSTR::from_raw(nullterminatedpath.as_ptr());
    let mut module = HMODULE::default();
    GetModuleHandleExA(0, name, &mut module as *mut HMODULE).unwrap();
    // let module = LoadLibraryExA(name, HANDLE::default(), LOAD_LIBRARY_FLAGS::default()).unwrap();
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
    CloseHandle(explorerhandle.0);
    // dbg!(modinfo);
    modinfo
}
