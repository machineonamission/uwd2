use std::ffi::c_void;
use std::fs;
use std::path::PathBuf;

use windows::core::imp::CloseHandle;
use windows::core::s;
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::Diagnostics::Debug::{
    FlushInstructionCache, ReadProcessMemory, WriteProcessMemory,
};
use windows::Win32::UI::Shell::{SHChangeNotify, SHCNE_ASSOCCHANGED, SHCNF_IDLIST};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowA, GetWindow, GetWindowInfo, SendMessageA, GW_CHILD, WINDOWINFO, WM_COMMAND,
    WS_VISIBLE,
};

use crate::cache_pdb::get_rva;
use crate::constants::*;
use crate::explorer_modinfo::{
    get_explorer_handle_for, get_explorer_info, get_guid, get_shell32_offset,
};

#[derive(Clone, Debug)]
struct PatchState {
    process_id: u32,
    process_started: u64,
    rva: u32,
    module_offset: u64,
    original_bytes: Vec<u8>,
}

fn state_path() -> PathBuf {
    config_dir().join("patch-state")
}

fn load_state() -> Result<Option<PatchState>, String> {
    let contents = match fs::read_to_string(state_path()) {
        Ok(contents) => contents,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(format!("Could not read UWD2 restore data: {err}")),
    };

    let fields = contents.split_whitespace().collect::<Vec<_>>();
    if fields.len() != 5 {
        return Err(
            "UWD2 restore data is invalid; the saved Explorer instruction cannot be restored."
                .into(),
        );
    }

    let parse_error = || {
        "UWD2 restore data is invalid; the saved Explorer instruction cannot be restored."
            .to_owned()
    };
    let process_id = fields[0].parse().map_err(|_| parse_error())?;
    let process_started = fields[1].parse().map_err(|_| parse_error())?;
    let rva = u32::from_str_radix(fields[2], 16).map_err(|_| parse_error())?;
    let module_offset = u64::from_str_radix(fields[3], 16).map_err(|_| parse_error())?;
    if fields[4].len() != RET.len() * 2 {
        return Err(
            "UWD2 restore data is incompatible with this instruction size; restart Explorer before using the off control."
                .into(),
        );
    }
    let original_bytes = fields[4]
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let pair = std::str::from_utf8(pair).map_err(|_| parse_error())?;
            u8::from_str_radix(pair, 16).map_err(|_| parse_error())
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Some(PatchState {
        process_id,
        process_started,
        rva,
        module_offset,
        original_bytes,
    }))
}

fn save_state(state: &PatchState) -> Result<(), String> {
    fs::create_dir_all(config_dir())
        .map_err(|err| format!("Could not create UWD2 settings folder: {err}"))?;
    let original_bytes = state
        .original_bytes
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<String>();
    let contents = format!(
        "{} {} {:08X} {:016X} {}\n",
        state.process_id, state.process_started, state.rva, state.module_offset, original_bytes
    );
    fs::write(state_path(), contents)
        .map_err(|err| format!("Could not save Explorer restore data: {err}"))
}

fn clear_state() -> Result<(), String> {
    match fs::remove_file(state_path()) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(format!("Could not clear UWD2 restore data: {err}")),
    }
}

unsafe fn read_bytes(
    handle: windows::Win32::Foundation::HANDLE,
    address: u64,
    length: usize,
) -> Result<Vec<u8>, String> {
    let mut bytes = vec![0u8; length];
    let mut bytes_read = 0usize;
    ReadProcessMemory(
        handle,
        address as usize as *const c_void,
        bytes.as_mut_ptr() as *mut c_void,
        length,
        Some(&mut bytes_read),
    )
    .map_err(|err| format!("Could not read Explorer memory: {err}"))?;
    if bytes_read != length {
        return Err("Could not read the watermark routine in Explorer.".into());
    }
    Ok(bytes)
}

unsafe fn write_bytes(
    handle: windows::Win32::Foundation::HANDLE,
    address: u64,
    bytes: &[u8],
) -> Result<(), String> {
    let mut bytes_written = 0usize;
    WriteProcessMemory(
        handle,
        address as usize as *const c_void,
        bytes.as_ptr() as *const c_void,
        bytes.len(),
        Some(&mut bytes_written),
    )
    .map_err(|err| format!("Could not update Explorer memory: {err}"))?;
    if bytes_written != bytes.len() {
        return Err("Explorer did not accept the complete watermark patch.".into());
    }
    FlushInstructionCache(handle, Some(address as usize as *const c_void), bytes.len())
        .map_err(|err| format!("Could not refresh Explorer's watermark routine: {err}"))?;
    Ok(())
}

fn matches_target(
    state: &PatchState,
    process_id: u32,
    process_started: u64,
    rva: u32,
    module_offset: u64,
) -> bool {
    state.process_id == process_id
        && state.process_started == process_started
        && state.rva == rva
        && state.module_offset == module_offset
}

