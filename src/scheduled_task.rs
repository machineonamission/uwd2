use std::env;
use std::fs;
use std::process::Command;

pub fn install_task() {
    let exe = match env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Cannot determine current exe path: {e}");
            return;
        }
    };

    let vbs_path = exe.with_file_name("uwd2_launch.vbs");
    let exe_str = exe.to_string_lossy();
    let vbs = format!(
        "CreateObject(\"WScript.Shell\").Run Chr(34) & \"{exe}\" & Chr(34) & \" inject\", 0, True\r\n",
        exe = exe_str.replace('"', "\"\"")
    );
    if let Err(e) = fs::write(&vbs_path, vbs.as_bytes()) {
        eprintln!("Cannot write launcher script: {e}");
        return;
    }

    let vbs_str = vbs_path.to_string_lossy();
    let vbs_ps = vbs_str.replace('\'', "''");

    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
$vbs      = '{vbs_ps}'
$action   = New-ScheduledTaskAction -Execute 'wscript.exe' -Argument ('//nologo "' + $vbs + '"')
$trigger  = New-ScheduledTaskTrigger -AtLogOn
$settings = New-ScheduledTaskSettingsSet `
    -AllowStartIfOnBatteries `
    -DontStopIfGoingOnBatteries `
    -ExecutionTimeLimit (New-TimeSpan -Minutes 2) `
    -StartWhenAvailable
$principal = New-ScheduledTaskPrincipal `
    -UserId ([System.Security.Principal.WindowsIdentity]::GetCurrent().Name) `
    -LogonType Interactive `
    -RunLevel Highest
Register-ScheduledTask `
    -TaskName    'UWD2_WatermarkRemover' `
    -Action      $action `
    -Trigger     $trigger `
    -Settings    $settings `
    -Principal   $principal `
    -Description 'UWD2: removes the Windows Insider evaluation watermark at logon.' `
    -Force | Out-Null
Write-Host 'Task registered.'
"#,
        vbs_ps = vbs_ps
    );

    let status = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &script,
        ])
        .status();

    match status {
        Ok(s) if s.success() => {
            println!("Scheduled task 'UWD2_WatermarkRemover' installed.");
            println!("UWD2 will now run automatically at every logon.");
        }
        Ok(s) => eprintln!(
            "PowerShell exited with code {:?}. Try running uwd2 as administrator.",
            s.code()
        ),
        Err(e) => eprintln!("Failed to launch PowerShell: {e}"),
    }
}

pub fn remove_task() {
    if let Ok(exe) = env::current_exe() {
        let _ = fs::remove_file(exe.with_file_name("uwd2_launch.vbs"));
    }

    let script = r#"
$ErrorActionPreference = 'Stop'
Unregister-ScheduledTask -TaskName 'UWD2_WatermarkRemover' -Confirm:$false
Write-Host 'Task removed.'
"#;

    let status = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .status();

    match status {
        Ok(s) if s.success() => println!("Scheduled task removed."),
        Ok(s) => eprintln!(
            "PowerShell exited with code {:?}. The task may not have been installed.",
            s.code()
        ),
        Err(e) => eprintln!("Failed to launch PowerShell: {e}"),
    }
}
