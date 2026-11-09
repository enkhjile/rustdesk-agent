//! Process-lifetime containment for the one-shot settings CLI, not the service.
use std::{
    io,
    os::windows::io::{AsRawHandle, FromRawHandle, IntoRawHandle, OwnedHandle},
    ptr,
};
use windows_sys::Win32::System::{
    JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject,
    },
    Threading::GetCurrentProcess,
};

/// Call once, before spawning any setting-command children. Fail closed if the
/// host's job restrictions prevent containment (including incompatible nesting).
pub fn install() -> io::Result<()> {
    unsafe {
        // Unnamed and non-inheritable: this process owns the only job handle.
        let raw = CreateJobObjectW(ptr::null(), ptr::null());
        if raw.is_null() {
            return Err(io::Error::last_os_error());
        }
        let job = OwnedHandle::from_raw_handle(raw);
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if SetInformationJobObject(
            job.as_raw_handle(),
            JobObjectExtendedLimitInformation,
            (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        ) == 0
        {
            return Err(io::Error::last_os_error());
        }
        // Children inherit membership from birth, avoiding a spawn/assign race.
        if AssignProcessToJobObject(job.as_raw_handle(), GetCurrentProcess()) == 0 {
            return Err(io::Error::last_os_error());
        }
        // Intentionally retain until OS process teardown, even on forced death.
        // Dropping it here would kill this process; inheriting it would let a
        // surviving child keep the job alive. Windows closes it on normal exit too.
        let _process_lifetime_handle = job.into_raw_handle();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::Path,
        process::{Command, Stdio},
        thread,
        time::{Duration, Instant},
    };
    use windows_sys::Win32::{
        Foundation::{CloseHandle, WAIT_OBJECT_0},
        System::Threading::{
            OpenProcess, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE, TerminateProcess,
            WaitForSingleObject,
        },
    };

    fn child_command(root: &Path, mode: &str) -> Command {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", "process_job::tests::child"])
            .env("RUSTDESK_AGENT_JOB_TEST_ROOT", root)
            .env("RUSTDESK_AGENT_JOB_TEST_MODE", mode)
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        command
    }

    #[test]
    fn child() {
        let Ok(root) = std::env::var("RUSTDESK_AGENT_JOB_TEST_ROOT") else {
            return;
        };
        let root = Path::new(&root);
        match std::env::var("RUSTDESK_AGENT_JOB_TEST_MODE")
            .unwrap()
            .as_str()
        {
            "owner" => {
                install().unwrap();
                let mut leaf = child_command(root, "leaf").spawn().unwrap();
                let deadline = Instant::now() + Duration::from_secs(15);
                while !root.join("exit").exists() && Instant::now() < deadline {
                    thread::sleep(Duration::from_millis(10));
                }
                // The job, not a Rust destructor, must clean up this child.
                let _ = leaf.try_wait();
            }
            "leaf" => {
                let mut grandchild = child_command(root, "grandchild").spawn().unwrap();
                fs::write(root.join("leaf"), std::process::id().to_string()).unwrap();
                thread::sleep(Duration::from_secs(20));
                let _ = grandchild.try_wait();
            }
            "grandchild" => {
                fs::write(root.join("grandchild"), std::process::id().to_string()).unwrap();
                thread::sleep(Duration::from_secs(20));
            }
            _ => panic!("invalid child mode"),
        }
    }

    #[test]
    fn descendants_exit_on_normal_and_forced_parent_exit() {
        for force in [false, true] {
            let root = std::env::temp_dir().join(format!("rustdesk-job-{}", uuid::Uuid::new_v4()));
            fs::create_dir(&root).unwrap();
            let mut owner = child_command(&root, "owner").spawn().unwrap();
            let deadline = Instant::now() + Duration::from_secs(10);
            let ready = |name| fs::metadata(root.join(name)).is_ok_and(|entry| entry.len() > 0);
            while !ready("grandchild") || !ready("leaf") {
                if Instant::now() >= deadline {
                    let _ = owner.kill();
                    panic!("job test children did not start");
                }
                thread::sleep(Duration::from_millis(10));
            }
            let handles: Vec<_> = ["leaf", "grandchild"]
                .iter()
                .map(|name| {
                    let pid = fs::read_to_string(root.join(name))
                        .unwrap()
                        .parse()
                        .unwrap();
                    let handle =
                        unsafe { OpenProcess(PROCESS_SYNCHRONIZE | PROCESS_TERMINATE, 0, pid) };
                    assert!(!handle.is_null());
                    handle
                })
                .collect();
            if force {
                owner.kill().unwrap();
            } else {
                fs::write(root.join("exit"), "exit").unwrap();
            }
            let status = owner.wait().unwrap();
            let mut all_exited = true;
            for handle in handles {
                unsafe {
                    let exited = WaitForSingleObject(handle, 3_000) == WAIT_OBJECT_0;
                    if !exited {
                        TerminateProcess(handle, 1);
                        WaitForSingleObject(handle, 3_000);
                    }
                    CloseHandle(handle);
                    all_exited &= exited;
                }
            }
            fs::remove_dir_all(root).unwrap();
            if !force {
                assert!(status.success(), "containment changed normal exit status");
            }
            assert!(
                all_exited,
                "descendants survived parent exit (forced={force})"
            );
        }
    }
}
