use std::fs;
use std::path::Path;

use crate::constants::*;
use crate::explorer_modinfo::verify_rva;
use crate::fetch_pdb;
use crate::parse_pdb::parse_pdb;
use crate::scan_dll;
use crate::structural_scan;

/// Resolve the RVA of `CDesktopWatermark::s_DesktopBuildPaint` for the
/// shell32.dll currently loaded in explorer.exe.
///
/// Fallback chain:
///   1. Local .rva cache hit
///   2. Microsoft symbol server (PDB download)
///   3. Multi-pattern binary scan (patterns.bin from a previous PDB run)
///   4. Structural GDI-call scan (no cached state required)
///   5. Panic with manual-recovery instructions
pub fn get_rva(guid: String) -> u32 {
    let dir = data_dir();
    let rva_path = dir.join(guid.clone() + ".rva");

    // 1. Cache hit
    if rva_path.exists() {
        println!("RVA cached. Reading...");
        let file = fs::read(rva_path).unwrap();
        return u32::from_be_bytes(file.try_into().unwrap());
    }

    // 2. Symbol server
    println!("PDB not cached. Fetching from symbol server...");
    let url = fetch_pdb::build_url(guid.clone());
    if let Some(pdbfile) = fetch_pdb::try_fetch(url) {
        println!("Fetched! Parsing...");
        let rva = parse_pdb(pdbfile);
        println!("Parsed! Caching...");
        save_rva_and_patterns(&dir, &guid, rva);
        println!("Cached!");
        return rva;
    }

    // 3. Multi-pattern binary scan
    println!("PDB unavailable. Trying multi-pattern binary scan...");
    let dll_bytes = scan_dll::read_dll();
    if let Some(rva) = try_multi_pattern_scan(&dir, &guid, &dll_bytes) {
        return rva;
    }

    // 4. Structural GDI-call scan
    println!("Trying structural GDI-call scan...");
    if let Some(rva) = structural_scan::find_by_gdi_calls(&dll_bytes) {
        let anchor = scan_dll::read_at_rva(&dll_bytes, rva, 8).unwrap_or_default();
        let verified = unsafe { verify_rva(rva, &anchor) };
        if verified {
            println!("Structural scan: verified RVA {rva:#x}. Caching...");
            save_rva_and_patterns(&dir, &guid, rva);
            return rva;
        }
        eprintln!("Structural scan returned {rva:#x} but live-process verification failed.");
    }

    // 5. Give up
    panic!(
        "\nAll automatic methods failed to locate CDesktopWatermark::s_DesktopBuildPaint.\n\
         \n\
         Manual recovery:\n\
         1. Open x64dbg and attach to explorer.exe\n\
         2. Search All Modules → String references for \"DesktopBuildPaint\"\n\
            or: set a breakpoint on SetTextColor and look at the call stack when\n\
            the desktop refreshes\n\
         3. Note the RVA  =  function_address - shell32_base\n\
         4. Run:  uwd2.exe patch-rva <hex-rva>\n"
    );
}

/// Try the multi-pattern binary scan using a saved patterns.bin.
fn try_multi_pattern_scan(dir: &Path, guid: &str, dll_bytes: &[u8]) -> Option<u32> {
    let patterns_path = dir.join("patterns.bin");
    if !patterns_path.exists() {
        eprintln!(
            "No cached patterns for binary scan.\n\
             Run UWD2 on a build where the PDB is available, or use patch-rva."
        );
        return None;
    }

    let data = match fs::read(&patterns_path) {
        Ok(d) => d,
        Err(e) => { eprintln!("Could not read patterns.bin: {e}"); return None; }
    };
    let patterns = match scan_dll::load_patterns(&data) {
        Some(p) => p,
        None => { eprintln!("patterns.bin is corrupt or unrecognized version."); return None; }
    };

    println!("Scanning shell32.dll with {} sub-patterns...", patterns.len());
    let hits = scan_dll::scan_for_multi_pattern(dll_bytes, &patterns);

    match hits.len() {
        0 => {
            eprintln!("Multi-pattern scan: no match — function prologue changed too much.");
            None
        }
        1 => {
            let file_offset = hits[0];
            let rva = scan_dll::file_offset_to_rva(dll_bytes, file_offset)?;
            println!("Found candidate at RVA {rva:#x}. Verifying against live process...");

            let anchor = &patterns[0].1;
            let verified = unsafe { verify_rva(rva, anchor) };
            if !verified {
                eprintln!("Live-process verification failed — bytes at {rva:#x} do not match.");
                return None;
            }

            println!("Verified! Caching RVA...");
            cleanup_old_rva_files(dir);
            fs::create_dir_all(dir).unwrap();
            fs::write(dir.join(guid.to_string() + ".rva"), rva.to_be_bytes()).unwrap();
            println!("Cached!");
            Some(rva)
        }
        n => {
            eprintln!("Multi-pattern scan: {n} matches — ambiguous, cannot safely patch.");
            None
        }
    }
}

/// Persist RVA and refresh patterns.bin for the current build.
///
/// Called whenever a new RVA is resolved by any method (PDB, patch-rva,
/// structural scan).  Overwrites patterns.bin so future binary scans use
/// the most up-to-date function bytes.
fn save_rva_and_patterns(dir: &Path, guid: &str, rva: u32) {
    cleanup_old_rva_files(dir);
    fs::create_dir_all(dir).unwrap();
    fs::write(dir.join(guid.to_string() + ".rva"), rva.to_be_bytes()).unwrap();

    let dll_bytes = scan_dll::read_dll();
    match scan_dll::save_patterns(&dll_bytes, rva) {
        Some(data) => {
            fs::write(dir.join("patterns.bin"), &data).unwrap();
            println!(
                "Cached {} sub-patterns from RVA {rva:#x} for future builds.",
                data.get(1).copied().unwrap_or(0)
            );
        }
        None => {
            eprintln!(
                "Warning: could not read function bytes — \
                 binary scan unavailable for future builds."
            );
        }
    }
}

/// Called by `patch-rva`: user supplies an RVA manually, we cache it and
/// update patterns.bin so the next normal run hits the cache.
pub fn seed_rva(rva: u32) {
    let guid = unsafe { crate::explorer_modinfo::get_guid() };
    let dir = data_dir();
    save_rva_and_patterns(&dir, &guid, rva);
}

fn cleanup_old_rva_files(dir: &Path) {
    if !dir.exists() {
        return;
    }
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "rva") {
                let _ = fs::remove_file(path);
            }
        }
    }
}
