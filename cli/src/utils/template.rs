//! Template-provided files of a config repo (`nix flake init -t <template>`).
//!
//! The files a template provided are kept verbatim under [`PRISTINE_DIR`], a
//! local record that `.gitignore` keeps out of the repo. A refresh rolls those back and applies the new template, so files the operator
//! added (machine overrides in `modules/`, …) survive `neo update`:
//!
//! - unchanged template file → replaced by the new version, or removed when
//!   the new template no longer has it;
//! - edited template file → three-way merged (`git merge-file`, base = the
//!   pristine copy); on conflict the edit stays and the new version is written
//!   beside it as `<file>.neo-new`;
//! - any other file → untouched.
//!
//! `flake.nix` is owned by `write-flake`: written when missing, never replaced.

use anyhow::{bail, Context, Result};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::{git_cmd, run_nix};

/// Verbatim copy of the files the last template provided (git-ignored).
pub const PRISTINE_DIR: &str = ".neo-template";
/// Suffix of the new template version of a file whose edits did not merge.
pub const NEW_SUFFIX: &str = ".neo-new";
const FLAKE: &str = "flake.nix";

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub written: Vec<String>,
    pub removed: Vec<String>,
    pub merged: Vec<String>,
    /// Kept as edited; the new template version is in `<file>.neo-new`.
    pub conflicts: Vec<String>,
    /// Edited (or deleted) locally and left alone although the template changed or dropped it.
    pub kept: Vec<String>,
}

impl Report {
    pub fn print(&self) {
        for (label, files) in [
            ("updated", &self.written),
            ("removed", &self.removed),
            ("merged with local edits", &self.merged),
            ("kept local version", &self.kept),
        ] {
            if !files.is_empty() {
                println!("template: {label}: {}", files.join(", "));
            }
        }
        for f in &self.conflicts {
            eprintln!(
                "warning: template: {f} has local edits that conflict with the new template; \
                 kept yours, the new version is in {f}{NEW_SUFFIX}"
            );
        }
    }
}

/// Stage what a refresh changed (flakes only see tracked files): written and
/// merged files; removed files leave the index. Side files of conflicts stay
/// untracked for the operator.
pub fn stage(config_path: &str, report: &Report) -> Result<()> {
    let dir = Path::new(config_path);
    let add: Vec<&str> = report
        .written
        .iter()
        .chain(&report.merged)
        .map(String::as_str)
        .filter(|p| dir.join(p).exists())
        .collect();
    if !add.is_empty() {
        let mut args = vec!["add", "-A", "--"];
        args.extend(add);
        git_cmd(config_path, &args)?;
    }
    if !report.removed.is_empty() {
        let mut args = vec!["rm", "-q", "--cached", "--ignore-unmatch", "--"];
        args.extend(report.removed.iter().map(String::as_str));
        git_cmd(config_path, &args)?;
    }
    Ok(())
}

/// Instantiate `template` into a scratch directory and apply it to `config_path`.
pub fn install(config_path: &str, template: &str, nix_cmd: &str) -> Result<Report> {
    let scratch = std::env::temp_dir().join(format!(
        "neo-template-{}-{}",
        std::process::id(),
        super::get_timestamp()
    ));
    if scratch.exists() {
        fs::remove_dir_all(&scratch).context("clear template scratch dir")?;
    }
    fs::create_dir_all(&scratch).context("create template scratch dir")?;
    let result = run_nix(
        &scratch.to_string_lossy(),
        nix_cmd,
        &["flake", "init", "-t", template],
    )
    .and_then(|_| apply(Path::new(config_path), &scratch));
    let _ = fs::remove_dir_all(&scratch);
    result
}

