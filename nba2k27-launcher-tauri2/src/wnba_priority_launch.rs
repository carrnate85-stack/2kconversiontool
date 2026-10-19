//! Saved-toggle setup for the exact owned dev Play; no game-memory capability.
use crate::{file_identity::Stable, loader_files::Directories, session_safety::Identity};
use anyhow::{Context, Result, ensure};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::windows::{ffi::OsStrExt, fs::OpenOptionsExt},
    path::{Path, PathBuf},
};
use windows::{
    Win32::Storage::FileSystem::{
        FILE_FLAG_OPEN_REPARSE_POINT, FILE_FLAG_WRITE_THROUGH, MOVE_FILE_FLAGS, MoveFileExW,
    },
    core::PCWSTR,
};

fn terminal_source_failure(
    bytes: &[u8],
    session: &str,
    identity: &Identity,
) -> Result<Option<String>> {
    ensure!(
        bytes.len() <= 4096
            && session.len() == 48
            && session.bytes().all(|b| b.is_ascii_hexdigit()),
        "Invalid WNBA source receipt scope"
    );
    let text = std::str::from_utf8(bytes)?;
    let lines: Vec<_> = text.lines().collect();
    ensure!(
        lines.len() == 8
            && lines[0] == "SCHEMA=wnba-native-source-admission-test-v1"
            && lines[2] == format!("PID={}", identity.pid)
            && lines[3] == format!("SESSION={session}")
            && lines[4].starts_with("REASON=")
            && lines[5..]
                == [
                    "CATEGORY_WRITES=0",
                    "GAME_CODE_HOOKS=0",
                    "WNBA_GAMEPLAY_VERIFIED=0"
                ],
        "WNBA source receipt does not match this owned game session"
    );
    match lines[1] {
        "STATE=REGISTERED" => Ok(None),
        "STATE=REFUSED" | "STATE=FAILED" => {
            let reason = lines[4].strip_prefix("REASON=").unwrap();
            ensure!(
                !reason.is_empty() && reason.chars().all(|c| !c.is_control()),
                "Invalid WNBA source failure reason"
            );
            Ok(Some(format!(
                "WNBA roster source did not initialize: {}",
                reason.chars().take(300).collect::<String>()
            )))
        }
        _ => anyhow::bail!("Unrecognized terminal WNBA source receipt"),
    }
}

