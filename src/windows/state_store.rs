//! Protected local snapshot replacement. Initialization is administrator-only.
use crate::{
    credentials::{
        self, LocalMemory, check, descriptor, open, security_attributes, verify_security, wide,
    },
    enrollment::EnrollmentInfo,
    state::{DurableState, MAX_STATE_BYTES, StateStore},
};
use std::{
    fs::File,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};
use windows_sys::Win32::{
    Security::{DACL_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION},
    Storage::FileSystem::*,
};

const STATE: &str = "state.json";
const TEMP: &str = "state.next";
const LOCK: &str = "state.lock";

fn root() -> io::Result<PathBuf> {
    Ok(credentials::root()?.with_file_name("RustDeskAgentState"))
}
fn policy(sid: &str, directory: bool) -> io::Result<LocalMemory> {
    // Files use the creating account's owner; service tokens need not own the Administrators SID.
    descriptor(&format!(
        "{}D:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;FRFWFXSD;;;{sid})(A;;RC;;;OW)",
        if directory { "O:BA" } else { "" }
    ))
}
fn verify_file(file: &File, policy: &LocalMemory) -> io::Result<()> {
    verify_security(file, policy, false, DACL_SECURITY_INFORMATION)
}

pub struct FileStore {
    path: PathBuf,
    _directory: File,
    _lock: File,
    policy: LocalMemory,
}
impl FileStore {
    pub fn open() -> io::Result<Self> {
        Self::open_at(&root()?, &credentials::service_sid()?)
    }
    fn open_at(path: &Path, sid: &str) -> io::Result<Self> {
        let directory = open(path, None, true)?;
        verify_security(
            &directory,
            &policy(sid, true)?,
            true,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
        )?;
        let policy = policy(sid, false)?;
        let lock = open(&path.join(LOCK), None, false)?;
        verify_file(&lock, &policy)?;
        lock.try_lock()
            .map_err(|_| io::Error::other("state store is already in use"))?;
        Ok(Self {
            path: path.into(),
            _directory: directory,
            _lock: lock,
            policy,
        })
    }
}
impl StateStore for FileStore {
    fn load(&mut self) -> io::Result<Vec<u8>> {
        let file = open(&self.path.join(STATE), None, false)?;
        verify_file(&file, &self.policy)?;
        let mut bytes = Vec::new();
        file.take((MAX_STATE_BYTES + 1) as u64)
            .read_to_end(&mut bytes)?;
        if bytes.is_empty() || bytes.len() > MAX_STATE_BYTES {
            return Err(io::Error::other("invalid state store"));
        }
        Ok(bytes)
    }
    fn save(&mut self, bytes: &[u8]) -> io::Result<()> {
        if bytes.is_empty() || bytes.len() > MAX_STATE_BYTES {
            return Err(io::Error::other("invalid state size"));
        }
        // Refuse a missing/replaced current state, even after startup.
        let current = open(&self.path.join(STATE), None, false)?;
        verify_file(&current, &self.policy)?;
        drop(current);
        let temp = self.path.join(TEMP);
        match open(&temp, None, false) {
            Ok(file) => {
                verify_file(&file, &self.policy)?;
                drop(file);
                std::fs::remove_file(&temp)
                    .map_err(|e| io::Error::new(e.kind(), format!("remove staged state: {e}")))?;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => (),
            Err(error) => return Err(error),
        }
        let mut file = open(&temp, Some(&self.policy), false)
            .map_err(|e| io::Error::new(e.kind(), format!("create staged state: {e}")))?;
        verify_file(&file, &self.policy)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        check(unsafe {
            MoveFileExW(
                wide(&temp).as_ptr(),
                wide(self.path.join(STATE)).as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        })
        .map_err(|e| io::Error::new(e.kind(), format!("replace committed state: {e}")))
    }
}

pub fn initialize() -> io::Result<()> {
    credentials::require_admin()?;
    let enrollment = credentials::load()?;
    initialize_at(&root()?, &credentials::service_sid()?, &enrollment.info())
}
fn initialize_at(path: &Path, sid: &str, info: &EnrollmentInfo) -> io::Result<()> {
    let directory_policy = policy(sid, true)?;
    check(unsafe {
        CreateDirectoryW(wide(path).as_ptr(), &security_attributes(&directory_policy))
    })?;
    let directory = open(path, None, true)?;
    verify_security(
        &directory,
        &directory_policy,
        true,
        OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
    )?;
    let file_policy = policy(sid, false)?;
    let lock = open(&path.join(LOCK), Some(&file_policy), false)?;
    verify_file(&lock, &file_policy)?;
    lock.sync_all()?;
    let mut file = open(&path.join(STATE), Some(&file_policy), false)?;
    verify_file(&file, &file_policy)?;
    file.write_all(
        &DurableState::initial(info)
            .encode()
            .map_err(io::Error::other)?,
    )?;
    file.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn protected_state_replaces_reopens_and_excludes_other_writers() {
        credentials::require_admin().unwrap();
        let path =
            std::env::temp_dir().join(format!("rustdesk-state-test-{}", uuid::Uuid::new_v4()));
        let sid = "S-1-5-80-1-2-3-4-5";
        let info = EnrollmentInfo {
            agent_uuid: uuid::Uuid::new_v4(),
            management_origin: url::Url::parse("https://example.test").unwrap(),
        };
        initialize_at(&path, sid, &info).unwrap();
        assert!(initialize_at(&path, sid, &info).is_err());
        let mut store = FileStore::open_at(&path, sid).unwrap();
        assert!(FileStore::open_at(&path, sid).is_err());
        let initial = store.load().unwrap();
        let mut state = DurableState::decode(&initial, &info).unwrap();
        let desired = crate::parse_desired(
            br#"{"schema_version":1,"revision":7,"settings":{"approve_mode":"click"}}"#,
        )
        .unwrap();
        state.observe(&desired, None, None).unwrap();
        // Interrupted staging must not change committed state.
        let mut temp = open(&path.join(TEMP), Some(&store.policy), false).unwrap();
        temp.write_all(b"partial").unwrap();
        drop(temp);
        assert_eq!(store.load().unwrap(), initial);
        let updated = state.encode().unwrap();
        store.save(&updated).unwrap();
        drop(store);
        let mut store = FileStore::open_at(&path, sid).unwrap();
        assert_eq!(store.load().unwrap(), updated);
        assert!(FileStore::open_at(&path, "S-1-5-80-5-4-3-2-1").is_err());
        std::fs::remove_file(path.join(STATE)).unwrap();
        assert!(store.load().is_err());
        assert!(store.save(&updated).is_err());
        drop(store);
        std::fs::remove_file(path.join(LOCK)).unwrap();
        std::fs::remove_dir(path).unwrap();
    }
}
