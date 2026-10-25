//! Native provider credentials and retained, query-only process identities.
//! This grants file-provider access, never game-memory write authority.
#![allow(dead_code)]
use crate::{
    native_session::same_path,
    session_safety::{self, Identity},
};
use anyhow::{Context, Result, ensure};
use std::{
    fs,
    io::Read,
    mem::size_of,
    os::windows::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    sync::Mutex,
};
use windows::{
    Win32::{
        Foundation::{
            CloseHandle, FILETIME, HANDLE, HLOCAL, LocalFree, WAIT_OBJECT_0, WAIT_TIMEOUT,
        },
        Security::{
            Authorization::*,
            Cryptography::{BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom},
            *,
        },
        System::{Pipes::GetNamedPipeClientProcessId, Threading::*},
    },
    core::{PCWSTR, PWSTR},
};
pub(crate) struct Process {
    handle: HANDLE,
    pub identity: Identity,
}
// Retained kernel process handles support concurrent query/wait operations.
// Only Drop closes the handle; there is no duplication of ownership.
unsafe impl Send for Process {}
unsafe impl Sync for Process {}
impl Drop for Process {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.handle);
        }
    }
}
impl Process {
    pub fn open(identity: &Identity) -> Result<Self> {
        ensure!(
            session_safety::identity(identity.pid)? == *identity,
            "Provider process identity changed"
        );
        let process = Self {
            handle: unsafe {
                OpenProcess(
                    PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                    false,
                    identity.pid,
                )?
            },
            identity: identity.clone(),
        };
        let (mut created, mut exit, mut kernel, mut user) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        unsafe {
            GetProcessTimes(
                process.handle,
                &mut created,
                &mut exit,
                &mut kernel,
                &mut user,
            )?;
        }
        let birth = ((created.dwHighDateTime as u64) << 32) | created.dwLowDateTime as u64;
        let mut path = vec![0u16; 32768];
        let mut len = path.len() as u32;
        unsafe {
            QueryFullProcessImageNameW(
                process.handle,
                PROCESS_NAME_WIN32,
                PWSTR(path.as_mut_ptr()),
                &mut len,
            )?;
        }
        ensure!(
            identity.started == birth.to_string()
                && same_path(
                    &String::from_utf16(&path[..len as usize])?,
                    Path::new(&identity.path)
                )
                && process.alive()?,
            "Provider process was replaced or exited"
        );
        Ok(process)
    }
    pub fn alive(&self) -> Result<bool> {
        match unsafe { WaitForSingleObject(self.handle, 0) } {
            WAIT_OBJECT_0 => Ok(false),
            WAIT_TIMEOUT => Ok(true),
            _ => anyhow::bail!("Provider process presence is unknown"),
        }
    }
    pub fn accepts(&self, pid: u32) -> Result<bool> {
        Ok(pid == self.identity.pid && self.alive()?)
    }
}
pub(crate) fn random() -> Result<[u8; 32]> {
    let mut b = [0; 32];
    unsafe {
        BCryptGenRandom(None, &mut b, BCRYPT_USE_SYSTEM_PREFERRED_RNG).ok()?;
    }
    Ok(b)
}
pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
pub(crate) struct Secret([u8; 32]);
impl Secret {
    pub fn new() -> Result<Self> {
        Ok(Self(random()?))
    }
    pub fn text(&self) -> String {
        hex(&self.0)
    }
    pub fn matches(&self, text: &str) -> bool {
        if text.len() != 64 {
            return false;
        }
        let mut diff = 0u8;
        for (i, pair) in text.as_bytes().chunks_exact(2).enumerate() {
            let nibble = |b: u8| match b {
                b'0'..=b'9' => Some(b - b'0'),
                b'a'..=b'f' => Some(b - b'a' + 10),
                b'A'..=b'F' => Some(b - b'A' + 10),
                _ => None,
            };
            let (Some(a), Some(b)) = (nibble(pair[0]), nibble(pair[1])) else {
                return false;
            };
            diff |= self.0[i] ^ ((a << 4) | b);
        }
        diff == 0
    }
}
pub(crate) struct Gate {
    parent: Process,
    game_exe: PathBuf,
    game: Mutex<Option<(Process, Option<VerifiedImage>)>>,
    pub secret: Secret,
}
impl Gate {
    pub fn new(parent: &Identity, game_exe: &Path) -> Result<Self> {
        ensure!(
            game_exe.is_absolute()
                && game_exe
                    .file_name()
                    .is_some_and(|s| s.to_string_lossy().eq_ignore_ascii_case("NBA2K27.exe")),
            "Invalid configured game path"
        );
        ensure!(
            same_path(&parent.path, &std::env::current_exe()?),
            "Provider owner must use this exact launcher executable"
        );
        Ok(Self {
            parent: Process::open(parent)?,
            game_exe: game_exe.into(),
            game: Mutex::new(None),
            secret: Secret::new()?,
        })
    }
    pub fn parent(&self, pid: u32, token: &str) -> Result<bool> {
        Ok(self.parent.accepts(pid)? && self.secret.matches(token))
    }
    pub fn parent_alive(&self) -> Result<bool> {
        self.parent.alive()
    }
    pub fn game(&self, pid: u32) -> Result<bool> {
        let game = self
            .game
            .lock()
            .map_err(|_| anyhow::anyhow!("Provider game ownership unavailable"))?;
        if let Some((game, _)) = &*game {
            game.accepts(pid)
        } else {
            Ok(false)
        }
    }
    pub fn game_state(&self) -> Result<Option<bool>> {
        self.game
            .lock()
            .map_err(|_| anyhow::anyhow!("Provider game ownership unavailable"))?
            .as_ref()
            .map(|(p, _)| p.alive())
            .transpose()
    }
    pub fn bind(&self, identity: &Identity, check: impl Fn() -> Result<()>) -> Result<()> {
        check()?;
        ensure!(
            same_path(&identity.path, &self.game_exe),
            "Game does not match this launch session"
        );
        let processes = session_safety::processes()?;
        ensure!(
            !processes
                .iter()
                .any(|p| session_safety::protected_process(&p.name)),
            "Protected process present; provider game binding refused"
        );
        let games: Vec<_> = processes
            .iter()
            .filter(|p| p.name.eq_ignore_ascii_case("NBA2K27.exe"))
            .collect();
        ensure!(
            games.len() == 1 && games[0].identity.as_ref() == Some(identity),
            "Game identity is ambiguous"
        );
        let process = Process::open(identity)?;
        let image = verify_build(&self.game_exe)?;
        let mut slot = self
            .game
            .lock()
            .map_err(|_| anyhow::anyhow!("Provider game ownership unavailable"))?;
        ensure!(
            slot.is_none(),
            "Provider game identity is immutable for this session"
        );
        check()?;
        ensure!(process.alive()?, "Game exited before provider binding");
        *slot = Some((process, Some(image)));
        Ok(())
    }
    #[cfg(test)]
    pub fn bind_fixture(&self, identity: &Identity) -> Result<()> {
        let process = Process::open(identity)?;
        let mut slot = self.game.lock().unwrap();
        ensure!(slot.is_none(), "Fixture already bound");
        *slot = Some((process, None));
        Ok(())
    }
}
pub(crate) struct VerifiedImage {
    _file: fs::File,
    pub build: serde_json::Value,
}
pub(crate) fn verify_build(path: &Path) -> Result<VerifiedImage> {
    let mut file = fs::OpenOptions::new().read(true).share_mode(1).open(path)?;
    let mut bytes = [0u8; 4096];
    file.read_exact(&mut bytes)?;
    ensure!(&bytes[..2] == b"MZ", "Unsupported game executable");
    let pe = u32::from_le_bytes(bytes[0x3C..0x40].try_into()?) as usize;
    ensure!(
        pe >= 0x40 && pe.checked_add(24 + 60).is_some_and(|n| n <= bytes.len()),
        "Invalid game PE header"
    );
    ensure!(
        &bytes[pe..pe + 4] == b"PE\0\0"
            && u16::from_le_bytes(bytes[pe + 4..pe + 6].try_into()?) == 0x8664
            && u16::from_le_bytes(bytes[pe + 24..pe + 26].try_into()?) == 0x20B,
        "Unsupported game architecture"
    );
    let stamp = u32::from_le_bytes(bytes[pe + 8..pe + 12].try_into()?);
    let size = u32::from_le_bytes(bytes[pe + 24 + 56..pe + 24 + 60].try_into()?);
    let (mut outfits_mapped, mut outfit_error) = (false, None::<String>);
    let (label, writes) = match (stamp, size) {
        (0x6AA2013D, 0x36558000) => ("September build", true),
        (0x6A974D13, 0x368C3000) => ("Earlier provider build", false),
        (0x6AC53765, 0x35BDE000) => {
            crate::provider_build_profile::verify_v2(&mut file, &bytes, pe)?;
            // Independent manual-outfit admission. A failure never fails the
            // build; only Version 2 outfit editing/carry-over stays off.
            match crate::provider_build_profile::verify_outfit_carry_v2_launch(
                &mut file, &bytes, pe,
            ) {
                Ok(()) => outfits_mapped = true,
                Err(e) => {
                    outfit_error = Some(format!(
                        "Version 2 outfit editing is off: the outfit carry-over proof did not pass for this game image ({e:#}). Mod loading is available separately."
                    ))
                }
            }
            ("Version 2", false)
        }
        (0x6AD2E48C, 0x36532000) => {
            // October 21 update: retain the structural provider proof, then
            // expose only the independently remapped Version 2 feature lanes.
            // The broad native write gate remains closed because the complete
            // roster/team serializer layout is not yet proven for this image.
            let _provider = crate::feature_relocation::admit_mod_loading(path)
                .map_err(|e| anyhow::anyhow!("Unsupported game build: {e:#}"))?;
            match crate::provider_build_profile::verify_outfit_carry_v2_launch(
                &mut file, &bytes, pe,
            ) {
                Ok(()) => outfits_mapped = true,
                Err(e) => {
                    outfit_error = Some(format!(
                        "Version 2 outfit editing is off: the outfit carry-over proof did not pass for this game image ({e:#}). Mod loading is available separately."
                    ))
                }
            }
            return Ok(VerifiedImage {
                _file: file,
                build: serde_json::json!({
                    "label": "Version 2",
                    "modProviderMapped": true,
                    "nativeWritesMapped": false,
                    "version2ArenaBallMapped": true,
                    "version2GameplayBallMapped": true,
                    "version2ScoreboardMapped": true,
                    "observerMapped": true,
                    "version2OutfitCarryMapped": outfits_mapped,
                    "version2OutfitCarryError": outfit_error,
                    "resolvedUpdate": {"timestamp": "6AD2E48C", "imageSize": "36532000"},
                }),
            });
        }
        _ => {
            // Unrecognized game update: admit mod loading only, and only when
            // WPF's structural provider resolver and every provider ABI anchor
            // re-derive (feature_relocation.rs). Every native feature keeps
            // its own exact-build guard and stays off.
            let build = crate::feature_relocation::admit_mod_loading(path)
                .map_err(|e| anyhow::anyhow!("Unsupported game build: {e:#}"))?;
            return Ok(VerifiedImage { _file: file, build });
        }
    };
    Ok(VerifiedImage {
        _file: file,
        build: serde_json::json!({
            "label": label, "modProviderMapped": true, "nativeWritesMapped": writes,
            "version2ArenaBallMapped": label == "Version 2",
            "version2GameplayBallMapped": label == "Version 2",
            "version2ScoreboardMapped": label == "Version 2",
            "observerMapped": writes || label == "Version 2",
            "version2OutfitCarryMapped": outfits_mapped,
            "version2OutfitCarryError": outfit_error,
        }),
    })
}
pub(crate) fn client_pid(handle: HANDLE) -> Result<u32> {
    let mut pid = 0;
    unsafe {
        GetNamedPipeClientProcessId(handle, &mut pid)?;
    }
    ensure!(pid > 0, "Unknown provider client");
    Ok(pid)
}
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
    #[cfg(test)]
    pub fn audit(&self) -> Result<()> {
        let (mut present, mut defaulted) = (
            windows::core::BOOL::default(),
            windows::core::BOOL::default(),
        );
        let mut acl = std::ptr::null_mut();
        unsafe {
            GetSecurityDescriptorDacl(self.descriptor, &mut present, &mut acl, &mut defaulted)?;
        }
        ensure!(
            present.as_bool() && !defaulted.as_bool() && !acl.is_null(),
            "Missing private provider DACL"
        );
        ensure!(
            unsafe { (*acl).AceCount } == 1,
            "Unexpected provider ACL entries"
        );
        let mut ace = std::ptr::null_mut();
        unsafe {
            GetAce(acl, 0, &mut ace)?;
        }
        let ace = unsafe { &*(ace as *const ACCESS_ALLOWED_ACE) };
        ensure!(
            ace.Header.AceType == 0 && ace.Mask == 0x10000000,
            "Unexpected provider permissions"
        );
        let mut principal = PWSTR::null();
        unsafe {
            ConvertSidToStringSidW(PSID(&ace.SidStart as *const u32 as _), &mut principal)?;
        }
        let text = unsafe { principal.to_string() };
        unsafe {
            let _ = LocalFree(Some(HLOCAL(principal.0 as _)));
        }
        ensure!(
            text? == self.sid,
            "Provider ACL belongs to another principal"
        );
        let mut control = 0;
        let mut revision = 0;
        unsafe {
            GetSecurityDescriptorControl(self.descriptor, &mut control, &mut revision)?;
        }
        ensure!(
            control & SE_DACL_PROTECTED.0 != 0,
            "Inherited provider permissions are allowed"
        );
        Ok(())
    }
}