fn resolve_target() -> (u32, u64, u32, u64) {
    let rva = get_rva(unsafe { get_guid() });
    let module_offset = unsafe { get_shell32_offset() };
    let (process_id, process_started) = get_explorer_info();
    (process_id, process_started, rva, module_offset)
}

/// Returns whether this UWD2 build has the routine patched in the current Explorer process.
pub fn is_enabled() -> Result<bool, String> {
    let (process_id, process_started, rva, module_offset) = resolve_target();
    let handle = unsafe { get_explorer_handle_for(process_id) };
    let result = (|| unsafe {
        let address = module_offset + rva as u64;
        let bytes = read_bytes(handle, address, RET.len())?;
        match load_state()? {
            Some(state) if matches_target(&state, process_id, process_started, rva, module_offset) => {
                if bytes.as_slice() == RET.as_slice() {
                    Ok(true)
                } else if bytes == state.original_bytes {
                    clear_state()?;
                    Ok(false)
                } else {
                    Err("The Explorer watermark routine changed after UWD2 saved its restore data. Restart Explorer before using UWD2 again.".into())
                }
            }
            Some(_) => {
                clear_state()?;
                if bytes.as_slice() == RET.as_slice() {
                    Err("Explorer appears patched, but UWD2 has no matching restore data. Restart Explorer before using the off control.".into())
                } else {
                    Ok(false)
                }
            }
            None if bytes.as_slice() == RET.as_slice() => Err("Explorer appears patched, but UWD2 has no saved restore data. Restart Explorer before using the off control.".into()),
            None => Ok(false),
        }
    })();
    unsafe { CloseHandle(handle.0) };
    result
}

/// Enables or disables UWD2 in the current Explorer process and returns the resulting state.
pub fn set_enabled(enabled: bool) -> Result<bool, String> {
    let (process_id, process_started, rva, module_offset) = resolve_target();
    let handle = unsafe { get_explorer_handle_for(process_id) };
    let result = (|| unsafe {
        let address = module_offset + rva as u64;
        let bytes = read_bytes(handle, address, RET.len())?;
        let saved = load_state()?;
        let matching_state = saved
            .as_ref()
            .filter(|state| matches_target(state, process_id, process_started, rva, module_offset));

        if enabled {
            if let Some(state) = matching_state {
                if bytes.as_slice() == RET.as_slice() {
                    return Ok(true);
                }
                if bytes == state.original_bytes {
                    clear_state()?;
                } else {
                    return Err("The Explorer watermark routine changed after UWD2 saved its restore data. Restart Explorer before trying again.".into());
                }
            } else if saved.is_some() {
                clear_state()?;
            }

            if bytes.as_slice() == RET.as_slice() {
                return Err("The watermark routine already begins with a return instruction, but UWD2 has no restore data for this Explorer session.".into());
            }

            let state = PatchState {
                process_id,
                process_started,
                rva,
                module_offset,
                original_bytes: bytes,
            };
            save_state(&state)?;
            if let Err(err) = write_bytes(handle, address, &RET) {
                if write_bytes(handle, address, &state.original_bytes).is_ok() {
                    let _ = clear_state();
                }
                return Err(err);
            }
            refresh();
            Ok(true)
        } else {
            let state = match matching_state {
                Some(state) => state.clone(),
                None => {
                    if saved.is_some() {
                        clear_state()?;
                    }
                    if bytes.as_slice() == RET.as_slice() {
                        return Err("UWD2 cannot safely restore this Explorer session because its original bytes were not saved. Restart Explorer to restore it.".into());
                    }
                    return Ok(false);
                }
            };

            if bytes == state.original_bytes {
                clear_state()?;
                return Ok(false);
            }
            if bytes.as_slice() != RET.as_slice() {
                return Err("Explorer's watermark routine no longer matches UWD2's patch. Restart Explorer to restore it safely.".into());
            }
            write_bytes(handle, address, &state.original_bytes)?;
            refresh();
            clear_state()?;
            Ok(false)
        }
    })();
    unsafe { CloseHandle(handle.0) };
    result
}

pub unsafe fn refresh() {
    println!("Refreshing desktop...");
    let h_wnd = GetWindow(FindWindowA(s!("Progman"), s!("Program Manager")), GW_CHILD);

    // check if desktop icons are visible
    // https://stackoverflow.com/a/6403014/9044183
    let h_wnd2 = GetWindow(h_wnd, GW_CHILD);
    let mut wi = WINDOWINFO::default();
    wi.cbSize = std::mem::size_of::<WINDOWINFO>() as u32;
    GetWindowInfo(h_wnd2, &mut wi as *mut _).unwrap();
    let visible = wi.dwStyle & WS_VISIBLE == WS_VISIBLE;

    if visible {
        // "A file type association has changed" causes the desktop to refresh.
        SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None);
    } else {
        // Temporarily toggle desktop icons when the shell is configured to hide them.
        SendMessageA(h_wnd, WM_COMMAND, WPARAM(0x7402), LPARAM::default());
        SendMessageA(h_wnd, WM_COMMAND, WPARAM(0x7402), LPARAM::default());
    }
    println!("Refreshed!")
}
