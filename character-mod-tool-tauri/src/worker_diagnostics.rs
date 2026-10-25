use serde::Serialize;
use std::{
    io,
    path::Path,
    process::ExitStatus,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

pub fn push_tail(buffer: &mut String, text: &str, limit: usize) {
    buffer.push_str(text);
    if buffer.len() > limit {
        let mut start = buffer.len() - limit;
        while !buffer.is_char_boundary(start) {
            start += 1;
        }
        buffer.drain(..start);
    }
}

fn tail(text: &str, limit: usize) -> String {
    let mut output = String::new();
    push_tail(&mut output, text, limit);
    output
}

#[derive(Serialize)]
pub struct Failure {
    command: String,
    reason: String,
    exit_code: Option<i32>,
    exit_code_hex: Option<String>,
    wait_error: Option<String>,
    terminated_after_protocol_failure: bool,
    stderr: String,
    unrecognized_stdout: String,
    timestamp: u64,
}

impl Failure {
    pub fn new(
        command: &str,
        reason: &str,
        status: &io::Result<ExitStatus>,
        terminated_after_protocol_failure: bool,
        stderr: &str,
        stdout: &str,
    ) -> Self {
        let exit_code = status.as_ref().ok().and_then(ExitStatus::code);
        Self {
            command: tail(command, 128),
            reason: tail(reason, 2048),
            exit_code,
            exit_code_hex: exit_code.map(|code| format!("0x{:08X}", code as u32)),
            wait_error: status
                .as_ref()
                .err()
                .map(|error| tail(&error.to_string(), 512)),
            terminated_after_protocol_failure,
            stderr: tail(stderr, 4096),
            unrecognized_stdout: tail(stdout, 2048),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|time| time.as_secs())
                .unwrap_or_default(),
        }
    }

    pub fn message(&self) -> String {
        let status = match (self.exit_code, &self.exit_code_hex) {
            (Some(code), Some(hex)) => format!("exit code {code} / {hex}"),
            _ => format!(
                "exit status unavailable: {}",
                self.wait_error.as_deref().unwrap_or("unknown")
            ),
        };
        let terminated = if self.terminated_after_protocol_failure {
            "; stopped after protocol failure"
        } else {
            ""
        };
        let stderr = if self.stderr.trim().is_empty() {
            String::new()
        } else {
            format!("\n{}", tail(self.stderr.trim(), 1024))
        };
        format!(
            "Conversion worker '{}' failed ({status}{terminated}): {}{stderr}",
            self.command, self.reason
        )
    }

    pub fn write_latest(&self, data: &Path) {
        // One bounded local file, not one file per worker or failed request.
        static WRITER: Mutex<()> = Mutex::new(());
        let Ok(_guard) = WRITER.lock() else { return };
        let logs = data.join("Logs");
        if std::fs::create_dir_all(&logs).is_ok() {
            if let Ok(bytes) = serde_json::to_vec(self) {
                let _ = std::fs::write(logs.join("worker-failure.json"), bytes);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::windows::process::ExitStatusExt;

    #[test]
    fn windows_crash_status_keeps_unsigned_hex() {
        let failure = Failure::new(
            "native-hair-catalog",
            "The conversion engine ended without a result",
            &Ok(ExitStatus::from_raw(0xC0000005)),
            false,
            "",
            "",
        );
        assert_eq!(failure.exit_code, Some(-1073741819));
        assert_eq!(failure.exit_code_hex.as_deref(), Some("0xC0000005"));
        assert!(failure.message().contains("native-hair-catalog"));
        assert!(!failure.message().contains("stopped after"));
    }

    #[test]
    fn missing_result_is_an_error_even_after_exit_zero() {
        let failure = Failure::new(
            "inspect",
            "Missing result",
            &Ok(ExitStatus::from_raw(0)),
            false,
            "",
            "",
        );
        assert!(failure.message().contains("exit code 0 / 0x00000000"));
        assert!(failure.message().contains("Missing result"));
        let failure = Failure::new(
            "inspect",
            "Malformed protocol",
            &Err(io::Error::other("wait failed")),
            true,
            "",
            "",
        );
        assert!(failure.message().contains("wait failed"));
        assert!(failure.message().contains("stopped after protocol failure"));
    }

    #[test]
    fn tails_are_utf8_safe_and_bounded() {
        let mut text = String::new();
        push_tail(&mut text, &"\u{1f600}".repeat(5000), 4096);
        push_tail(&mut text, "\nlast line", 4096);
        assert!(text.len() <= 4096);
        assert!(text.ends_with("\nlast line"));
        let failure = Failure::new(
            "inspect",
            &text.repeat(10),
            &Ok(ExitStatus::from_raw(1)),
            false,
            &text.repeat(10),
            &text.repeat(10),
        );
        assert!(failure.reason.len() <= 2048);
        assert!(failure.stderr.len() <= 4096);
        assert!(failure.unrecognized_stdout.len() <= 2048);
    }

    #[test]
    fn only_latest_failure_is_retained() {
        let directory = std::env::temp_dir().join(format!(
            "cmt-worker-log-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        for command in ["first", "second"] {
            Failure::new(
                command,
                "Missing result",
                &Ok(ExitStatus::from_raw(1)),
                false,
                "",
                "",
            )
            .write_latest(&directory);
        }
        let files = std::fs::read_dir(directory.join("Logs"))
            .unwrap()
            .collect::<Vec<_>>();
        assert_eq!(files.len(), 1);
        let value: serde_json::Value = serde_json::from_slice(
            &std::fs::read(directory.join("Logs/worker-failure.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(value["command"], "second");
        std::fs::remove_file(directory.join("Logs/worker-failure.json")).unwrap();
        std::fs::remove_dir(directory.join("Logs")).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }
}
