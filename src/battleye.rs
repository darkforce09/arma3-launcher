//! BattlEye's status reports. Started with log mode 1, the BattlEye launcher
//! (`Arma3BattlEye.exe`) writes one status byte to its standard output per change; codes 6 and 11
//! are followed by a length byte and that many bytes of UTF-8 text
//! (`GameExecutor.MonitorBattlEyeProcess`, `BattlEyeStatusCode`). Under Proton the launch chain
//! shares that output: a byte past the last code starts a line of someone else's text, which is
//! skipped through its newline (itself code 10) instead of being reported like vanilla does.

use std::io::Read;

const INITIALIZATION_FAILED: u8 = 6;
const FILE_LOADING_BLOCKED: u8 = 11;
/// `StartFailedWillRetry`, the last code.
const LAST_CODE: u8 = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

impl Severity {
    /// The log icon (`BattlEyeSeverityConverter`).
    pub fn icon(self) -> &'static str {
        match self {
            Severity::Info => "be_status_green.png",
            Severity::Warning => "be_status_yellow.png",
            Severity::Error => "be_status_red.png",
        }
    }
}

/// One status report: the code and, for codes 6 and 11, its text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    pub code: u8,
    pub text: Option<String>,
}

impl Status {
    /// `IsError`, `IsInfo`, otherwise a warning.
    pub fn severity(&self) -> Severity {
        match self.code {
            2 | 4 | 6 | 10 => Severity::Error,
            0 | 1 | 3 | 7 | 9 => Severity::Info,
            _ => Severity::Warning,
        }
    }

    /// The BattlEye page's text for the status (`BattlEyeToDisplayStrings`).
    pub fn log_message(&self) -> String {
        let text = self.text.as_deref().unwrap_or_default();
        match self.code {
            0 => "Installing BattlEye Service...".into(),
            1 => "BattlEye Service installed".into(),
            2 => "BattlEye Service installation failed.".into(),
            3 => "Starting BattlEye Service...".into(),
            4 | 12 => "BattlEye Service startup failed.".into(),
            5 => "BattlEye Service communication error.".into(),
            6 => format!("Failed to initialize BattlEye Service: {text}."),
            7 => "Updating BattlEye Service...".into(),
            8 => "BattlEye Service update failed. The master server is offline or unreachable. Please ensure that you are connected to the internet and BEService.exe is not blocked by a firewall.".into(),
            9 => "Launching the game...".into(),
            10 => "Launching the game failed.".into(),
            11 => format!("Blocked loading of file '{text}'"),
            _ => "Unknown BattlEye status".into(),
        }
    }

    /// The error that offers a restart without BattlEye (`BEError`), for the failures that stop
    /// the game.
    pub fn error_message(&self) -> Option<String> {
        let text = self.text.as_deref().unwrap_or_default();
        Some(match self.code {
            2 => "BattlEye Service installation failed.".into(),
            4 => "BattlEye Service startup failed.".into(),
            6 => format!("Failed to initialize BattlEye Service: {text}."),
            10 => "Launching the game failed.".into(),
            _ => return None,
        })
    }
}

/// Reads status reports from the BattlEye launcher's output until it closes.
pub fn read_statuses(mut output: impl Read, mut on_status: impl FnMut(Status)) {
    let mut byte = [0u8];
    while output.read_exact(&mut byte).is_ok() {
        let code = byte[0];
        if code > LAST_CODE {
            while byte[0] != b'\n' && output.read_exact(&mut byte).is_ok() {}
            continue;
        }
        let text = if matches!(code, INITIALIZATION_FAILED | FILE_LOADING_BLOCKED) {
            read_text(&mut output)
        } else {
            None
        };
        on_status(Status { code, text });
    }
}

fn read_text(output: &mut impl Read) -> Option<String> {
    let mut len = [0u8];
    output.read_exact(&mut len).ok()?;
    let mut text = vec![0; len[0] as usize];
    output.read_exact(&mut text).ok()?;
    Some(String::from_utf8_lossy(&text).into_owned())
}

/// A line of the BattlEye page's log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    /// `HH:mm:ss`.
    pub time: String,
    pub text: String,
    pub severity: Severity,
}

/// The BattlEye page's log (`BattlEyeViewModel.Messages`).
#[derive(Default)]
pub struct Log {
    pub messages: Vec<Message>,
    last_code: Option<u8>,
}

impl Log {
    /// Adds a status; the first of a run of blocked files is preceded by a note that blocks can
    /// be ignored.
    pub fn push(&mut self, status: &Status, time: &str) {
        if status.code == FILE_LOADING_BLOCKED && self.last_code != Some(FILE_LOADING_BLOCKED) {
            self.messages.push(Message {
                time: time.to_owned(),
                text: "BattlEye: Note that file blocks can be ignored if they don't cause problems with the game.".into(),
                severity: Severity::Info,
            });
        }
        self.messages.push(Message {
            time: time.to_owned(),
            text: status.log_message(),
            severity: status.severity(),
        });
        self.last_code = Some(status.code);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_codes_and_their_texts() {
        let mut output = vec![3];
        output.extend(b"fsync: up and running.\n");
        output.extend([1, 11, 7]);
        output.extend(b"abc.dll");
        output.extend([6, 3]);
        output.extend(b"err");
        output.push(9);
        let mut statuses = Vec::new();
        read_statuses(output.as_slice(), |s| statuses.push(s));
        let codes: Vec<(u8, Option<&str>)> = statuses
            .iter()
            .map(|s| (s.code, s.text.as_deref()))
            .collect();
        assert_eq!(
            codes,
            [
                (3, None),
                (1, None),
                (11, Some("abc.dll")),
                (6, Some("err")),
                (9, None)
            ]
        );
        assert_eq!(statuses[3].severity(), Severity::Error);
        assert!(statuses[3].error_message().is_some());
        assert_eq!(statuses[2].severity(), Severity::Warning);
        assert_eq!(statuses[4].severity(), Severity::Info);
    }

    #[test]
    fn a_truncated_text_still_reports_the_status() {
        let mut statuses = Vec::new();
        read_statuses([11u8, 5, b'a'].as_slice(), |s| statuses.push(s));
        assert_eq!(
            statuses,
            [Status {
                code: 11,
                text: None
            }]
        );
    }

    #[test]
    fn notes_the_first_blocked_file_of_a_run() {
        let mut log = Log::default();
        let blocked = Status {
            code: 11,
            text: Some("x".into()),
        };
        log.push(&blocked, "10:00:00");
        log.push(&blocked, "10:00:01");
        log.push(
            &Status {
                code: 9,
                text: None,
            },
            "10:00:02",
        );
        log.push(&blocked, "10:00:03");
        let severities: Vec<Severity> = log.messages.iter().map(|m| m.severity).collect();
        assert_eq!(
            severities,
            [
                Severity::Info,
                Severity::Warning,
                Severity::Warning,
                Severity::Info,
                Severity::Info,
                Severity::Warning
            ]
        );
    }
}
