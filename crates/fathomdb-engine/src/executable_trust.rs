use std::fs;
use std::path::Path;

use crate::admin::{
    VectorGeneratorPolicy, VectorRegenerationFailure, VectorRegenerationFailureClass,
};

pub(crate) fn validate_generator_executable(
    executable: &str,
    policy: &VectorGeneratorPolicy,
) -> Result<(), VectorRegenerationFailure> {
    let path = Path::new(executable);
    if policy.require_absolute_executable && !path.is_absolute() {
        return Err(VectorRegenerationFailure::new(
            VectorRegenerationFailureClass::InvalidContract,
            format!("generator executable '{executable}' must be an absolute path"),
        ));
    }

    let metadata = fs::metadata(path).map_err(|error| {
        VectorRegenerationFailure::new(
            VectorRegenerationFailureClass::InvalidContract,
            format!("generator executable '{executable}' is not accessible: {error}"),
        )
    })?;

    if !policy.allowed_executable_roots.is_empty() {
        let canonical = fs::canonicalize(path).map_err(|error| {
            VectorRegenerationFailure::new(
                VectorRegenerationFailureClass::InvalidContract,
                format!("generator executable '{executable}' cannot be canonicalized: {error}"),
            )
        })?;
        let mut matched = false;
        for root in &policy.allowed_executable_roots {
            let root_canonical = fs::canonicalize(Path::new(root)).map_err(|error| {
                VectorRegenerationFailure::new(
                    VectorRegenerationFailureClass::InvalidContract,
                    format!("allowed executable root '{root}' cannot be canonicalized: {error}"),
                )
            })?;
            if canonical.starts_with(&root_canonical) {
                matched = true;
                break;
            }
        }
        if !matched {
            return Err(VectorRegenerationFailure::new(
                VectorRegenerationFailureClass::InvalidContract,
                format!("generator executable '{executable}' is outside allowed executable roots"),
            ));
        }
    }

    if policy.reject_world_writable_executable {
        reject_broadly_writable(path, executable, &metadata)?;
    }

    Ok(())
}

