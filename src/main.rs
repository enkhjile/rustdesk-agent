#[cfg(all(windows, target_arch = "x86_64"))]
#[path = "windows/service.rs"]
mod service;

#[cfg(all(windows, target_arch = "x86_64"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::path::PathBuf;
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() == 2 && args[0] == "--enroll" {
        use std::io::{IsTerminal, Read};
        use zeroize::Zeroizing;
        if std::io::stdin().is_terminal() {
            return Err("Supply the device credential through redirected stdin; interactive entry is disabled".into());
        }
        let origin = args[1].to_str().ok_or("invalid management origin")?;
        let mut bytes = Zeroizing::new(Vec::new());
        std::io::stdin().take(259).read_to_end(&mut bytes)?;
        let input = std::str::from_utf8(&bytes).map_err(|_| "invalid device credential")?;
        let input = input
            .strip_suffix("\r\n")
            .or_else(|| input.strip_suffix('\n'))
            .unwrap_or(input);
        let info = rustdesk_agent::credentials::enroll(origin, Zeroizing::new(input.to_owned()))?;
        println!("{}", serde_json::to_string_pretty(&info)?);
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
    } else if args.len() == 3 && args[0] == "--service" {
        service::dispatch(PathBuf::from(&args[1]), PathBuf::from(&args[2]))
    } else {
        Err("Usage: rustdesk-agent --enroll <https-origin> (credential on stdin) | --init-state | --enrollment-info | --detect <absolute RustDesk.exe path> | --service <absolute RustDesk.exe path> <absolute report.json path>".into())
    }
}

#[cfg(not(all(windows, target_arch = "x86_64")))]
fn main() {
    eprintln!("The agent executable supports Windows x64 only.");
    std::process::exit(1);
}
