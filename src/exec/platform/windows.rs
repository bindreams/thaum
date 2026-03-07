//! Windows implementations of platform-specific queries.
//!
//! Uses the `windows` crate for safe Win32 API access.

use std::mem::MaybeUninit;

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Security::Authorization::{GetSecurityInfo, SE_FILE_OBJECT};
use windows::Win32::Security::{
    EqualSid, GetLengthSid, GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, TokenElevation,
    TokenGroups, TokenUser, GROUP_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION, PSID, SID_AND_ATTRIBUTES,
    TOKEN_ELEVATION, TOKEN_GROUPS, TOKEN_QUERY, TOKEN_USER,
};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, GetFileInformationByHandle, GetFileType, BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_NORMAL,
    FILE_FLAG_BACKUP_SEMANTICS, FILE_GENERIC_READ, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_TYPE_PIPE, OPEN_EXISTING,
};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

// File ownership ======================================================================================================

pub fn file_owned_by_current_user(path: &str) -> bool {
    file_sid_matches(path, true)
}

pub fn file_group_matches_current_user(path: &str) -> bool {
    file_sid_matches(path, false)
}

/// Compare the file's owner (or group) SID against the current process token's
/// user SID.
fn file_sid_matches(path: &str, check_owner: bool) -> bool {
    let wide_path: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();

    let Ok(handle) = (unsafe {
        CreateFileW(
            windows::core::PCWSTR(wide_path.as_ptr()),
            FILE_GENERIC_READ.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_ATTRIBUTE_NORMAL,
            None,
        )
    }) else {
        return false;
    };

    let info_type = if check_owner {
        OWNER_SECURITY_INFORMATION
    } else {
        GROUP_SECURITY_INFORMATION
    };

    let mut file_sid = PSID::default();
    let result = unsafe {
        GetSecurityInfo(
            handle,
            SE_FILE_OBJECT,
            info_type,
            if check_owner { Some(&mut file_sid) } else { None },
            if check_owner { None } else { Some(&mut file_sid) },
            None,
            None,
            None,
        )
    };
    let _ = unsafe { CloseHandle(handle) };

    if result.is_err() || file_sid.is_invalid() {
        return false;
    }

    let Some(token_sid) = current_token_user_sid() else {
        return false;
    };
    unsafe { EqualSid(file_sid, PSID(token_sid.as_ptr() as _)) }.is_ok()
}

// Same-file check =====================================================================================================

pub fn files_are_same(path_a: &str, path_b: &str) -> bool {
    let (Some(a), Some(b)) = (file_info(path_a), file_info(path_b)) else {
        return false;
    };
    a.dwVolumeSerialNumber == b.dwVolumeSerialNumber
        && a.nFileIndexHigh == b.nFileIndexHigh
        && a.nFileIndexLow == b.nFileIndexLow
}

fn file_info(path: &str) -> Option<BY_HANDLE_FILE_INFORMATION> {
    let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    let handle = unsafe {
        CreateFileW(
            windows::core::PCWSTR(wide.as_ptr()),
            0, // no access needed for metadata
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_ATTRIBUTE_NORMAL,
            None,
        )
    }
    .ok()?;

    let mut info = MaybeUninit::zeroed();
    let result = unsafe { GetFileInformationByHandle(handle, info.as_mut_ptr()) };
    let _ = unsafe { CloseHandle(handle) };
    result.ok()?;
    Some(unsafe { info.assume_init() })
}

// Named pipe detection ================================================================================================

pub fn is_named_pipe(path: &str) -> bool {
    let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    let Ok(handle) = (unsafe {
        CreateFileW(
            windows::core::PCWSTR(wide.as_ptr()),
            FILE_GENERIC_READ.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
    }) else {
        return false;
    };
    let file_type = unsafe { GetFileType(handle) };
    let _ = unsafe { CloseHandle(handle) };
    file_type == FILE_TYPE_PIPE
}

// Token queries =======================================================================================================

/// Get the current user's SID as a byte vector.
fn current_token_user_sid() -> Option<Vec<u8>> {
    let token = open_process_token()?;
    let buf = get_token_info(token, TokenUser)?;
    let _ = unsafe { CloseHandle(token) };
    let user: &TOKEN_USER = unsafe { &*(buf.as_ptr() as *const TOKEN_USER) };
    Some(sid_to_bytes(user.User.Sid))
}

/// Get the RID (last sub-authority) of the current user's SID.
pub fn current_user_rid() -> Option<u32> {
    let token = open_process_token()?;
    let buf = get_token_info(token, TokenUser)?;
    let _ = unsafe { CloseHandle(token) };
    let user: &TOKEN_USER = unsafe { &*(buf.as_ptr() as *const TOKEN_USER) };
    sid_last_rid(user.User.Sid)
}

/// Check if the current process is elevated (running as administrator).
pub fn is_elevated() -> bool {
    let Some(token) = open_process_token() else {
        return false;
    };
    let Some(buf) = get_token_info(token, TokenElevation) else {
        let _ = unsafe { CloseHandle(token) };
        return false;
    };
    let _ = unsafe { CloseHandle(token) };
    let elevation: &TOKEN_ELEVATION = unsafe { &*(buf.as_ptr() as *const TOKEN_ELEVATION) };
    elevation.TokenIsElevated != 0
}

/// Get the RIDs of all groups in the current process token.
pub fn current_group_rids() -> Vec<u32> {
    let Some(token) = open_process_token() else {
        return vec![0];
    };
    let Some(buf) = get_token_info(token, TokenGroups) else {
        let _ = unsafe { CloseHandle(token) };
        return vec![0];
    };
    let _ = unsafe { CloseHandle(token) };
    let groups: &TOKEN_GROUPS = unsafe { &*(buf.as_ptr() as *const TOKEN_GROUPS) };
    let slice = unsafe { std::slice::from_raw_parts(groups.Groups.as_ptr(), groups.GroupCount as usize) };
    slice
        .iter()
        .filter_map(|g: &SID_AND_ATTRIBUTES| sid_last_rid(g.Sid))
        .collect()
}

// Helpers =============================================================================================================

fn open_process_token() -> Option<HANDLE> {
    let mut token = HANDLE::default();
    unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) }.ok()?;
    Some(token)
}

fn get_token_info(token: HANDLE, class: windows::Win32::Security::TOKEN_INFORMATION_CLASS) -> Option<Vec<u8>> {
    let mut len = 0u32;
    // First call: get required buffer size.
    let _ = unsafe { GetTokenInformation(token, class, None, 0, &mut len) };
    if len == 0 {
        return None;
    }
    let mut buf = vec![0u8; len as usize];
    unsafe { GetTokenInformation(token, class, Some(buf.as_mut_ptr() as _), len, &mut len) }.ok()?;
    Some(buf)
}

/// Extract the last sub-authority (RID) from a SID.
fn sid_last_rid(sid: PSID) -> Option<u32> {
    let count = unsafe { *GetSidSubAuthorityCount(sid) };
    if count == 0 {
        return None;
    }
    Some(unsafe { *GetSidSubAuthority(sid, (count - 1) as u32) })
}

/// Copy a SID to an owned byte vector.
fn sid_to_bytes(sid: PSID) -> Vec<u8> {
    let len = unsafe { GetLengthSid(sid) } as usize;
    let mut buf = vec![0u8; len];
    unsafe { std::ptr::copy_nonoverlapping(sid.0 as *const u8, buf.as_mut_ptr(), len) };
    buf
}
