use std::fs;

use crate::constants::SHELL32_PATH;

pub(crate) struct Section {
    virtual_size: u32,
    virtual_address: u32,
    raw_size: u32,
    ptr_to_raw: u32,
    characteristics: u32,
}

pub(crate) fn parse_sections(dll: &[u8]) -> Vec<Section> {
    let e_lfanew = u32::from_le_bytes(dll[0x3C..0x40].try_into().unwrap()) as usize;
    let num_sections =
        u16::from_le_bytes(dll[e_lfanew + 6..e_lfanew + 8].try_into().unwrap()) as usize;
    let opt_hdr_size =
        u16::from_le_bytes(dll[e_lfanew + 20..e_lfanew + 22].try_into().unwrap()) as usize;
    // PE sig (4) + COFF header (20) + optional header
    let sec_base = e_lfanew + 24 + opt_hdr_size;

    (0..num_sections)
        .map(|i| {
            let b = sec_base + i * 40;
            Section {
                virtual_size: u32::from_le_bytes(dll[b + 8..b + 12].try_into().unwrap()),
                virtual_address: u32::from_le_bytes(dll[b + 12..b + 16].try_into().unwrap()),
                raw_size: u32::from_le_bytes(dll[b + 16..b + 20].try_into().unwrap()),
                ptr_to_raw: u32::from_le_bytes(dll[b + 20..b + 24].try_into().unwrap()),
                characteristics: u32::from_le_bytes(dll[b + 36..b + 40].try_into().unwrap()),
            }
        })
        .collect()
}

pub fn read_dll() -> Vec<u8> {
    fs::read(SHELL32_PATH).expect("failed to read shell32.dll from disk")
}

pub fn rva_to_file_offset(dll: &[u8], rva: u32) -> Option<u32> {
    for s in parse_sections(dll) {
        let span = s.virtual_size.max(s.raw_size);
        if rva >= s.virtual_address && rva < s.virtual_address + span {
            return Some(rva - s.virtual_address + s.ptr_to_raw);
        }
    }
    None
}

pub fn file_offset_to_rva(dll: &[u8], offset: u32) -> Option<u32> {
    for s in parse_sections(dll) {
        if offset >= s.ptr_to_raw && offset < s.ptr_to_raw + s.raw_size {
            return Some(offset - s.ptr_to_raw + s.virtual_address);
        }
    }
    None
}

pub fn read_at_rva(dll: &[u8], rva: u32, count: usize) -> Option<Vec<u8>> {
    let off = rva_to_file_offset(dll, rva)? as usize;
    if off + count > dll.len() {
        return None;
    }
    Some(dll[off..off + count].to_vec())
}

const IMAGE_SCN_MEM_EXECUTE: u32 = 0x2000_0000;

/// Returns file offsets (not RVAs) of every match within executable sections.
pub fn scan_for_pattern(dll: &[u8], pattern: &[u8]) -> Vec<u32> {
    let mut hits = Vec::new();
    for s in parse_sections(dll) {
        if s.characteristics & IMAGE_SCN_MEM_EXECUTE == 0 {
            continue;
        }
        let start = s.ptr_to_raw as usize;
        let end = (s.ptr_to_raw + s.raw_size) as usize;
        if end > dll.len() {
            continue;
        }
        for (i, window) in dll[start..end].windows(pattern.len()).enumerate() {
            if window == pattern {
                hits.push(s.ptr_to_raw + i as u32);
            }
        }
    }
    hits
}

// ── patterns.bin format ────────────────────────────────────────────────────
//
// [u8  version = 1]
// [u8  count]
// count × {
//   [u32 relative_offset_from_func_entry  (little-endian)]
//   [u8  length]
//   [length × u8 bytes]
// }
//
// Offsets are collected at: 0 (prologue), 8, 30, 60 bytes into the function.
// The first entry always has relative_offset == 0 and serves as the scan anchor.

const PATTERN_SPECS: &[(u32, usize)] = &[(0, 8), (8, 8), (30, 8), (60, 8)];

/// Build a patterns.bin payload from a known function RVA in the on-disk DLL.
pub fn save_patterns(dll: &[u8], func_rva: u32) -> Option<Vec<u8>> {
    let mut entries: Vec<(u32, Vec<u8>)> = Vec::new();
    for &(rel_off, len) in PATTERN_SPECS {
        if let Some(bytes) = read_at_rva(dll, func_rva + rel_off, len) {
            entries.push((rel_off, bytes));
        }
    }
    if entries.is_empty() {
        return None;
    }
    let mut buf = vec![1u8, entries.len() as u8]; // version, count
    for (rel_off, bytes) in &entries {
        buf.extend_from_slice(&rel_off.to_le_bytes());
        buf.push(bytes.len() as u8);
        buf.extend_from_slice(bytes);
    }
    Some(buf)
}

/// Parse a patterns.bin payload into a list of (relative_offset, bytes) pairs.
pub fn load_patterns(data: &[u8]) -> Option<Vec<(u32, Vec<u8>)>> {
    if data.len() < 2 || data[0] != 1 {
        return None;
    }
    let count = data[1] as usize;
    let mut patterns = Vec::with_capacity(count);
    let mut pos = 2usize;
    for _ in 0..count {
        if pos + 5 > data.len() {
            return None;
        }
        let rel_off = u32::from_le_bytes(data[pos..pos + 4].try_into().ok()?);
        let len = data[pos + 4] as usize;
        pos += 5;
        if pos + len > data.len() {
            return None;
        }
        patterns.push((rel_off, data[pos..pos + len].to_vec()));
        pos += len;
    }
    Some(patterns)
}

/// Scan executable sections for a multi-pattern match.
///
/// `patterns` is a list of (relative_offset_from_func_entry, bytes).  The
/// first entry must have relative_offset == 0 and acts as the search anchor.
/// A candidate position passes only if *every* pattern matches at its
/// relative offset from that position.
///
/// Returns file offsets of matching function entries.
pub fn scan_for_multi_pattern(dll: &[u8], patterns: &[(u32, Vec<u8>)]) -> Vec<u32> {
    let Some((anchor_off, anchor_bytes)) = patterns.first() else {
        return vec![];
    };
    if *anchor_off != 0 || anchor_bytes.is_empty() {
        return vec![];
    }

    let mut hits = Vec::new();
    for s in parse_sections(dll) {
        if s.characteristics & IMAGE_SCN_MEM_EXECUTE == 0 {
            continue;
        }
        let start = s.ptr_to_raw as usize;
        let end = (s.ptr_to_raw + s.raw_size).min(dll.len() as u32) as usize;
        if end < start + anchor_bytes.len() {
            continue;
        }

        'outer: for i in 0..=(end - start - anchor_bytes.len()) {
            let candidate = start + i;
            if &dll[candidate..candidate + anchor_bytes.len()] != anchor_bytes.as_slice() {
                continue;
            }
            for (rel_off, bytes) in &patterns[1..] {
                let check = candidate + *rel_off as usize;
                if check + bytes.len() > end {
                    continue 'outer;
                }
                if &dll[check..check + bytes.len()] != bytes.as_slice() {
                    continue 'outer;
                }
            }
            hits.push(s.ptr_to_raw + i as u32);
        }
    }
    hits
}
