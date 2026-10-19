//! Actual current-user-only ACL implementation, copied for isolated linkage.
use anyhow::{Context, Result, ensure};
use std::mem::size_of;
use windows::{
    Win32::{
        Foundation::*,
        Security::{Authorization::*, *},
        System::Threading::*,
    },
    core::{PCWSTR, PWSTR},
};
pub(crate) struct Acl {
    descriptor: PSECURITY_DESCRIPTOR,
    pub sid: String,
}
impl Drop for Acl {
    fn drop(&mut self) {
        unsafe {
            let _ = LocalFree(Some(HLOCAL(self.descriptor.0)));
        }
    }
}
impl Acl {
    pub fn current_user() -> Result<Self> {
        struct Token(HANDLE);
        impl Drop for Token {
            fn drop(&mut self) {
                unsafe {
                    let _ = CloseHandle(self.0);
                }
            }
        }
        let mut token = Token(HANDLE::default());
        unsafe {
            OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token.0)?;
        }
        let mut size = 0;
        let _ = unsafe { GetTokenInformation(token.0, TokenUser, None, 0, &mut size) };
        ensure!(
            (size_of::<TOKEN_USER>() as u32..=65536).contains(&size),
            "Unexpected provider owner SID size"
        );
        let mut buffer = vec![0usize; (size as usize).div_ceil(size_of::<usize>())];
        unsafe {
            GetTokenInformation(
                token.0,
                TokenUser,
                Some(buffer.as_mut_ptr() as _),
                size,
                &mut size,
            )?;
        }
        let user = unsafe { &*(buffer.as_ptr() as *const TOKEN_USER) };
        let mut sid = PWSTR::null();
        unsafe {
            ConvertSidToStringSidW(user.User.Sid, &mut sid)?;
        }
        let text = unsafe { sid.to_string() };
        unsafe {
            let _ = LocalFree(Some(HLOCAL(sid.0 as _)));
        }
        let text = text.context("Provider owner SID is invalid")?;
        ensure!(
            text.starts_with("S-1-")
                && text
                    .bytes()
                    .all(|b| b == b'-' || b == b'S' || b.is_ascii_digit()),
            "Invalid owner SID"
        );
        let descriptor = format!("D:P(A;;GA;;;{text})")
            .encode_utf16()
            .chain(Some(0))
            .collect::<Vec<_>>();
        let mut result = Self {
            descriptor: PSECURITY_DESCRIPTOR::default(),
            sid: text,
        };
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                PCWSTR(descriptor.as_ptr()),
                SDDL_REVISION_1,
                &mut result.descriptor,
                None,
            )?;
        }
        Ok(result)
    }
    pub fn attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.descriptor.0,
            bInheritHandle: false.into(),
        }
    }
}
