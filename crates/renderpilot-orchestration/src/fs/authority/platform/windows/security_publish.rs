//! Security derivation for publishing a private staged entry into a public
//! game directory.
//!
//! A private workspace deliberately has an owner-only DACL.  Moving an entry
//! out of it retains that descriptor, so publication derives the destination
//! parent's inheritable DACL onto the retained source *before* the no-replace
//! rename.  This keeps the private entry private until it becomes public and
//! does not alter custody restores, which use the ordinary rename path.

use crate::ServiceError;
use std::fs::File;

#[cfg(windows)]
struct LocalAcl(std::ptr::NonNull<windows_sys::Win32::Security::ACL>);

#[cfg(windows)]
#[expect(
    unsafe_code,
    reason = "Releasing the LocalAlloc-owned ACL through its required Windows API"
)]
impl Drop for LocalAcl {
    fn drop(&mut self) {
        // The normalized ACL is allocated by LocalAlloc below.
        unsafe { windows_sys::Win32::Foundation::LocalFree(self.0.as_ptr().cast()) };
    }
}

/// Copies an inherited DACL into its declared allocation and makes its ACEs
/// applicable to the published object. `SetSecurityInfo` receives an ACL, not
/// a full inheritance operation; inherited ACE markers must therefore be
/// cleared on the copy. All other ACE fields and ACL slack are preserved.
#[cfg(windows)]
#[expect(
    unsafe_code,
    reason = "Windows ACL copy and bounded ACE inspection for staged publication"
)]
fn normalized_publish_dacl(
    dacl: *const windows_sys::Win32::Security::ACL,
) -> Result<LocalAcl, ServiceError> {
    use windows_sys::Win32::{
        Security::{
            ACE_HEADER, ACL, ACL_SIZE_INFORMATION, AclSizeInformation, GetAce, GetAclInformation,
            INHERITED_ACE, IsValidAcl,
        },
        System::Memory::{LMEM_FIXED, LMEM_ZEROINIT, LocalAlloc},
    };

    if dacl.is_null() || unsafe { IsValidAcl(dacl) } == 0 {
        return Err(crate::failed(
            "derived staged publication security descriptor has no valid DACL",
        ));
    }
    let mut info = ACL_SIZE_INFORMATION::default();
    if unsafe {
        GetAclInformation(
            dacl,
            (&raw mut info).cast(),
            u32::try_from(std::mem::size_of::<ACL_SIZE_INFORMATION>())
                .map_err(|_| crate::failed("Windows ACL information buffer is too large"))?,
            AclSizeInformation,
        )
    } == 0
    {
        return Err(crate::failed(format!(
            "failed to inspect derived staged publication DACL: {}",
            std::io::Error::last_os_error()
        )));
    }
    let acl_size = usize::from(unsafe { (*dacl).AclSize });
    let bytes_in_use = usize::try_from(info.AclBytesInUse)
        .map_err(|_| crate::failed("derived staged publication DACL is too large"))?;
    let header_size = std::mem::size_of::<ACL>();
    if acl_size < header_size || bytes_in_use < header_size || bytes_in_use > acl_size {
        return Err(crate::failed(
            "derived staged publication DACL has an invalid used-byte range",
        ));
    }
    let copied = unsafe { LocalAlloc(LMEM_FIXED | LMEM_ZEROINIT, acl_size) }.cast::<ACL>();
    let copied = std::ptr::NonNull::new(copied).ok_or_else(|| {
        crate::failed(format!(
            "failed to allocate staged publication DACL: {}",
            std::io::Error::last_os_error()
        ))
    })?;
    let copied = LocalAcl(copied);
    // Copy only initialized ACL bytes. The LocalAlloc buffer keeps any
    // declared ACL slack zeroed while preserving its exact allocation size.
    unsafe {
        std::ptr::copy_nonoverlapping(
            dacl.cast::<u8>(),
            copied.0.as_ptr().cast::<u8>(),
            bytes_in_use,
        );
    }
    let dacl_start = dacl as usize;
    let dacl_end = dacl_start
        .checked_add(bytes_in_use)
        .ok_or_else(|| crate::failed("derived staged publication DACL range overflow"))?;
    let mut prior_end = dacl_start
        .checked_add(header_size)
        .ok_or_else(|| crate::failed("derived staged publication DACL range overflow"))?;
    for index in 0..info.AceCount {
        let mut ace = std::ptr::null_mut();
        if unsafe { GetAce(dacl, index, &raw mut ace) } == 0 || ace.is_null() {
            return Err(crate::failed(
                "derived staged publication DACL ACE is unreadable",
            ));
        }
        let ace_start = ace as usize;
        let header_end = ace_start
            .checked_add(std::mem::size_of::<ACE_HEADER>())
            .ok_or_else(|| crate::failed("derived staged publication DACL ACE range overflow"))?;
        if ace_start < prior_end || ace_start < dacl_start || header_end > dacl_end {
            return Err(crate::failed(
                "derived staged publication DACL ACE lies outside its used range",
            ));
        }
        let header = unsafe { &*ace.cast::<ACE_HEADER>() };
        let ace_size = usize::from(header.AceSize);
        let ace_end = ace_start
            .checked_add(ace_size)
            .ok_or_else(|| crate::failed("derived staged publication DACL ACE range overflow"))?;
        if ace_size < std::mem::size_of::<ACE_HEADER>() || ace_end > dacl_end {
            return Err(crate::failed(
                "derived staged publication DACL ACE has an invalid size",
            ));
        }
        let offset = ace_start
            .checked_sub(dacl_start)
            .ok_or_else(|| crate::failed("derived staged publication DACL ACE offset underflow"))?;
        let copied_ace_flags_offset = offset
            .checked_add(std::mem::offset_of!(ACE_HEADER, AceFlags))
            .ok_or_else(|| {
                crate::failed("derived staged publication DACL ACE flags offset overflow")
            })?;
        if copied_ace_flags_offset >= bytes_in_use {
            return Err(crate::failed(
                "derived staged publication DACL ACE flags lie outside its used range",
            ));
        }
        let copied_ace_flags =
            unsafe { copied.0.as_ptr().cast::<u8>().add(copied_ace_flags_offset) };
        unsafe { *copied_ace_flags &= !(INHERITED_ACE as u8) };
        prior_end = ace_end;
    }
    if unsafe { IsValidAcl(copied.0.as_ptr()) } == 0 {
        return Err(crate::failed(
            "normalized staged publication DACL is invalid",
        ));
    }
    Ok(copied)
}

