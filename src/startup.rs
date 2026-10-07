use std::env;
use std::fs;
use std::path::PathBuf;

fn startup_file() -> Result<PathBuf, String> {
    let appdata = env::var_os("APPDATA")
        .ok_or_else(|| "Windows did not provide the current user's AppData folder.".to_owned())?;
    Ok(PathBuf::from(appdata)
        .join("Microsoft")
        .join("Windows")
        .join("Start Menu")
        .join("Programs")
        .join("Startup")
        .join("UWD2-startup.vbs"))
}

pub fn is_enabled() -> Result<bool, String> {
    Ok(startup_file()?.is_file())
}

pub fn set_enabled(enabled: bool) -> Result<(), String> {
    let file = startup_file()?;
    if enabled {
        let executable =
            env::current_exe().map_err(|err| format!("Could not locate UWD2.exe: {err}"))?;
        let command = format!("\"{}\" --startup", executable.to_string_lossy());
        let escaped_command = command.replace('"', "\"\"");
        let script = format!(
            "Set UWD2Shell = CreateObject(\"WScript.Shell\")\r\nUWD2Shell.Run \"{escaped_command}\", 0, False\r\n"
        );
        let folder = file
            .parent()
            .ok_or_else(|| "Could not locate the Windows Startup folder.".to_owned())?;
        fs::create_dir_all(folder)
            .map_err(|err| format!("Could not create the Windows Startup folder: {err}"))?;
        fs::write(&file, script)
            .map_err(|err| format!("Could not add UWD2 to Windows startup: {err}"))
    } else {
        match fs::remove_file(&file) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(format!("Could not remove UWD2 from Windows startup: {err}")),
        }
    }
}