/// A missing terminal receipt is normal during native registration. An exact
/// REFUSED/FAILED receipt is terminal for this game and must be shown instead
/// of waiting forever. This function never opens a game process or memory.
#[derive(Debug, PartialEq)]
pub(crate) enum SourceState {
    Pending,
    Registered,
    Refused(String),
}
fn parsed_blacktop_progress(bytes: &[u8], session: &str) -> Result<Option<(String, String)>> {
    ensure!(
        session.len() == 48
            && session.bytes().all(|b| b.is_ascii_hexdigit())
            && bytes.len() <= 65536,
        "Invalid Blacktop progress scope"
    );
    let text = std::str::from_utf8(bytes)?;
    let prefix = format!(" SESSION={session} STATE=");
    let mut latest = None;
    let mut limit_reached = false;
    for line in text.lines().rev() {
        let Some((tick, rest)) = line.split_once(&prefix) else {
            continue;
        };
        ensure!(
            tick.strip_prefix("TICK_MS=")
                .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())),
            "Invalid Blacktop progress tick"
        );
        let (state, detail) = rest
            .split_once(" DETAIL=")
            .context("Invalid Blacktop progress detail")?;
        ensure!(
            detail.len() <= 512 && detail.chars().all(|c| !c.is_control()),
            "Invalid Blacktop progress message"
        );
        if state == "LABEL_RESOLVED" {
            continue;
        }
        let phase = match state {
            "PUBLISHED" | "ACTIVE" => "active",
            "RESTORED" => "waiting",
            "HELD" | "FAILED" | "REFUSED" => "held",
            "LIMIT_REACHED" => "reload",
            "WAITING"
            | "WAITING_FOR_APPROVAL"
            | "WAITING_ROSTER"
            | "ROSTER_ARMED"
            | "ROSTER_RESTORED" => "waiting",
            _ => anyhow::bail!("Unrecognized Blacktop progress state"),
        };
        let message = match state {
            "PUBLISHED" | "ACTIVE" => "WNBA Blacktop category is active. Use Y to cycle teams.",
            "RESTORED" => "WNBA category is paused outside Blacktop player selection.",
            "LIMIT_REACHED" => {
                "WNBA Blacktop reached the change limit for this game session. Close the game and start a new Play session."
            }
            "WAITING_FOR_APPROVAL" => "Preparing WNBA Blacktop for this Play session.",
            "WAITING_ROSTER" => {
                "WNBA roster has not finished loading. Return Home and re-enter Blacktop; the session log records the cause."
            }
            "ROSTER_ARMED" => "Waiting for Blacktop to load the WNBA roster.",
            "ROSTER_RESTORED" => "Waiting for the loaded WNBA teams to be verified.",
            "WAITING"
                if detail.to_ascii_lowercase().contains("stock roster")
                    || detail.to_ascii_lowercase().contains("warmup") =>
            {
                "WNBA roster has not finished loading. Return Home and re-enter Blacktop; the session log records the cause."
            }
            "WAITING" => "Waiting for Blacktop player selection.",
            _ => {
                "WNBA Blacktop is held. Close the game and start a new Play session. The session log records the reason."
            }
        };
        // Holds are terminal. A once-only limit notice survives later ordinary
        // observations, which must not turn restart-needed back into waiting.
        if phase == "held" {
            return Ok(Some((phase.into(), message.into())));
        }
        if state == "LIMIT_REACHED" {
            limit_reached = true;
        }
        if latest.is_none() || (limit_reached && phase == "reload") {
            latest = Some((phase.into(), message.into()));
        }
    }
    Ok(latest)
}
/// Informational exact-session progress; never grants a write capability.
pub(crate) fn blacktop_state(
    root: &Path,
    session: &str,
    identity: &Identity,
) -> Result<Option<(String, String)>> {
    use std::os::windows::{fs::MetadataExt, io::AsRawHandle};
    use windows::Win32::{
        Foundation::HANDLE,
        Storage::FileSystem::{BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle},
    };
    match source_state(root, session, identity)? {
        SourceState::Refused(_) => return Ok(Some(("held".into(), "WNBA roster did not initialize. Close the game and start a new Play session. The session log records the reason.".into()))),
        SourceState::Pending => {
            return Ok(Some((
                "waiting".into(),
                "Preparing the WNBA roster source; waiting for Home or Quick Play.".into(),
            )));
        }
        SourceState::Registered => {}
    }
    let directory = root.join("Logs/game-session");
    let path = directory.join(format!(
        "wnba-blacktop-categories-{}.progress",
        identity.pid
    ));
    let _dirs = Directories::pin(&directory)?;
    let mut file = match OpenOptions::new()
        .read(true)
        .share_mode(3)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(path)
    {
        Err(error) if matches!(error.raw_os_error(), Some(2 | 3 | 32 | 33)) => return Ok(None),
        result => result?,
    };
    let before = file.metadata()?;
    let id = Stable::read(&file)?;
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    unsafe {
        GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut info)?;
    }
    ensure!(
        before.is_file()
            && before.file_attributes() & 0x400 == 0
            && before.len() <= 65536
            && info.nNumberOfLinks == 1,
        "Blacktop progress is not a bounded ordinary file"
    );
    let mut bytes = Vec::new();
    (&mut file).take(65537).read_to_end(&mut bytes)?;
    let after = file.metadata()?;
    if Stable::read(&file)? != id
        || after.len() != before.len()
        || after.last_write_time() != before.last_write_time()
        || bytes.len() as u64 != before.len()
    {
        return Ok(None);
    }
    parsed_blacktop_progress(&bytes, session)
}
pub(crate) fn source_state(root: &Path, session: &str, identity: &Identity) -> Result<SourceState> {
    use std::os::windows::{fs::MetadataExt, io::AsRawHandle};
    use windows::Win32::{
        Foundation::HANDLE,
        Storage::FileSystem::{BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle},
    };
    let directory = root.join("Logs/game-session");
    let path = directory.join(format!("wnba-source-test-{}.log", identity.pid));
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(SourceState::Pending);
        }
        Err(error) => return Err(error.into()),
        Ok(_) => {}
    }
    let _dirs = Directories::pin(&directory)?;
    let mut file = match OpenOptions::new()
        .read(true)
        .share_mode(1)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(&path)
    {
        // Native CREATE_NEW report is not complete until its writer closes.
        Err(error) if matches!(error.raw_os_error(), Some(32 | 33)) => {
            return Ok(SourceState::Pending);
        }
        result => result?,
    };
    let id = Stable::read(&file)?;
    let metadata = file.metadata()?;
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    unsafe {
        GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut info)?;
    }
    ensure!(
        metadata.is_file()
            && metadata.file_attributes() & 0x400 == 0
            && metadata.len() <= 4096
            && info.nNumberOfLinks == 1,
        "WNBA source receipt is not a bounded ordinary file"
    );
    let mut bytes = Vec::new();
    (&mut file).take(4097).read_to_end(&mut bytes)?;
    let after = file.metadata()?;
    ensure!(
        Stable::read(&file)? == id
            && after.len() == metadata.len()
            && after.last_write_time() == metadata.last_write_time()
            && bytes.len() as u64 == metadata.len(),
        "WNBA source receipt changed during read"
    );
    Ok(match terminal_source_failure(&bytes, session, identity)? {
        Some(reason) => SourceState::Refused(reason),
        None => SourceState::Registered,
    })
}

