//! Live journal of one unit for the logs dialog: the last 100 lines, then
//! whatever follows. The journalctl child is killed when this is dropped (the
//! client closed the EventSource).
use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, BufReader, Lines};
use tokio::process::{Child, ChildStdout, Command};

use crate::utils::bins;

pub struct JournalFollow {
    _child: Child,
    lines: Lines<BufReader<ChildStdout>>,
}

impl JournalFollow {
    /// `journalctl -u <unit> -n 100 -f` (neo-web's user is in `systemd-journal`).
    /// The unit name must be checked by the caller (`unit_name_valid`). `Err` is
    /// a message for the dialog.
    pub fn start(unit: &str) -> Result<Self, String> {
        let mut child = Command::new(bins::journalctl())
            .args([
                "-u",
                unit,
                "-n",
                "100",
                "-f",
                "--no-pager",
                "-o",
                "short-iso",
            ])
            .kill_on_drop(true)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("spawn error: {e}"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or("spawn error: missing piped stdout")?;
        Ok(JournalFollow {
            _child: child,
            lines: BufReader::new(stdout).lines(),
        })
    }

    /// The next journal line; `None` when journalctl ended.
    pub async fn next_line(&mut self) -> std::io::Result<Option<String>> {
        self.lines.next_line().await
    }
}
