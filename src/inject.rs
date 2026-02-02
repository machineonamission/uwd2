use std::ffi::c_void;

use windows::core::imp::CloseHandle;
use windows::core::s;
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::Diagnostics::Debug::WriteProcessMemory;
use windows::Win32::UI::Shell::{SHChangeNotify, SHCNE_ASSOCCHANGED, SHCNF_IDLIST};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowA, GetWindow, GetWindowInfo, SendMessageA, GW_CHILD, WINDOWINFO, WM_COMMAND,
    WS_VISIBLE,
};

use crate::constants::*;
use crate::explorer_modinfo::{get_explorer_handles, get_shell32_modinfo};

pub unsafe fn inject(rva: u32) {
    println!("Getting shell32 offsets for each explorer instance...");
    let handles = get_explorer_handles();
    for handle in handles {
        let modinfo = get_shell32_modinfo(handle);
        let offset = modinfo.BaseOfImage;
        println!("Offset of shell32 inside explorer.exe is {offset:#x}");
        println!("Injecting ret into process handle {:#x}...", handle.0);
        // write return instruction to address of function, effectively disabling it
        WriteProcessMemory(
            handle,
            // offset is position of dll inside explorer.exe, rva is position of func inside dll
            (offset + rva as u64) as *const c_void,
            &RET as *const u8 as *const c_void,
            RET.len(),
            None,
        )
        .unwrap();
        println!("Injected!");
        CloseHandle(handle.0);
    }
}

pub unsafe fn refresh() {
    println!("Refreshing desktop...");
    let hWnd = GetWindow(FindWindowA(s!("Progman"), s!("Program Manager")), GW_CHILD);

    // check if desktop icons are visible
    // https://stackoverflow.com/a/6403014/9044183
    let hWnd2 = GetWindow(hWnd, GW_CHILD);
    let mut wi = WINDOWINFO::default();
    wi.cbSize = std::mem::size_of::<WINDOWINFO>() as u32;
    GetWindowInfo(hWnd2, &mut wi as *mut _).unwrap();
    let visible = wi.dwStyle & WS_VISIBLE == WS_VISIBLE;

    if visible {
        // i have no idea why this works
        // "A file type association has changed" causes the desktop to refresh
        // which makes the watermark go away so whatever it works
        SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None);
    } else {
        // if icons are hidden, no refreshing or anything will work, so just unhide and rehide the icons
        // https://stackoverflow.com/a/6403014/9044183
        SendMessageA(hWnd, WM_COMMAND, WPARAM(0x7402), LPARAM::default());
        SendMessageA(hWnd, WM_COMMAND, WPARAM(0x7402), LPARAM::default());
    }
    println!("Refreshed!")
}