pub(crate) struct Prepared {
    root: PathBuf,
    _pack: crate::wnba_priority_categories::PreparedPack,
    approvals: Vec<File>,
    freestyle: bool,
    blacktop: bool,
}
fn encoded(magic: &[u8; 8], session: &str, identity: &Identity) -> Result<Vec<u8>> {
    ensure!(
        session.len() == 48 && session.bytes().all(|b| b.is_ascii_hexdigit()),
        "Invalid WNBA launch session"
    );
    let birth: u64 = identity
        .started
        .parse()
        .context("Invalid owned WNBA game birth")?;
    ensure!(
        birth > 0 && identity.pid > 0,
        "Missing owned WNBA game identity"
    );
    let mut bytes = magic.to_vec();
    bytes.extend(identity.pid.to_le_bytes());
    bytes.extend([0; 4]);
    bytes.extend(birth.to_le_bytes());
    bytes.extend(session.bytes());
    Ok(bytes)
}
fn publish_token(
    root: &Path,
    leaf: &str,
    bytes: &[u8],
    check: &impl Fn() -> Result<()>,
) -> Result<File> {
    check()?;
    let _dirs = Directories::pin(root)?;
    ensure!(
        bytes.len() == 72 && !leaf.contains(['/', '\\']) && leaf.ends_with(".bin"),
        "Invalid WNBA launch token target"
    );
    let target = root.join(leaf);
    ensure!(
        !target.try_exists()?,
        "WNBA launch token already exists; it will not be adopted or replaced"
    );
    let temporary = root.join(format!(".{}.{}.tmp", leaf, crate::backend::unique_id()));
    let mut output = OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(1)
        .create_new(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0 | FILE_FLAG_WRITE_THROUGH.0)
        .open(&temporary)?;
    output.write_all(bytes)?;
    output.sync_all()?;
    let id = Stable::read(&output)?;
    drop(output);
    check()?;
    let from: Vec<_> = temporary.as_os_str().encode_wide().chain(Some(0)).collect();
    let to: Vec<_> = target.as_os_str().encode_wide().chain(Some(0)).collect();
    // Atomic, no replacement. The native watcher can only see a complete token.
    unsafe {
        MoveFileExW(
            PCWSTR(from.as_ptr()),
            PCWSTR(to.as_ptr()),
            MOVE_FILE_FLAGS(0),
        )?;
    }
    let mut pinned = OpenOptions::new()
        .read(true)
        .share_mode(1)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(&target)?;
    ensure!(
        Stable::read(&pinned)? == id && pinned.metadata()?.len() == 72,
        "WNBA launch token file identity changed"
    );
    let mut observed = Vec::new();
    Read::by_ref(&mut pinned)
        .take(73)
        .read_to_end(&mut observed)?;
    ensure!(observed == bytes, "WNBA launch token readback differs");
    check()?;
    Ok(pinned)
}
impl Prepared {
    pub(crate) fn open(root: &Path, freestyle: bool, blacktop: bool) -> Result<Self> {
        ensure!(
            cfg!(debug_assertions),
            "WNBA toggle setup is currently dev-only"
        );
        ensure!(
            freestyle || blacktop,
            "No WNBA access requested for this launch"
        );
        Ok(Self {
            root: root.into(),
            _pack: crate::wnba_priority_categories::prepare_pack(
                root,
                crate::wnba_priority_categories::PackProfile::for_blacktop(blacktop),
            )?,
            approvals: Vec::new(),
            freestyle,
            blacktop,
        })
    }
    pub(crate) fn bind(
        &mut self,
        session: &str,
        identity: &Identity,
        check: impl Fn() -> Result<()>,
    ) -> Result<()> {
        ensure!(
            self.approvals.is_empty(),
            "WNBA launch authorization is single-use"
        );
        let directory = self.root.join("Data/wnba-priority-v2");
        // Freeze both permissions before the source worker can consume approval.
        for (leaf, magic) in token_plan(session, self.freestyle, self.blacktop)? {
            self.approvals.push(publish_token(
                &directory,
                &leaf,
                &encoded(magic, session, identity)?,
                &check,
            )?);
        }
        Ok(())
    }
}
fn token_plan(
    session: &str,
    freestyle: bool,
    blacktop: bool,
) -> Result<Vec<(String, &'static [u8; 8])>> {
    ensure!(freestyle || blacktop, "No WNBA launch permission requested");
    ensure!(
        session.len() == 48 && session.bytes().all(|b| b.is_ascii_hexdigit()),
        "Invalid WNBA launch session"
    );
    let mut result = Vec::new();
    if freestyle {
        result.push((format!("categories-{session}.bin"), b"C27WNPC1" as &[u8; 8]));
    }
    if blacktop {
        result.push((
            format!("blacktop-categories-{session}.bin"),
            b"C27WNBC2" as &[u8; 8],
        ));
    }
    result.push((
        format!("approval-{session}.bin"),
        if blacktop { b"C27WNBT1" } else { b"C27WNPA1" },
    ));
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    const SESSION: &str = "0123456789abcdef0123456789abcdef0123456789abcdef";
    fn identity() -> Identity {
        Identity {
            pid: 123,
            started: "456".into(),
            path: "C:\\game\\NBA2K27.exe".into(),
        }
    }
    #[test]
    fn terminal_source_failure_is_exact_scoped_and_pending_is_retryable() {
        let receipt = |state: &str| {
            format!(
                "SCHEMA=wnba-native-source-admission-test-v1\nSTATE={state}\nPID=123\nSESSION={SESSION}\nREASON=No stable inactive Home/Quick Play boundary within 120 seconds\nCATEGORY_WRITES=0\nGAME_CODE_HOOKS=0\nWNBA_GAMEPLAY_VERIFIED=0\n"
            )
        };
        assert!(
            terminal_source_failure(receipt("REGISTERED").as_bytes(), SESSION, &identity())
                .unwrap()
                .is_none()
        );
        for state in ["REFUSED", "FAILED"] {
            assert!(
                terminal_source_failure(receipt(state).as_bytes(), SESSION, &identity())
                    .unwrap()
                    .unwrap()
                    .contains("Home/Quick Play")
            );
        }
        assert!(
            terminal_source_failure(receipt("REFUSED").as_bytes(), &"a".repeat(48), &identity())
                .is_err()
        );
        let mut wrong = identity();
        wrong.pid += 1;
        assert!(terminal_source_failure(receipt("REFUSED").as_bytes(), SESSION, &wrong).is_err());
        assert!(
            terminal_source_failure(receipt("PENDING").as_bytes(), SESSION, &identity()).is_err()
        );
        let root = std::env::temp_dir().join(format!(
            "wnba-source-pending-{}",
            crate::backend::unique_id()
        ));
        assert!(source_state(&root, SESSION, &identity()).unwrap() == SourceState::Pending);
    }
    #[test]
    fn blacktop_progress_is_exact_scoped_and_keeps_held_and_restore_truthful() {
        let line = |state: &str, detail: &str| {
            format!("TICK_MS=123 SESSION={SESSION} STATE={state} DETAIL={detail}\n")
        };
        assert_eq!(
            parsed_blacktop_progress(
                line("PUBLISHED", "Owned category active").as_bytes(),
                SESSION
            )
            .unwrap(),
            Some((
                "active".into(),
                "WNBA Blacktop category is active. Use Y to cycle teams.".into()
            ))
        );
        assert_eq!(
            parsed_blacktop_progress(line("RESTORED", "Stock route restored").as_bytes(), SESSION)
                .unwrap(),
            Some((
                "waiting".into(),
                "WNBA category is paused outside Blacktop player selection.".into()
            ))
        );
        let text = format!(
            "{}{}",
            line("PUBLISHED", "Old active"),
            line("HELD", "Foreign label; close game")
        );
        assert_eq!(
            parsed_blacktop_progress(text.as_bytes(), SESSION).unwrap(),
            Some(("held".into(), "WNBA Blacktop is held. Close the game and start a new Play session. The session log records the reason.".into()))
        );
        assert_eq!(
            parsed_blacktop_progress(text.as_bytes(), &"a".repeat(48)).unwrap(),
            None
        );
        assert!(parsed_blacktop_progress(line("BOGUS", "Unknown").as_bytes(), SESSION).is_err());
        assert!(
            parsed_blacktop_progress(line("HELD", "Injected\tcontrol").as_bytes(), SESSION)
                .is_err()
        );
        assert!(parsed_blacktop_progress(b"", "bad").is_err());
        assert!(parsed_blacktop_progress(line("WAITING_ROSTER","Blacktop still has the 370-team stock roster; the WNBA source has not been consumed").as_bytes(),SESSION).unwrap().unwrap().1.contains("has not finished loading"));
        let diagnostic = format!(
            "{}{}",
            line("WAITING", "stable selection needed"),
            line("LABEL_RESOLVED", "resolvedRVA=123")
        );
        assert_eq!(
            parsed_blacktop_progress(diagnostic.as_bytes(), SESSION)
                .unwrap()
                .unwrap()
                .0,
            "waiting"
        );
        assert!(
            parsed_blacktop_progress(
                line("WAITING", "Loaded stock roster: 370 teams; warmup needed").as_bytes(),
                SESSION
            )
            .unwrap()
            .unwrap()
            .1
            .contains("re-enter Blacktop")
        );
        let limited = line(
            "LIMIT_REACHED",
            "Session change limit reached; routes unowned",
        );
        for later in [
            "WAITING",
            "RESTORED",
            "ROSTER_ARMED",
            "ROSTER_RESTORED",
            "LABEL_RESOLVED",
        ] {
            let text = format!(
                "{}{}",
                limited,
                line(later, "Later informational observation")
            );
            let (phase, message) = parsed_blacktop_progress(text.as_bytes(), SESSION)
                .unwrap()
                .unwrap();
            assert_eq!(phase, "reload");
            assert!(message.contains("change limit") && message.contains("new Play session"));
        }
        for held in ["HELD", "FAILED", "REFUSED"] {
            let text = format!(
                "{}{}{}",
                limited,
                line(held, "Ownership failed"),
                line("WAITING", "Stale ordinary observation")
            );
            assert_eq!(
                parsed_blacktop_progress(text.as_bytes(), SESSION)
                    .unwrap()
                    .unwrap()
                    .0,
                "held"
            );
        }
        assert_eq!(
            parsed_blacktop_progress(limited.as_bytes(), &"a".repeat(48)).unwrap(),
            None
        );
        for state in ["ROSTER_ARMED", "ROSTER_RESTORED"] {
            let (phase, message) = parsed_blacktop_progress(
                line(state, "Native callback changed").as_bytes(),
                SESSION,
            )
            .unwrap()
            .unwrap();
            assert_eq!(phase, "waiting");
            assert!(!message.contains("active") && !message.contains("Freestyle"));
        }
    }
    #[test]
    fn source_and_categories_are_separate_exact_game_tokens() {
        let source = encoded(b"C27WNPA1", SESSION, &identity()).unwrap();
        let category = encoded(b"C27WNPC1", SESSION, &identity()).unwrap();
        assert_eq!(source.len(), 72);
        assert_ne!(source, category);
        assert_eq!(&source[8..12], 123u32.to_le_bytes());
        assert_eq!(&source[16..24], 456u64.to_le_bytes());
        assert_eq!(&source[24..], SESSION.as_bytes());
        assert!(encoded(b"C27WNPA1", "bad", &identity()).is_err());
        let mut bad = identity();
        bad.started = "0".into();
        assert!(encoded(b"C27WNPA1", SESSION, &bad).is_err());
    }
    #[test]
    fn toggles_publish_only_requested_categories_before_the_exact_source_profile() {
        assert!(token_plan(SESSION, false, false).is_err());
        for (freestyle, blacktop, magics) in [
            (true, false, vec![*b"C27WNPC1", *b"C27WNPA1"]),
            (false, true, vec![*b"C27WNBC2", *b"C27WNBT1"]),
            (true, true, vec![*b"C27WNPC1", *b"C27WNBC2", *b"C27WNBT1"]),
        ] {
            let plan = token_plan(SESSION, freestyle, blacktop).unwrap();
            assert_eq!(
                plan.iter().map(|(_, magic)| **magic).collect::<Vec<_>>(),
                magics
            );
            assert_eq!(plan.last().unwrap().0, format!("approval-{SESSION}.bin"));
            assert_eq!(
                crate::wnba_priority_categories::PackProfile::for_blacktop(blacktop),
                if blacktop {
                    crate::wnba_priority_categories::PackProfile::Blacktop
                } else {
                    crate::wnba_priority_categories::PackProfile::Freestyle
                }
            );
            let source = encoded(plan.last().unwrap().1, SESSION, &identity()).unwrap();
            for (_, magic) in &plan[..plan.len() - 1] {
                let category = encoded(magic, SESSION, &identity()).unwrap();
                assert_ne!(category, source);
                assert_eq!(&category[8..], &source[8..]);
            }
        }
    }
    #[test]
    fn publication_never_overwrites_an_existing_token_and_failed_owner_publishes_nothing() {
        let root =
            std::env::temp_dir().join(format!("wnba-toggle-{}", crate::backend::unique_id()));
        fs::create_dir(&root).unwrap();
        let bytes = encoded(b"C27WNPA1", SESSION, &identity()).unwrap();
        assert!(
            publish_token(&root, "failed.bin", &bytes, &|| anyhow::bail!(
                "owner closed"
            ))
            .is_err()
        );
        assert!(!root.join("failed.bin").exists());
        let held = publish_token(&root, "owned.bin", &bytes, &|| Ok(())).unwrap();
        assert_eq!(fs::read(root.join("owned.bin")).unwrap(), bytes);
        assert!(publish_token(&root, "owned.bin", &bytes, &|| Ok(())).is_err());
        assert!(fs::write(root.join("owned.bin"), b"replacement").is_err());
        drop(held);
    }
}