/// Apply the template files in `new_dir` to the config repo `dir`.
pub fn apply(dir: &Path, new_dir: &Path) -> Result<Report> {
    let pristine_dir = dir.join(PRISTINE_DIR);
    let new_files = list_files(new_dir)?;
    let old_files = if pristine_dir.is_dir() {
        list_files(&pristine_dir)?
    } else {
        BTreeSet::new()
    };
    // Before the pristine copy existed every template file was overwritten by
    // update; keep doing that for those once, other files are kept from now on.
    let legacy = !pristine_dir.is_dir();
    let mut report = Report::default();
    // A conflicted file keeps its previous base, so the template change is
    // offered again (and merges once the operator resolved it).
    let mut keep_base: Vec<(String, Option<Vec<u8>>)> = Vec::new();

    for rel in old_files.union(&new_files) {
        if rel == FLAKE {
            continue;
        }
        let cur_path = dir.join(rel);
        let side = sibling(&cur_path, NEW_SUFFIX);
        if side.exists() {
            fs::remove_file(&side).with_context(|| format!("remove {}", side.display()))?;
        }
        let cur = read_opt(&cur_path)?;
        let old = old_files
            .contains(rel)
            .then(|| read_opt(&pristine_dir.join(rel)))
            .transpose()?
            .flatten();
        let new = new_files
            .contains(rel)
            .then(|| read_opt(&new_dir.join(rel)))
            .transpose()?
            .flatten();

        match (old, new, cur) {
            (_, None, None) => {}
            // Dropped by the template.
            (Some(old), None, Some(cur)) => {
                if cur == old {
                    fs::remove_file(&cur_path)
                        .with_context(|| format!("remove {}", cur_path.display()))?;
                    report.removed.push(rel.clone());
                } else {
                    report.kept.push(rel.clone());
                }
            }
            (None, None, Some(_)) => {}
            // Deleted locally: respect it.
            (Some(_), Some(_), None) => report.kept.push(rel.clone()),
            (None, Some(new), None) => {
                write(&cur_path, &new)?;
                report.written.push(rel.clone());
            }
            (None, Some(new), Some(cur)) => {
                if cur == new {
                    continue;
                }
                if legacy {
                    write(&cur_path, &new)?;
                    report.written.push(rel.clone());
                } else {
                    write(&side, &new)?;
                    report.conflicts.push(rel.clone());
                    keep_base.push((rel.clone(), None));
                }
            }
            (Some(old), Some(new), Some(cur)) => {
                if cur == new {
                    continue;
                }
                if cur == old {
                    write(&cur_path, &new)?;
                    report.written.push(rel.clone());
                } else if new == old {
                    // Template unchanged: the local edit stands.
                } else {
                    match merge(&cur, &old, &new)? {
                        Some(merged) => {
                            write(&cur_path, &merged)?;
                            report.merged.push(rel.clone());
                        }
                        None => {
                            write(&side, &new)?;
                            report.conflicts.push(rel.clone());
                            keep_base.push((rel.clone(), Some(old)));
                        }
                    }
                }
            }
        }
    }

    // `flake.nix` only when missing (a fresh repo); write-flake regenerates it.
    let flake = dir.join(FLAKE);
    if !flake.exists() {
        if let Some(text) = read_opt(&new_dir.join(FLAKE))? {
            write(&flake, &text)?;
            report.written.push(FLAKE.to_string());
        }
    }

    // Record what this template provided.
    if pristine_dir.exists() {
        fs::remove_dir_all(&pristine_dir).context("replace pristine template copy")?;
    }
    for rel in &new_files {
        if rel == FLAKE {
            continue;
        }
        let base = match keep_base.iter().find(|(p, _)| p == rel) {
            Some((_, base)) => base.clone(),
            None => read_opt(&new_dir.join(rel))?,
        };
        if let Some(text) = base {
            write(&pristine_dir.join(rel), &text)?;
        }
    }
    ignore_pristine(dir)?;
    Ok(report)
}

/// Keep the pristine copy out of the repo even when `.gitignore` is the
/// operator's own (the template's lists it already).
fn ignore_pristine(dir: &Path) -> Result<()> {
    let path = dir.join(".gitignore");
    let text = fs::read_to_string(&path).unwrap_or_default();
    let listed = text.lines().any(|l| {
        let l = l.trim().trim_start_matches('/').trim_end_matches('/');
        l == PRISTINE_DIR
    });
    if listed {
        return Ok(());
    }
    let sep = if text.is_empty() || text.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    write(&path, format!("{text}{sep}/{PRISTINE_DIR}/\n").as_bytes())
}

