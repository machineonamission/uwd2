use std::ffi::c_void;
use std::mem::size_of;
use std::path::Path;

use windows::core::imp::CloseHandle;
use windows::core::PCSTR;
use windows::Win32::Foundation::{GetLastError, FALSE, HANDLE, HMODULE};
use windows::Win32::System::Diagnostics::Debug::{
    ReadProcessMemory, SymGetModuleInfo64, SymInitialize, SymLoadModuleEx, SymSetOptions,
    IMAGEHLP_MODULE64, SYMOPT_UNDNAME, SYM_LOAD_FLAGS,
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
    let explorerid =
        // initialize sysinfo with process info
        sysinfo::System::new_with_specifics(
            sysinfo::RefreshKind::new().with_processes(sysinfo::ProcessRefreshKind::everything()),
        )
            // get explorer
            .processes()
            .values()
            .find(|proc| {
                if let Some(p) = proc.exe() {
                    p == Path::new(r"C:\Windows\explorer.exe")
                } else {
                    false
                }
            })
            .unwrap()
            // get PID
            .pid()
            .as_u32();

    OpenProcess(PROCESS_ALL_ACCESS, FALSE, explorerid).unwrap()
}

/// Reads `expected.len()` bytes from the live explorer process at
/// `shell32_base + rva` and returns true only if they exactly match `expected`.
pub unsafe fn verify_rva(rva: u32, expected: &[u8]) -> bool {
    let base = get_shell32_offset();
    let handle = get_explorer_handle();
    let addr = (base + rva as u64) as *const c_void;
    let mut buf = vec![0u8; expected.len()];
    let ok = ReadProcessMemory(
        handle,
        addr,
        buf.as_mut_ptr() as *mut c_void,
        buf.len(),
        None,
    );
    CloseHandle(handle.0);
    ok.is_ok() && buf == expected
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
