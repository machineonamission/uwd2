use std::path::PathBuf;

use directories::ProjectDirs;

pub fn data_dir() -> PathBuf {
    let binding = ProjectDirs::from("net", "reticivis", "UWD2").unwrap();
    let dir = binding.data_dir();
    dir.to_owned()
}

pub const SHELL32_PATH: &str = r"C:\Windows\System32\shell32.dll";

// ret instruction
#[cfg(target_arch = "x86_64")]
pub const RET: [u8; 1] = [0xC3];

// br lr instruction
#[cfg(target_arch = "aarch64")]
pub const RET: [u8; 4] = [0xc0, 0x03, 0x1f, 0xd6];
