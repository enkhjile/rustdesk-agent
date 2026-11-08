use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    sync::OnceLock,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::watch;
use windows_service::{
    define_windows_service,
    service::{
        ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
        ServiceType,
    },
    service_control_handler::{self, ServiceControlHandlerResult, ServiceStatusHandle},
    service_dispatcher,
};

const NAME: &str = "RustDeskAgent";
static CONFIG: OnceLock<(PathBuf, PathBuf)> = OnceLock::new();
define_windows_service!(ffi_service_main, service_main);

fn is_report_path(path: &Path) -> bool {
    path.is_absolute()
        && path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
}

pub fn dispatch(executable: PathBuf, report: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    if !executable.is_absolute() || !is_report_path(&report) || report == executable {
        return Err(
            "Use an absolute executable path and a distinct absolute .json report path".into(),
        );
    }
    if !report.parent().is_some_and(|p| p.is_dir()) {
        return Err("Report directory must already exist and be administrator-protected".into());
    }
    CONFIG
        .set((executable, report))
        .map_err(|_| "service already configured")?;
    service_dispatcher::start(NAME, ffi_service_main)?;
    Ok(())
}

fn publish(
    handle: ServiceStatusHandle,
    state: ServiceState,
    failed: bool,
) -> windows_service::Result<()> {
    let pending = matches!(
        state,
        ServiceState::StartPending | ServiceState::StopPending
    );
    handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: state,
        controls_accepted: if state == ServiceState::Running {
            ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN
        } else {
            ServiceControlAccept::empty()
        },
        exit_code: if failed {
            ServiceExitCode::ServiceSpecific(1)
        } else {
            ServiceExitCode::Win32(0)
        },
        checkpoint: u32::from(pending),
        wait_hint: if pending {
            Duration::from_secs(5)
        } else {
            Duration::ZERO
        },
        process_id: None,
    })
}

fn service_main(_: Vec<OsString>) {
    let (stop_tx, stop_rx) = watch::channel(false);
    let Ok(handle) = service_control_handler::register(NAME, move |event| match event {
        ServiceControl::Stop | ServiceControl::Shutdown => {
            let _ = stop_tx.send(true);
            ServiceControlHandlerResult::NoError
        }
        ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
        _ => ServiceControlHandlerResult::NotImplemented,
    }) else {
        return;
    };
    let result = run(handle, stop_rx);
    let _ = publish(handle, ServiceState::StopPending, false);
    let _ = publish(handle, ServiceState::Stopped, result.is_err());
}

fn run(
    handle: ServiceStatusHandle,
    mut stop: watch::Receiver<bool>,
) -> Result<(), Box<dyn std::error::Error>> {
    publish(handle, ServiceState::StartPending, false)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let enrollment = match rustdesk_agent::credentials::load() {
        Ok(value) => Some(value),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    publish(handle, ServiceState::Running, false)?;
    let (executable, report_path) = CONFIG.get().ok_or("missing service configuration")?;
    let enrollment_info = enrollment.as_ref().map(|value| value.info());
    runtime.block_on(async {
        let mut poller = enrollment
            .map(rustdesk_agent::management::Poller::new)
            .transpose()?;
        loop {
            // Cancelling detection drops its child with kill_on_drop enabled.
            let detection = tokio::select! {
                biased;
                _ = stop.changed() => break,
                detection = rustdesk_agent::probe::detect(executable) => detection,
            };
            let management = match poller.as_mut() {
                Some(poller) => tokio::select! {
                    biased;
                    _ = stop.changed() => break,
                    observation = poller.poll() => Some(observation),
                },
                None => None,
            };
            let delay = management
                .as_ref()
                .map_or(60, |value| value.next_poll_seconds);
            let snapshot = serde_json::json!({
                "observed_at_unix_seconds": SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
                "detection": detection,
                "enrollment": enrollment_info,
                "management": management,
            });
            // A diagnostic snapshot only, never revision state or an applied report.
            std::fs::write(report_path, serde_json::to_vec_pretty(&snapshot)?)?;
            tokio::select! {
                biased;
                _ = stop.changed() => break,
                _ = tokio::time::sleep(Duration::from_secs(delay)) => (),
            }
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_must_be_an_absolute_json_path() {
        assert!(is_report_path(Path::new(
            r"C:\ProgramData\RustDeskAgent\detection.json"
        )));
        assert!(is_report_path(Path::new(
            r"C:\ProgramData\RustDeskAgent\detection.JSON"
        )));
        for path in [
            r"detection.json",
            r"C:\Program Files\RustDesk\RUSTDESK.EXE",
            r"C:\ProgramData\RustDeskAgent\report",
        ] {
            assert!(!is_report_path(Path::new(path)));
        }
    }
}