#[cfg(unix)]
fn reject_broadly_writable(
    _path: &Path,
    executable: &str,
    metadata: &fs::Metadata,
) -> Result<(), VectorRegenerationFailure> {
    use std::os::unix::fs::PermissionsExt;

    if metadata.permissions().mode() & 0o002 != 0 {
        return Err(VectorRegenerationFailure::new(
            VectorRegenerationFailureClass::InvalidContract,
            format!("generator executable '{executable}' is a world-writable executable"),
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn reject_broadly_writable(
    path: &Path,
    executable: &str,
    _metadata: &fs::Metadata,
) -> Result<(), VectorRegenerationFailure> {
    if windows_acl::is_broadly_writable(path).map_err(|error| {
        VectorRegenerationFailure::new(
            VectorRegenerationFailureClass::InvalidContract,
            format!("failed to inspect executable ACLs for '{executable}': {error}"),
        )
    })? {
        return Err(VectorRegenerationFailure::new(
            VectorRegenerationFailureClass::InvalidContract,
            format!("generator executable '{executable}' is a broadly writable executable"),
        ));
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn reject_broadly_writable(
    _path: &Path,
    _executable: &str,
    _metadata: &fs::Metadata,
) -> Result<(), VectorRegenerationFailure> {
    Ok(())
}

#[cfg(windows)]
mod windows_acl {
    use std::ffi::{OsStr, c_void};
    use std::io;
    use std::mem::size_of;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use std::ptr::{null, null_mut};

    use windows_sys::Win32::Foundation::{ERROR_SUCCESS, GENERIC_WRITE, LocalFree};
    use windows_sys::Win32::Security::Authorization::{GetNamedSecurityInfoW, SE_FILE_OBJECT};
    use windows_sys::Win32::Security::{
        ACCESS_ALLOWED_ACE, ACE_HEADER, ACL, ACL_SIZE_INFORMATION, AclSizeInformation,
        CreateWellKnownSid, DACL_SECURITY_INFORMATION, EqualSid, GetAce, GetAclInformation, PSID,
        WinAuthenticatedUserSid, WinBuiltinUsersSid, WinWorldSid,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_APPEND_DATA, FILE_GENERIC_WRITE, FILE_WRITE_DATA,
    };
    use windows_sys::Win32::System::SystemServices::ACCESS_ALLOWED_ACE_TYPE;

    // Standard Windows access rights not re-exported by windows-sys 0.59.
    const WRITE_DAC: u32 = 0x0004_0000;
    const WRITE_OWNER: u32 = 0x0008_0000;

    const WRITE_MASK: u32 = FILE_WRITE_DATA
        | FILE_APPEND_DATA
        | FILE_GENERIC_WRITE
        | GENERIC_WRITE
        | WRITE_DAC
        | WRITE_OWNER;

    pub(crate) fn is_broadly_writable(path: &Path) -> io::Result<bool> {
        let path_w = to_wide(path.as_os_str());
        let mut dacl: *mut ACL = null_mut();
        let mut security_descriptor: *mut c_void = null_mut();
        let status = unsafe {
            GetNamedSecurityInfoW(
                path_w.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                &mut dacl,
                null_mut(),
                &mut security_descriptor,
            )
        };
        if status != ERROR_SUCCESS {
            return Err(io::Error::from_raw_os_error(status as i32));
        }
        let security_descriptor = SecurityDescriptor(security_descriptor);

        if dacl.is_null() {
            return Ok(true);
        }

        let mut acl_info = ACL_SIZE_INFORMATION {
            AceCount: 0,
            AclBytesInUse: 0,
            AclBytesFree: 0,
        };
        let ok = unsafe {
            GetAclInformation(
                dacl.cast(),
                (&mut acl_info as *mut ACL_SIZE_INFORMATION).cast(),
                size_of::<ACL_SIZE_INFORMATION>() as u32,
                AclSizeInformation,
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }

        let broad_sids = [
            well_known_sid(WinWorldSid)?,
            well_known_sid(WinAuthenticatedUserSid)?,
            well_known_sid(WinBuiltinUsersSid)?,
        ];
        for index in 0..acl_info.AceCount {
            let mut ace_ptr: *mut c_void = null_mut();
            let ok = unsafe { GetAce(dacl, index, &mut ace_ptr) };
            if ok == 0 {
                return Err(io::Error::last_os_error());
            }
            let header = unsafe { &*(ace_ptr as *const ACE_HEADER) };
            if header.AceType as u32 != ACCESS_ALLOWED_ACE_TYPE {
                continue;
            }
            let ace = unsafe { &*(ace_ptr as *const ACCESS_ALLOWED_ACE) };
            if ace.Mask & WRITE_MASK == 0 {
                continue;
            }
            let sid = unsafe { (&ace.SidStart as *const u32).cast_mut().cast() };
            if broad_sids
                .iter()
                .any(|candidate| unsafe { EqualSid(sid, candidate.as_ptr().cast()) } != 0)
            {
                drop(security_descriptor);
                return Ok(true);
            }
        }

        drop(security_descriptor);
        Ok(false)
    }

    fn to_wide(value: &OsStr) -> Vec<u16> {
        value.encode_wide().chain(std::iter::once(0)).collect()
    }

    fn well_known_sid(kind: u32) -> io::Result<Vec<u8>> {
        let mut size = windows_sys::Win32::Security::SECURITY_MAX_SID_SIZE as u32;
        let mut buffer = vec![0u8; size as usize];
        let ok = unsafe { CreateWellKnownSid(kind, null(), buffer.as_mut_ptr().cast(), &mut size) };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        buffer.truncate(size as usize);
        Ok(buffer)
    }

    struct SecurityDescriptor(*mut c_void);

    impl Drop for SecurityDescriptor {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    LocalFree(self.0);
                }
            }
        }
    }
}
