//! Serializes explicit CLI invocations; this is not a RustDesk authorization boundary.
use std::{io, marker::PhantomData, ptr, rc::Rc};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, LocalFree, WAIT_ABANDONED, WAIT_OBJECT_0, WAIT_TIMEOUT},
    Security::{
        Authorization::{ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1},
        SECURITY_ATTRIBUTES,
    },
    System::Threading::{
        CreateMutexExW, MUTEX_MODIFY_STATE, ReleaseMutex, SYNCHRONIZATION_SYNCHRONIZE,
        WaitForSingleObject,
    },
};

pub struct SettingsLock {
    handle: HANDLE,
    // Windows mutex ownership is thread-affine, including when running async work.
    _same_thread: PhantomData<Rc<()>>,
}

impl SettingsLock {
    pub fn acquire() -> io::Result<Self> {
        Self::acquire_named("Global\\RustDeskAgent.Settings.v1", 3_000)
    }

    fn acquire_named(name: &str, timeout_ms: u32) -> io::Result<Self> {
        let name: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
        // Only synchronize/release access: creator, administrators, SYSTEM and LocalService.
        // OWNER RIGHTS also avoids granting the creator implicit WRITE_DAC access.
        let sddl: Vec<u16> =
            "D:P(A;;0x00100001;;;OW)(A;;0x00100001;;;BA)(A;;0x00100001;;;SY)(A;;0x00100001;;;LS)"
                .encode_utf16()
                .chain(Some(0))
                .collect();
        unsafe {
            let mut descriptor = ptr::null_mut();
            if ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                ptr::null_mut(),
            ) == 0
            {
                return Err(io::Error::last_os_error());
            }
            let attributes = SECURITY_ATTRIBUTES {
                nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: descriptor,
                bInheritHandle: 0,
            };
            let handle = CreateMutexExW(
                &attributes,
                name.as_ptr(),
                0,
                SYNCHRONIZATION_SYNCHRONIZE | MUTEX_MODIFY_STATE,
            );
            let create_error = io::Error::last_os_error();
            LocalFree(descriptor);
            if handle.is_null() {
                return Err(create_error);
            }
            let status = WaitForSingleObject(handle, timeout_ms);
            if status == WAIT_OBJECT_0 || status == WAIT_ABANDONED {
                let guard = Self {
                    handle,
                    _same_thread: PhantomData,
                };
                if status == WAIT_ABANDONED {
                    // No rollback is implied after another process dies mid-command.
                    drop(guard);
                    return Err(io::Error::new(
                        io::ErrorKind::Interrupted,
                        "Previous settings command was interrupted; inspect settings before retrying",
                    ));
                }
                return Ok(guard);
            }
            let error = if status == WAIT_TIMEOUT {
                io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "Settings busy: another agent command holds the settings lock",
                )
            } else {
                io::Error::last_os_error()
            };
            CloseHandle(handle);
            Err(error)
        }
    }
}

impl Drop for SettingsLock {
    fn drop(&mut self) {
        unsafe {
            ReleaseMutex(self.handle);
            CloseHandle(self.handle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::mpsc, thread};

    #[test]
    fn child() {
        let Ok(name) = std::env::var("RUSTDESK_AGENT_TEST_MUTEX") else {
            return;
        };
        let result = SettingsLock::acquire_named(&name, 30);
        if std::env::var_os("RUSTDESK_AGENT_TEST_BUSY").is_some() {
            assert_eq!(result.err().unwrap().kind(), io::ErrorKind::WouldBlock);
        } else {
            result.unwrap();
        }
    }

    #[test]
    fn excludes_another_process_then_releases() {
        let name = format!("Global\\RustDeskAgent.Test.{}", uuid::Uuid::new_v4());
        let owner = SettingsLock::acquire_named(&name, 0).unwrap();
        let run = |busy: bool| {
            let mut child = std::process::Command::new(std::env::current_exe().unwrap());
            child
                .args(["--exact", "setter_lock::tests::child"])
                .env("RUSTDESK_AGENT_TEST_MUTEX", &name)
                .env_remove("RUSTDESK_AGENT_TEST_BUSY");
            if busy {
                child.env("RUSTDESK_AGENT_TEST_BUSY", "1");
            }
            assert!(child.status().unwrap().success());
        };
        run(true);
        drop(owner);
        run(false);
    }

    #[test]
    fn abandoned_owner_fails_closed_then_releases() {
        let name = format!("Global\\RustDeskAgent.Test.{}", uuid::Uuid::new_v4());
        let other = name.clone();
        // Simulate owner-thread death while another handle keeps the object alive.
        let leaked = thread::spawn(move || {
            let owner = SettingsLock::acquire_named(&other, 0).unwrap();
            let handle = owner.handle as usize;
            std::mem::forget(owner);
            handle
        })
        .join()
        .unwrap();
        let error = SettingsLock::acquire_named(&name, 30).err().unwrap();
        assert_eq!(error.kind(), io::ErrorKind::Interrupted);
        drop(SettingsLock::acquire_named(&name, 30).unwrap());
        unsafe {
            CloseHandle(leaked as HANDLE);
        }
    }

    #[test]
    fn contention_times_out_then_acquires_after_release() {
        let name = format!("Global\\RustDeskAgent.Test.{}", uuid::Uuid::new_v4());
        let owner = SettingsLock::acquire_named(&name, 0).unwrap();
        let other = name.clone();
        let error = thread::spawn(move || {
            SettingsLock::acquire_named(&other, 30)
                .err()
                .map(|e| e.kind())
        })
        .join()
        .unwrap();
        assert_eq!(error, Some(io::ErrorKind::WouldBlock));
        drop(owner);
        thread::spawn(move || {
            SettingsLock::acquire_named(&name, 30).unwrap();
        })
        .join()
        .unwrap();
    }

    #[test]
    fn waiter_acquires_after_owner_drops() {
        let name = format!("Global\\RustDeskAgent.Test.{}", uuid::Uuid::new_v4());
        let owner = SettingsLock::acquire_named(&name, 0).unwrap();
        let (sent, received) = mpsc::channel();
        let waiter = thread::spawn(move || {
            let _guard = SettingsLock::acquire_named(&name, 2_000).unwrap();
            sent.send(()).unwrap();
        });
        assert!(
            received
                .recv_timeout(std::time::Duration::from_millis(50))
                .is_err()
        );
        drop(owner);
        received
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
        waiter.join().unwrap();
    }
}