/// Clean three-way merge of `cur` (local) and `new` over `base`, or `None` on conflict.
fn merge(cur: &[u8], base: &[u8], new: &[u8]) -> Result<Option<Vec<u8>>> {
    let tmp = std::env::temp_dir().join(format!(
        "neo-merge-{}-{}",
        std::process::id(),
        super::get_timestamp()
    ));
    fs::create_dir_all(&tmp)?;
    let paths = [tmp.join("local"), tmp.join("base"), tmp.join("template")];
    let result = (|| {
        for (p, text) in paths.iter().zip([cur, base, new]) {
            fs::write(p, text)?;
        }
        let out = Command::new("git")
            .args([
                "merge-file",
                "-p",
                "-L",
                "local",
                "-L",
                "base",
                "-L",
                "template",
            ])
            .args(&paths)
            .output()
            .context("run git merge-file")?;
        match out.status.code() {
            Some(0) => Ok(Some(out.stdout)),
            Some(n) if n > 0 => Ok(None),
            _ => bail!(
                "git merge-file failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ),
        }
    })();
    let _ = fs::remove_dir_all(&tmp);
    result
}

/// Relative paths of all files under `root` (skipping `.git` and `result`).
fn list_files(root: &Path) -> Result<BTreeSet<String>> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeSet<String>) -> Result<()> {
        for entry in fs::read_dir(dir).with_context(|| format!("read {}", dir.display()))? {
            let entry = entry?;
            let path = entry.path();
            let name = entry.file_name();
            if name == ".git" || name == "result" {
                continue;
            }
            if entry.file_type()?.is_dir() {
                walk(root, &path, out)?;
            } else if let Ok(rel) = path.strip_prefix(root) {
                out.insert(rel.to_string_lossy().to_string());
            }
        }
        Ok(())
    }
    let mut out = BTreeSet::new();
    walk(root, root, &mut out)?;
    Ok(out)
}

fn read_opt(path: &Path) -> Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(b) => Ok(Some(b)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("read {}", path.display())),
    }
}

