//! Machine DPAPI plus a protected, create-once store. No plaintext files.
use crate::enrollment::Enrollment;
use std::{
    ffi::{OsStr, OsString, c_void},
    fs::File,
    io::{self, Read, Write},
    os::windows::{
        ffi::{OsStrExt, OsStringExt},
        fs::MetadataExt,
        io::{AsRawHandle, FromRawHandle},
    },
    path::{Path, PathBuf},
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::{GENERIC_READ, GENERIC_WRITE, INVALID_HANDLE_VALUE, LocalFree},
    Security::{Authorization::*, Cryptography::*, *},
    Storage::FileSystem::*,
    System::Com::CoTaskMemFree,
    UI::Shell::{FOLDERID_ProgramData, SHGetKnownFolderPath},
};
use zeroize::{Zeroize, Zeroizing};

const FILE_NAME: &str = "enrollment.dpapi";
const SECURITY: u32 = OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION;

struct LocalMemory(*mut c_void);
impl Drop for LocalMemory {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.0);
        }
    }
}
fn wide(value: impl AsRef<OsStr>) -> Vec<u16> {
    value.as_ref().encode_wide().chain(Some(0)).collect()
}
fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "invalid or unprotected enrollment store",
    )
}
fn check(result: i32) -> io::Result<()> {
    if result == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
// Only called on NUL-terminated strings allocated by Windows APIs.
unsafe fn string(pointer: *const u16) -> OsString {
    let mut length = 0;
    while unsafe { *pointer.add(length) } != 0 {
        length += 1;
    }
    OsString::from_wide(unsafe { std::slice::from_raw_parts(pointer, length) })
}

fn root() -> io::Result<PathBuf> {
    let mut pointer = null_mut();
    let result =
        unsafe { SHGetKnownFolderPath(&FOLDERID_ProgramData, 0, null_mut(), &mut pointer) };
    if result < 0 {
        return Err(io::Error::other("cannot locate ProgramData"));
    }
    let path = PathBuf::from(unsafe { string(pointer) });
    unsafe {
        CoTaskMemFree(pointer.cast());
    }
    Ok(path.join("RustDeskAgentEnrollment"))
}

fn service_sid() -> io::Result<String> {
    let mut sid = [0u32; 17];
    let mut sid_size = std::mem::size_of_val(&sid) as u32;
    let mut domain = [0u16; 256];
    let mut domain_size = domain.len() as u32;
    let mut usage = 0;
    check(unsafe {
        LookupAccountNameW(
            null(),
            wide(r"NT SERVICE\RustDeskAgent").as_ptr(),
            sid.as_mut_ptr().cast(),
            &mut sid_size,
            domain.as_mut_ptr(),
            &mut domain_size,
            &mut usage,
        )
    })?;
    let mut text = null_mut();
    check(unsafe { ConvertSidToStringSidW(sid.as_mut_ptr().cast(), &mut text) })?;
    let _memory = LocalMemory(text.cast());
    let sid = unsafe { string(text) }.to_string_lossy().into_owned();
    if !sid.starts_with("S-1-5-80-") {
        return Err(invalid());
    }
    Ok(sid)
}

fn require_admin() -> io::Result<()> {
    let mut sid = null_mut();
    check(unsafe { ConvertStringSidToSidW(wide("S-1-5-32-544").as_ptr(), &mut sid) })?;
    let _memory = LocalMemory(sid);
    let mut member = 0;
    check(unsafe { CheckTokenMembership(null_mut(), sid, &mut member) })?;
    if member == 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "run enrollment as an elevated administrator",
        ));
    }
    Ok(())
}

