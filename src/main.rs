#[cfg(all(windows, target_arch = "x86_64"))]
#[path = "windows/service.rs"]
mod service;

#[cfg(all(windows, target_arch = "x86_64"))]
#[path = "windows/setter_lock.rs"]
mod setter_lock;

#[cfg(all(windows, target_arch = "x86_64"))]
#[path = "windows/process_job.rs"]
mod process_job;

#[cfg(all(windows, target_arch = "x86_64"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::path::PathBuf;
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if (args.len() == 2 && args[0] == "--enroll")
        || (args.len() == 1 && args[0] == "--replace-credential")
    {
        use std::io::{IsTerminal, Read};
        use zeroize::Zeroizing;
        if std::io::stdin().is_terminal() {
            return Err("Supply the device credential through redirected stdin; interactive entry is disabled".into());
        }
        let mut bytes = Zeroizing::new(Vec::new());
        std::io::stdin().take(259).read_to_end(&mut bytes)?;
        let input = std::str::from_utf8(&bytes).map_err(|_| "invalid device credential")?;
        let input = input
            .strip_suffix("\r\n")
            .or_else(|| input.strip_suffix('\n'))
            .unwrap_or(input);
        let credential = Zeroizing::new(input.to_owned());
        let info = if args[0] == "--replace-credential" {
            rustdesk_agent::credentials::replace_credential(credential)?
        } else {
            let origin = args[1].to_str().ok_or("invalid management origin")?;
            rustdesk_agent::credentials::enroll(origin, credential)?
        };
        println!("{}", serde_json::to_string_pretty(&info)?);
        Ok(())
    } else if (args.len() == 1 && (args[0] == "--finish-setup" || args[0] == "--wait-for-check-in"))
        || (args.len() == 2 && args[0] == "--finish-setup" && args[1] == "--initialize-if-missing")
    {
        rustdesk_agent::credentials::require_admin()?;
        let enrollment = rustdesk_agent::credentials::load()?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        if args[0] == "--finish-setup" {
            runtime.block_on(rustdesk_agent::setup::bind(&enrollment))?;
            rustdesk_agent::state_store::initialize_or_validate(args.len() == 2)?;
            println!("Console binding and state ready");
        } else {
            runtime.block_on(rustdesk_agent::setup::wait_for_check_in(&enrollment))?;
            println!("Console received an agent report");
        }
        Ok(())
    } else if args.len() == 1 && args[0] == "--init-state" {
        rustdesk_agent::state_store::initialize()?;
        println!("Management state initialized");
        Ok(())
    } else if args.len() == 1 && args[0] == "--enrollment-info" {
        let enrollment = rustdesk_agent::credentials::load()?;
        println!("{}", serde_json::to_string_pretty(&enrollment.info())?);
        Ok(())
    } else if args.len() == 2 && args[0] == "--detect" {
        let executable = PathBuf::from(&args[1]);
        if !executable.is_absolute() {
            return Err("RustDesk executable path must be absolute".into());
        }
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let report = runtime.block_on(rustdesk_agent::probe::detect(&executable));
        println!("{}", serde_json::to_string_pretty(&report)?);
        Ok(())
    } else if args.len() == 4 && args[0] == "--set-option" {
        let change = rustdesk_agent::setter::Change::parse(
            args[2].to_str().ok_or("invalid setting")?,
            args[3].to_str().ok_or("invalid value")?,
        )?;
        // Keep ownership on this thread through version check, write and readback.
        let _settings_lock = setter_lock::SettingsLock::acquire()?;
        process_job::install()?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let report = runtime
            .block_on(rustdesk_agent::setter::set_option(
                &PathBuf::from(&args[1]),
                change,
            ))
            .map_err(|error| format!("Settings command failed: {error:?}"))?;
        println!("{}", serde_json::to_string_pretty(&report)?);
        if !report.readback_matches {
            return Err("Settings command accepted, but CLI readback did not match".into());
        }
        Ok(())
    } else if (args.len() == 3 || (args.len() == 4 && args[3] == "--manage"))
        && args[0] == "--service"
    {
        service::dispatch(
            PathBuf::from(&args[1]),
            PathBuf::from(&args[2]),
            args.len() == 4,
        )
    } else {
        Err("Usage: rustdesk-agent --enroll <https-origin> (credential on stdin) | --replace-credential (credential on stdin) | --init-state | --enrollment-info | --detect <absolute RustDesk.exe path> | --set-option <absolute RustDesk.exe path> <setting> <value> | --service <absolute RustDesk.exe path> <absolute report.json path> [--manage]".into())
    }
}

#[cfg(not(all(windows, target_arch = "x86_64")))]
fn main() {
    eprintln!("The agent executable supports Windows x64 only.");
    std::process::exit(1);
}
