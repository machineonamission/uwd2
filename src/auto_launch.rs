use auto_launch::{AutoLaunch, WindowsEnableMode};

fn enable_startup() -> Result<(), Box<dyn std::error::Error>> {
    let app_name = "UWD2";
    let exe_path = std::env::current_exe()?.to_string_lossy().into_owned();

    // Specify current user (no admin required)
    let auto = AutoLaunch::new(app_name, &exe_path, WindowsEnableMode::CurrentUser, &[]);

    auto.enable()?;
    Ok(())
}