/// Applies the destination parent's inherited DACL to a staged entry through
/// retained handles.  Ownership, SACL, and the source object's other security
/// fields are intentionally not changed.
#[expect(
    unsafe_code,
    reason = "Windows retained-handle ACL inheritance for publication"
)]
pub(crate) fn windows_prepare_staged_publish_security(
    destination_parent: &File,
    source: &File,
    is_container: bool,
) -> Result<(), ServiceError> {
    use std::os::windows::io::{AsRawHandle, FromRawHandle};
    use windows_sys::Win32::{
        Foundation::{HANDLE, WIN32_ERROR},
        Security::Authorization::{GetSecurityInfo, SE_FILE_OBJECT, SetSecurityInfo},
        Security::{
            ACL, DACL_SECURITY_INFORMATION, DestroyPrivateObjectSecurity, GENERIC_MAPPING,
            GetSecurityDescriptorDacl, IsValidAcl, IsValidSecurityDescriptor, PSECURITY_DESCRIPTOR,
            SECURITY_AUTO_INHERIT_FLAGS, SEF_DACL_AUTO_INHERIT,
        },
        Storage::FileSystem::{
            FILE_ALL_ACCESS, FILE_GENERIC_EXECUTE, FILE_GENERIC_READ, FILE_GENERIC_WRITE,
        },
        System::Threading::{GetCurrentProcess, OpenProcessToken},
    };

    struct LocalSecurityDescriptor(PSECURITY_DESCRIPTOR);
    impl Drop for LocalSecurityDescriptor {
        fn drop(&mut self) {
            if !self.0.is_null() {
                // GetSecurityInfo allocates this descriptor with LocalAlloc.
                unsafe { windows_sys::Win32::Foundation::LocalFree(self.0.cast()) };
            }
        }
    }

    struct PrivateSecurityDescriptor(PSECURITY_DESCRIPTOR);
    impl Drop for PrivateSecurityDescriptor {
        fn drop(&mut self) {
            if !self.0.is_null() {
                // CreatePrivateObjectSecurityEx has its own paired release API.
                unsafe { DestroyPrivateObjectSecurity(&raw mut self.0) };
            }
        }
    }

    let mut parent_descriptor = std::ptr::null_mut();
    let mut parent_dacl: *mut ACL = std::ptr::null_mut();
    let status: WIN32_ERROR = unsafe {
        GetSecurityInfo(
            destination_parent.as_raw_handle() as HANDLE,
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &raw mut parent_dacl,
            std::ptr::null_mut(),
            &raw mut parent_descriptor,
        )
    };
    if status != 0 || parent_descriptor.is_null() || parent_dacl.is_null() {
        return Err(crate::failed(format!(
            "failed to read destination parent DACL: Windows error {status}"
        )));
    }
    let _parent_descriptor = LocalSecurityDescriptor(parent_descriptor);
    if unsafe { IsValidSecurityDescriptor(parent_descriptor) } == 0 {
        return Err(crate::failed(
            "destination parent returned an invalid security descriptor",
        ));
    }

    let mut token: HANDLE = std::ptr::null_mut();
    if unsafe {
        OpenProcessToken(
            GetCurrentProcess(),
            windows_sys::Win32::Security::TOKEN_QUERY,
            &raw mut token,
        )
    } == 0
    {
        return Err(crate::failed(format!(
            "failed to open current process token for staged publication: {}",
            std::io::Error::last_os_error()
        )));
    }
    let _token = unsafe { File::from_raw_handle(token) };
    let mapping = GENERIC_MAPPING {
        GenericRead: FILE_GENERIC_READ,
        GenericWrite: FILE_GENERIC_WRITE,
        GenericExecute: FILE_GENERIC_EXECUTE,
        GenericAll: FILE_ALL_ACCESS,
    };
    let mut derived = std::ptr::null_mut();
    if unsafe {
        windows_sys::Win32::Security::CreatePrivateObjectSecurityEx(
            parent_descriptor,
            std::ptr::null_mut(),
            &raw mut derived,
            std::ptr::null(),
            i32::from(is_container),
            SEF_DACL_AUTO_INHERIT as SECURITY_AUTO_INHERIT_FLAGS,
            token,
            &raw const mapping,
        )
    } == 0
        || derived.is_null()
    {
        return Err(crate::failed(format!(
            "failed to derive staged publication DACL: {}",
            std::io::Error::last_os_error()
        )));
    }
    let _derived = PrivateSecurityDescriptor(derived);
    if unsafe { IsValidSecurityDescriptor(derived) } == 0 {
        return Err(crate::failed(
            "derived staged publication security descriptor is invalid",
        ));
    }
    let mut present = 0;
    let mut dacl: *mut ACL = std::ptr::null_mut();
    let mut defaulted = 0;
    if unsafe {
        GetSecurityDescriptorDacl(derived, &raw mut present, &raw mut dacl, &raw mut defaulted)
    } == 0
        || present == 0
        || dacl.is_null()
        || unsafe { IsValidAcl(dacl) } == 0
    {
        return Err(crate::failed(
            "derived staged publication security descriptor has no valid DACL",
        ));
    }
    let normalized_dacl = normalized_publish_dacl(dacl)?;
    let status = unsafe {
        SetSecurityInfo(
            source.as_raw_handle() as HANDLE,
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            normalized_dacl.0.as_ptr(),
            std::ptr::null_mut(),
        )
    };
    if status != 0 {
        return Err(crate::failed(format!(
            "failed to apply staged publication DACL: Windows error {status}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[expect(
        unsafe_code,
        reason = "Constructing a Windows ACL fixture with explicit allocation slack"
    )]
    fn inherited_acl_with_slack() -> Vec<usize> {
        use super::super::security_create::windows_current_user_sid;
        use windows_sys::Win32::{
            Security::{
                ACCESS_ALLOWED_ACE, ACL, ACL_REVISION, AddAccessAllowedAceEx, GetLengthSid,
                INHERITED_ACE, InitializeAcl,
            },
            Storage::FileSystem::FILE_GENERIC_READ,
        };

        let sid = windows_current_user_sid().expect("current user SID");
        let sid_pointer = sid.as_ptr().cast_mut().cast();
        let sid_length = unsafe { GetLengthSid(sid_pointer) };
        assert_ne!(sid_length, 0, "current user SID has a length");
        let ace_length = std::mem::size_of::<ACCESS_ALLOWED_ACE>()
            .checked_sub(std::mem::size_of::<u32>())
            .and_then(|length| length.checked_add(sid_length as usize))
            .expect("ACE length");
        let acl_length = std::mem::size_of::<ACL>()
            .checked_add(ace_length)
            .and_then(|length| length.checked_add(64))
            .expect("ACL length with slack");
        let acl_length_u32 = u32::try_from(acl_length).expect("ACL length fits Windows API");
        let mut storage = vec![0_usize; acl_length.div_ceil(std::mem::size_of::<usize>())];
        let acl = storage.as_mut_ptr().cast::<ACL>();
        assert_ne!(
            unsafe { InitializeAcl(acl, acl_length_u32, ACL_REVISION) },
            0
        );
        assert_ne!(
            unsafe {
                AddAccessAllowedAceEx(
                    acl,
                    ACL_REVISION,
                    INHERITED_ACE,
                    FILE_GENERIC_READ,
                    sid_pointer,
                )
            },
            0
        );
        storage
    }

    #[test]
    #[expect(
        unsafe_code,
        reason = "Inspecting copied Windows ACL fixture to prove bounded normalization"
    )]
    fn normalizes_only_inherited_markers_with_declared_acl_slack() {
        use windows_sys::Win32::Security::{
            ACE_HEADER, ACL, ACL_SIZE_INFORMATION, AclSizeInformation, GetAce, GetAclInformation,
            INHERITED_ACE,
        };

        let mut source_storage = inherited_acl_with_slack();
        let source = source_storage.as_mut_ptr().cast::<ACL>();
        let normalized = normalized_publish_dacl(source).expect("normalize inherited ACL");
        let mut source_info = ACL_SIZE_INFORMATION::default();
        let mut normalized_info = ACL_SIZE_INFORMATION::default();
        assert_ne!(
            unsafe {
                GetAclInformation(
                    source,
                    (&raw mut source_info).cast(),
                    u32::try_from(std::mem::size_of::<ACL_SIZE_INFORMATION>()).expect("size"),
                    AclSizeInformation,
                )
            },
            0
        );
        assert_ne!(
            unsafe {
                GetAclInformation(
                    normalized.0.as_ptr(),
                    (&raw mut normalized_info).cast(),
                    u32::try_from(std::mem::size_of::<ACL_SIZE_INFORMATION>()).expect("size"),
                    AclSizeInformation,
                )
            },
            0
        );
        assert!(
            source_info.AclBytesFree > 0,
            "fixture retains declared slack"
        );
        assert_eq!(unsafe { (*source).AclSize }, unsafe {
            (*normalized.0.as_ptr()).AclSize
        });
        assert_eq!(source_info.AceCount, normalized_info.AceCount);
        assert_eq!(source_info.AclBytesInUse, normalized_info.AclBytesInUse);

        let mut source_ace = std::ptr::null_mut();
        let mut normalized_ace = std::ptr::null_mut();
        assert_ne!(unsafe { GetAce(source, 0, &raw mut source_ace) }, 0);
        assert_ne!(
            unsafe { GetAce(normalized.0.as_ptr(), 0, &raw mut normalized_ace) },
            0
        );
        let source_header = unsafe { &*source_ace.cast::<ACE_HEADER>() };
        let normalized_header = unsafe { &*normalized_ace.cast::<ACE_HEADER>() };
        let ace_size = usize::from(source_header.AceSize);
        assert_eq!(source_header.AceType, normalized_header.AceType);
        assert_eq!(source_header.AceSize, normalized_header.AceSize);
        assert_eq!(
            normalized_header.AceFlags,
            source_header.AceFlags & !(INHERITED_ACE as u8)
        );
        let source_bytes = unsafe { std::slice::from_raw_parts(source_ace.cast::<u8>(), ace_size) };
        let normalized_bytes =
            unsafe { std::slice::from_raw_parts(normalized_ace.cast::<u8>(), ace_size) };
        assert!(
            source_bytes
                .iter()
                .zip(normalized_bytes)
                .enumerate()
                .all(|(index, (source, normalized))| { index == 1 || source == normalized }),
            "normalization changes only the ACE inherited marker byte"
        );
    }

    #[test]
    fn rejects_a_malformed_acl_before_allocating_or_walking_aces() {
        let malformed = windows_sys::Win32::Security::ACL::default();
        assert!(normalized_publish_dacl(&raw const malformed).is_err());
    }
}
