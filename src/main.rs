#[cfg(all(windows, target_arch = "x86_64"))]
#[path = "windows/service.rs"]
mod service;

#[cfg(all(windows, target_arch = "x86_64"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::path::PathBuf;
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() == 2 && args[0] == "--detect" {
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
        Err("Usage: rustdesk-agent --detect <absolute RustDesk.exe path> | --service <absolute RustDesk.exe path> <absolute report.json path>".into())
    }
}

#[cfg(not(all(windows, target_arch = "x86_64")))]
fn main() {
    eprintln!("The agent executable supports Windows x64 only.");
    std::process::exit(1);
}