fn policy(sid: &str) -> io::Result<LocalMemory> {
    // No inherited ACEs; service can read, administrators and SYSTEM can maintain.
    let sddl = format!("O:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;FRFX;;;{sid})");
    let mut descriptor = null_mut();
    check(unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            wide(sddl).as_ptr(),
            1,
            &mut descriptor,
            null_mut(),
        )
    })?;
    Ok(LocalMemory(descriptor))
}
fn security_attributes(descriptor: &LocalMemory) -> SECURITY_ATTRIBUTES {
    SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor.0,
        bInheritHandle: 0,
    }
}
fn descriptor_text(descriptor: *mut c_void) -> io::Result<OsString> {
    let mut text = null_mut();
    check(unsafe {
        ConvertSecurityDescriptorToStringSecurityDescriptorW(
            descriptor,
            1,
            SECURITY,
            &mut text,
            null_mut(),
        )
    })?;
    let _memory = LocalMemory(text.cast());
    Ok(unsafe { string(text) })
}
fn verify(file: &File, expected: &LocalMemory, directory: bool) -> io::Result<()> {
    let metadata = file.metadata()?;
    if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
        || metadata.is_dir() != directory
    {
        return Err(invalid());
    }
    let mut actual = null_mut();
    let result = unsafe {
        GetSecurityInfo(
            file.as_raw_handle(),
            SE_FILE_OBJECT,
            SECURITY,
            null_mut(),
            null_mut(),
            null_mut(),
            null_mut(),
            &mut actual,
        )
    };
    if result != 0 {
        return Err(io::Error::from_raw_os_error(result as i32));
    }
    let actual = LocalMemory(actual);
    if descriptor_text(actual.0)? != descriptor_text(expected.0)? {
        return Err(invalid());
    }
    Ok(())
}
fn open(path: &Path, create: Option<&LocalMemory>, directory: bool) -> io::Result<File> {
    let attributes = create.map(security_attributes);
    let access = GENERIC_READ | if create.is_some() { GENERIC_WRITE } else { 0 };
    let handle = unsafe {
        CreateFileW(
            wide(path).as_ptr(),
            access,
            if create.is_some() { 0 } else { FILE_SHARE_READ },
            attributes.as_ref().map_or(null(), |v| v),
            if create.is_some() {
                CREATE_NEW
            } else {
                OPEN_EXISTING
            },
            FILE_FLAG_OPEN_REPARSE_POINT
                | if directory {
                    FILE_FLAG_BACKUP_SEMANTICS
                } else {
                    0
                },
            null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { File::from_raw_handle(handle) })
}

fn protect(bytes: &[u8], decrypt: bool) -> io::Result<Zeroizing<Vec<u8>>> {
    let input = CRYPT_INTEGER_BLOB {
        cbData: bytes.len().try_into().map_err(|_| invalid())?,
        pbData: bytes.as_ptr().cast_mut(),
    };
    let mut output: CRYPT_INTEGER_BLOB = unsafe { std::mem::zeroed() };
    let result = unsafe {
        if decrypt {
            CryptUnprotectData(
                &input,
                null_mut(),
                null(),
                null(),
                null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptProtectData(
                &input,
                null(),
                null(),
                null(),
                null(),
                CRYPTPROTECT_UI_FORBIDDEN | CRYPTPROTECT_LOCAL_MACHINE,
                &mut output,
            )
        }
    };
    check(result)?;
    let _memory = LocalMemory(output.pbData.cast());
    if output.pbData.is_null() || output.cbData == 0 {
        return Err(invalid());
    }
    let data = unsafe { std::slice::from_raw_parts_mut(output.pbData, output.cbData as usize) };
    let result = Zeroizing::new(data.to_vec());
    data.zeroize();
    Ok(result)
}

fn save_at(path: &Path, descriptor: &LocalMemory, enrollment: &Enrollment) -> io::Result<()> {
    let encrypted = protect(&enrollment.encode().map_err(|_| invalid())?, false)?;
    // Existing directories are never adopted, repaired or overwritten.
    check(unsafe { CreateDirectoryW(wide(path).as_ptr(), &security_attributes(descriptor)) })?;
    let directory = open(path, None, true)?;
    verify(&directory, descriptor, true)?;
    let mut file = open(&path.join(FILE_NAME), Some(descriptor), false)?;
    verify(&file, descriptor, false)?;
    file.write_all(&encrypted)?;
    file.sync_all()?;
    Ok(())
}
fn load_at(path: &Path, descriptor: &LocalMemory) -> io::Result<Enrollment> {
    let directory = open(path, None, true)?;
    verify(&directory, descriptor, true)?;
    let file = open(&path.join(FILE_NAME), None, false).map_err(|_| invalid())?;
    verify(&file, descriptor, false)?;
    let mut encrypted = Zeroizing::new(Vec::new());
    file.take(16385).read_to_end(&mut encrypted)?;
    if encrypted.len() > 16384 || encrypted.is_empty() {
        return Err(invalid());
    }
    let plaintext = protect(&encrypted, true).map_err(|_| invalid())?;
    Enrollment::decode(&plaintext).map_err(|_| invalid())
}

pub fn enroll(
    origin: &str,
    credential: Zeroizing<String>,
) -> io::Result<crate::enrollment::EnrollmentInfo> {
    require_admin()?;
    let descriptor = policy(&service_sid()?)?;
    let enrollment = Enrollment::new(origin, credential).map_err(io::Error::other)?;
    save_at(&root()?, &descriptor, &enrollment)?;
    Ok(enrollment.info())
}
pub fn load() -> io::Result<Enrollment> {
    let path = root()?;
    // A missing store is distinct from a corrupt/inaccessible existing store.
    let metadata = std::fs::symlink_metadata(&path)?;
    if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(invalid());
    }
    load_at(&path, &policy(&service_sid()?)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dpapi_round_trip_and_tamper_rejection() {
        let plaintext = b"disposable-test-credential-0123456789abcdef";
        let mut encrypted = protect(plaintext, false).unwrap();
        assert!(
            !encrypted
                .windows(plaintext.len())
                .any(|part| part == plaintext)
        );
        assert_eq!(protect(&encrypted, true).unwrap().as_slice(), plaintext);
        let index = encrypted.len() / 2;
        encrypted[index] ^= 1;
        assert!(protect(&encrypted, true).is_err());
    }

    #[test]
    fn protected_store_is_create_once_and_keeps_identity() {
        // Requires an elevated test runner; this is not service-account validation.
        require_admin().unwrap();
        let path =
            std::env::temp_dir().join(format!("rustdesk-enrollment-test-{}", uuid::Uuid::new_v4()));
        let descriptor = policy("S-1-5-80-1-2-3-4-5").unwrap();
        let enrollment = Enrollment::new(
            "https://example.test",
            Zeroizing::new("disposable-test-credential-0123456789abcdef".into()),
        )
        .unwrap();
        save_at(&path, &descriptor, &enrollment).unwrap();
        assert_eq!(
            load_at(&path, &descriptor).unwrap().info(),
            enrollment.info()
        );
        assert!(save_at(&path, &descriptor, &enrollment).is_err());
        let different_policy = policy("S-1-5-80-5-4-3-2-1").unwrap();
        assert!(load_at(&path, &different_policy).is_err());
        std::fs::write(path.join(FILE_NAME), b"corrupt").unwrap();
        assert!(load_at(&path, &descriptor).is_err());
        std::fs::remove_file(path.join(FILE_NAME)).unwrap();
        std::fs::remove_dir(path).unwrap();
    }
}
