//! Which system generation was running when, from the Neo activation log and
//! the journal.

use std::process::Command;

use super::profiles::{generation_for_path, generation_links, GenerationLink};
use super::ACTIVATION_LOG;

/// Which system generation was running when. Built from activation records
/// (the Neo activation log, and the journal: `switching to system
/// configuration …` per switch/test, `init=…` on each boot's kernel command
/// line). Before the first record it falls back to "newest generation created
/// by then", which cannot see rollbacks.
#[derive(Debug, Default)]
pub struct GenerationTimeline {
    links: Vec<GenerationLink>,
    /// `(epoch, generation)` sorted by time; `None`: that system no longer has a
    /// generation (deleted) or never had one (`switch-to-configuration test`).
    events: Vec<(i64, Option<u64>)>,
}

impl GenerationTimeline {
    pub fn load(sudo_cmd: &str) -> Self {
        let links = generation_links();
        let mut records = read_activation_log();
        records.extend(journal_activations(sudo_cmd));
        Self::from_records(links, records)
    }

    fn from_records(links: Vec<GenerationLink>, mut records: Vec<(i64, String)>) -> Self {
        records.sort();
        let events = records
            .into_iter()
            .map(|(t, path)| (t, generation_for_path(&links, &path, None, Some(t))))
            .collect();
        Self { links, events }
    }

    /// `(epoch, generation)` activation records, oldest first.
    pub fn events(&self) -> &[(i64, Option<u64>)] {
        &self.events
    }

    /// No generations on this machine (dev VM, non-NixOS).
    pub fn is_empty(&self) -> bool {
        self.links.is_empty()
    }

    /// Generation that was running at `t` (epoch seconds).
    pub fn at(&self, t: i64) -> Option<u64> {
        if let Some((_, g)) = self.events.iter().rev().find(|(et, _)| *et <= t) {
            return *g;
        }
        self.links
            .iter()
            .filter(|l| l.created <= t)
            .max_by_key(|l| (l.created, l.number))
            .map(|l| l.number)
    }
}

fn read_activation_log() -> Vec<(i64, String)> {
    std::fs::read_to_string(ACTIVATION_LOG)
        .map(|s| s.lines().filter_map(parse_activation_log_line).collect())
        .unwrap_or_default()
}

/// `1790611135 /nix/store/…-nixos-system-…`
fn parse_activation_log_line(line: &str) -> Option<(i64, String)> {
    let (t, path) = line.trim().split_once(' ')?;
    let path = path.trim();
    path.starts_with("/nix/store/")
        .then(|| Some((t.parse().ok()?, path.to_string())))?
}

fn journal_lines(sudo_cmd: &str, args: &[&str]) -> Vec<String> {
    let out = Command::new(sudo_cmd)
        .args(["-n", "journalctl", "--no-pager", "-q", "-o", "short-unix"])
        .args(args)
        .output();
    match out {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .lines()
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

/// Switches and boots recorded in the journal (as far back as it reaches).
fn journal_activations(sudo_cmd: &str) -> Vec<(i64, String)> {
    let mut out = Vec::new();
    for line in journal_lines(
        sudo_cmd,
        &[
            "SYSLOG_IDENTIFIER=nixos",
            "-g",
            "^switching to system configuration ",
        ],
    ) {
        out.extend(parse_journal_switch(&line));
    }
    for line in journal_lines(
        sudo_cmd,
        &["_TRANSPORT=kernel", "-g", "^Command line: .*init="],
    ) {
        out.extend(parse_journal_boot(&line));
    }
    out
}

fn journal_epoch(line: &str) -> Option<i64> {
    let ts = line.split_whitespace().next()?;
    ts.split('.').next()?.parse().ok()
}

/// `1790611135.419086 orion nixos[3441407]: switching to system configuration /nix/store/…`
fn parse_journal_switch(line: &str) -> Option<(i64, String)> {
    let path = line
        .split_once("switching to system configuration ")?
        .1
        .trim();
    path.starts_with("/nix/store/")
        .then(|| Some((journal_epoch(line)?, path.to_string())))?
}

/// `1790612419.783699 orion kernel: Command line: … init=/nix/store/…/init …`
fn parse_journal_boot(line: &str) -> Option<(i64, String)> {
    let init = line
        .split_whitespace()
        .find_map(|w| w.strip_prefix("init="))?;
    let path = init.strip_suffix("/init")?;
    path.starts_with("/nix/store/")
        .then(|| Some((journal_epoch(line)?, path.to_string())))?
}

#[cfg(test)]
mod tests {
    use super::super::profiles::test_link as link;
    use super::*;

    #[test]
    fn timeline_follows_activations() {
        let links = vec![
            link(10, 100, "/nix/store/a"),
            link(12, 300, "/nix/store/c"),
            link(13, 400, "/nix/store/d"),
        ];
        // 12 switched at 300, 13 at 400, reboot into 12 at 500; /nix/store/x is gone.
        let records = vec![
            (500, "/nix/store/c".to_string()),
            (300, "/nix/store/c".to_string()),
            (400, "/nix/store/d".to_string()),
            (250, "/nix/store/x".to_string()),
        ];
        let tl = GenerationTimeline::from_records(links, records);
        assert_eq!(tl.at(150), Some(10)); // before any record: creation time
        assert_eq!(tl.at(260), None);
        assert_eq!(tl.at(350), Some(12));
        assert_eq!(tl.at(450), Some(13));
        assert_eq!(tl.at(600), Some(12)); // creation time alone would say 13
    }

    #[test]
    fn timeline_without_records_uses_creation_time() {
        let links = vec![
            link(10, 100, "/a"),
            link(11, 200, "/b"),
            link(12, 300, "/c"),
        ];
        let tl = GenerationTimeline::from_records(links, vec![]);
        assert_eq!(tl.at(50), None);
        assert_eq!(tl.at(200), Some(11));
        assert_eq!(tl.at(250), Some(11));
        assert_eq!(tl.at(1000), Some(12));
    }

    #[test]
    fn parse_activation_records() {
        assert_eq!(
            parse_activation_log_line("1790611135 /nix/store/9q-nixos-system"),
            Some((1790611135, "/nix/store/9q-nixos-system".to_string()))
        );
        assert_eq!(parse_activation_log_line("garbage"), None);
        assert_eq!(
            parse_journal_switch("1790611135.419086 orion nixos[3441407]: switching to system configuration /nix/store/9q-nixos-system"),
            Some((1790611135, "/nix/store/9q-nixos-system".to_string()))
        );
        assert_eq!(
            parse_journal_boot("1790612419.783699 orion kernel: Command line: BOOT_IMAGE=(hd0,gpt1)//kernels/k-bzImage init=/nix/store/9q-nixos-system/init nohibernate"),
            Some((1790612419, "/nix/store/9q-nixos-system".to_string()))
        );
    }
}