fn write(path: &Path, text: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    // `nix flake init` copies store files read-only.
    if path.exists() {
        let mut perms = fs::metadata(path)?.permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(false);
        fs::set_permissions(path, perms)?;
    }
    fs::write(path, text).with_context(|| format!("write {}", path.display()))
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Dirs {
        root: PathBuf,
    }

    impl Dirs {
        fn new(name: &str) -> Self {
            let root =
                std::env::temp_dir().join(format!("neo-tpl-test-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(root.join("cfg")).unwrap();
            Self { root }
        }
        fn cfg(&self) -> PathBuf {
            self.root.join("cfg")
        }
        fn template(&self, name: &str, files: &[(&str, &str)]) -> PathBuf {
            let d = self.root.join(name);
            for (rel, text) in files {
                write(&d.join(rel), text.as_bytes()).unwrap();
            }
            d
        }
        fn read(&self, rel: &str) -> Option<String> {
            fs::read_to_string(self.cfg().join(rel)).ok()
        }
        fn put(&self, rel: &str, text: &str) {
            write(&self.cfg().join(rel), text.as_bytes()).unwrap();
        }
    }

    impl Drop for Dirs {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn update_keeps_operator_files_and_rolls_back_template_files() {
        let d = Dirs::new("rollback");
        let v1 = d.template(
            "v1",
            &[
                ("flake.nix", "flake v1\n"),
                ("modules/a.nix", "a1\n"),
                ("modules/old.nix", "old\n"),
            ],
        );
        apply(&d.cfg(), &v1).unwrap();
        assert_eq!(d.read("flake.nix").as_deref(), Some("flake v1\n"));
        d.put("modules/machine.nix", "mine\n");
        d.put("flake.nix", "regenerated\n");

        let v2 = d.template(
            "v2",
            &[
                ("flake.nix", "flake v2\n"),
                ("modules/a.nix", "a2\n"),
                ("modules/b.nix", "b\n"),
            ],
        );
        let r = apply(&d.cfg(), &v2).unwrap();
        assert_eq!(d.read("modules/a.nix").as_deref(), Some("a2\n"));
        assert_eq!(d.read("modules/b.nix").as_deref(), Some("b\n"));
        assert_eq!(d.read("modules/old.nix"), None);
        assert_eq!(d.read("modules/machine.nix").as_deref(), Some("mine\n"));
        assert_eq!(d.read("flake.nix").as_deref(), Some("regenerated\n"));
        assert_eq!(r.removed, vec!["modules/old.nix"]);
        assert_eq!(
            d.read(".neo-template/modules/b.nix").as_deref(),
            Some("b\n")
        );
        assert_eq!(d.read(".neo-template/flake.nix"), None);
        assert_eq!(d.read(".gitignore").as_deref(), Some("/.neo-template/\n"));
    }

    #[test]
    fn edited_template_files_merge_or_keep_with_side_file() {
        let d = Dirs::new("merge");
        let v1 = d.template("v1", &[("modules/s.nix", "one\ntwo\nthree\nfour\nfive\n")]);
        apply(&d.cfg(), &v1).unwrap();
        d.put("modules/s.nix", "one\ntwo\nthree\nfour\nFIVE\n");

        let v2 = d.template("v2", &[("modules/s.nix", "ONE\ntwo\nthree\nfour\nfive\n")]);
        let r = apply(&d.cfg(), &v2).unwrap();
        assert_eq!(r.merged, vec!["modules/s.nix"]);
        assert_eq!(
            d.read("modules/s.nix").as_deref(),
            Some("ONE\ntwo\nthree\nfour\nFIVE\n")
        );

        let v3 = d.template("v3", &[("modules/s.nix", "ONE\ntwo\nthree\nfour\nfive!\n")]);
        let r = apply(&d.cfg(), &v3).unwrap();
        assert_eq!(r.conflicts, vec!["modules/s.nix"]);
        assert_eq!(
            d.read("modules/s.nix").as_deref(),
            Some("ONE\ntwo\nthree\nfour\nFIVE\n")
        );
        assert_eq!(
            d.read("modules/s.nix.neo-new").as_deref(),
            Some("ONE\ntwo\nthree\nfour\nfive!\n")
        );
        assert_eq!(
            d.read(".neo-template/modules/s.nix").as_deref(),
            Some("ONE\ntwo\nthree\nfour\nfive\n")
        );
        // Unresolved, the next refresh offers the change again.
        let r = apply(&d.cfg(), &v3).unwrap();
        assert_eq!(r.conflicts, vec!["modules/s.nix"]);
        // Resolved, the next refresh is clean and the base moves on.
        d.put("modules/s.nix", "ONE\ntwo\nthree\nfour\nfive!\n");
        let r = apply(&d.cfg(), &v3).unwrap();
        assert_eq!(r, Report::default());
        assert_eq!(d.read("modules/s.nix.neo-new"), None);
    }

    #[test]
    fn legacy_repo_overwrites_template_names_once_and_keeps_the_rest() {
        let d = Dirs::new("legacy");
        d.put("flake.nix", "existing\n");
        d.put("modules/settings.nix", "old template\n");
        d.put("modules/machine.nix", "mine\n");
        let v = d.template(
            "v",
            &[("flake.nix", "f\n"), ("modules/settings.nix", "new\n")],
        );
        let r = apply(&d.cfg(), &v).unwrap();
        assert_eq!(r.written, vec!["modules/settings.nix"]);
        assert_eq!(d.read("modules/settings.nix").as_deref(), Some("new\n"));
        assert_eq!(d.read("modules/machine.nix").as_deref(), Some("mine\n"));
        assert_eq!(d.read("flake.nix").as_deref(), Some("existing\n"));
    }

    #[test]
    fn gitignore_gains_the_pristine_dir_once() {
        let d = Dirs::new("ignore");
        d.put(".gitignore", "result");
        let v = d.template("v", &[("modules/a.nix", "a\n")]);
        apply(&d.cfg(), &v).unwrap();
        apply(&d.cfg(), &v).unwrap();
        assert_eq!(
            d.read(".gitignore").as_deref(),
            Some("result\n/.neo-template/\n")
        );
        let v = d.template(
            "w",
            &[
                (".gitignore", "result\n/.neo-template/\n"),
                ("modules/a.nix", "a\n"),
            ],
        );
        apply(&d.cfg(), &v).unwrap();
        assert_eq!(
            d.read(".gitignore").as_deref(),
            Some("result\n/.neo-template/\n")
        );
    }

    #[test]
    fn locally_deleted_and_dropped_edited_files_stay_as_they_are() {
        let d = Dirs::new("deleted");
        let v1 = d.template("v1", &[("modules/a.nix", "a\n"), ("modules/b.nix", "b\n")]);
        apply(&d.cfg(), &v1).unwrap();
        fs::remove_file(d.cfg().join("modules/a.nix")).unwrap();
        d.put("modules/b.nix", "b edited\n");
        let v2 = d.template("v2", &[("modules/a.nix", "a2\n")]);
        let r = apply(&d.cfg(), &v2).unwrap();
        assert_eq!(d.read("modules/a.nix"), None);
        assert_eq!(d.read("modules/b.nix").as_deref(), Some("b edited\n"));
        assert_eq!(r.kept, vec!["modules/a.nix", "modules/b.nix"]);
    }
}
