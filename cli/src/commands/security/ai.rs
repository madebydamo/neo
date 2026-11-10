//! The review: one OpenAI-compatible chat completion per finding group, with
//! the evidence in the prompt. Works against Hermes' API server (which runs
//! its own agent and may look around the system itself) as well as plain
//! model endpoints (Ollama, OpenAI, …), since nothing depends on tool calls.
//!
//! HTTP goes through curl: the key is passed on stdin (`-K -`), never argv.

use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use super::config::AiConfig;
use super::model::{Group, Status, Verdict};
use super::scan::truncate;
use crate::utils::command::now_secs;

pub struct Client {
    base: String,
    model: Option<String>,
    key: Option<String>,
    hermes: bool,
}

impl Client {
    pub fn from_config(ai: &AiConfig) -> Result<Option<Self>> {
        if ai.source == "off" {
            return Ok(None);
        }
        let base = ai
            .base_url
            .clone()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow!("AI review: no server address"))?;
        let key = match &ai.key_file {
            Some(f) => Some(
                std::fs::read_to_string(f)
                    .with_context(|| format!("read AI key {f}"))?
                    .trim()
                    .to_string(),
            ),
            None => None,
        };
        Ok(Some(Self {
            base: base.trim_end_matches('/').to_string(),
            model: ai.model.clone().filter(|s| !s.is_empty()),
            key: key.filter(|k| !k.is_empty()),
            hermes: ai.source == "hermes",
        }))
    }

    /// "Hermes (model)" / the model name, for the page and `Verdict.by`.
    pub fn label(&self) -> String {
        match (&self.model, self.hermes) {
            (Some(m), true) => format!("Hermes ({m})"),
            (None, true) => "Hermes".into(),
            (Some(m), false) => m.clone(),
            (None, false) => self.base.clone(),
        }
    }

    /// Review several findings of one package in one request. Returns a
    /// verdict per vulnerability id the answer covered.
    pub fn review(
        &self,
        batch: &[(&Group, Option<Value>)],
        work: &Path,
    ) -> Result<Vec<(String, Verdict)>> {
        let model = self.model.clone().unwrap_or_else(|| {
            if self.hermes {
                "hermes-agent".into()
            } else {
                "default".into()
            }
        });
        let body = json!({
            "model": model,
            "messages": [
                {"role": "system", "content": system_prompt(self.hermes)},
                {"role": "user", "content": user_prompt(batch)},
            ],
            "temperature": 0.1,
            "stream": false,
        });
        let text = self.post(
            "/chat/completions",
            &body,
            work,
            if self.hermes { 1800 } else { 600 },
        )?;
        let v: Value = serde_json::from_str(&text)
            .with_context(|| format!("AI answer is not JSON: {}", truncate(&text, 300)))?;
        let content = v["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| anyhow!("AI answer without content: {}", truncate(&text, 300)))?;
        let by = format!("ai:{}", self.label());
        Ok(parse_verdicts(content)?
            .into_iter()
            .map(|(id, mut v)| {
                v.by = by.clone();
                (id, v)
            })
            .collect())
    }

    /// POST with retries: a restarting Hermes answers 503 (runtime_draining)
    /// or refuses connections for a while, a busy API 429.
    fn post(&self, path: &str, body: &Value, work: &Path, timeout: u32) -> Result<String> {
        let body_file = work.join(format!("ai-request-{}.json", std::process::id()));
        std::fs::write(&body_file, serde_json::to_vec(body)?).context("write AI request")?;
        let mut result = self.post_once(path, &body_file, timeout);
        for wait in [15, 45, 90] {
            match &result {
                Err(e) if is_transient(&format!("{e:#}")) => {
                    eprintln!("security: AI busy or restarting, retrying in {wait}s");
                    std::thread::sleep(std::time::Duration::from_secs(wait));
                    result = self.post_once(path, &body_file, timeout);
                }
                _ => break,
            }
        }
        let _ = std::fs::remove_file(&body_file);
        result
    }

    fn post_once(&self, path: &str, body_file: &Path, timeout: u32) -> Result<String> {
        let mut cfg = String::new();
        if let Some(k) = &self.key {
            cfg.push_str(&format!(
                "header = \"Authorization: Bearer {}\"\n",
                k.replace('"', "")
            ));
        }
        let mut child = Command::new("curl")
            .args([
                "-sS",
                "--fail-with-body",
                "-X",
                "POST",
                "-H",
                "Content-Type: application/json",
            ])
            .arg("-m")
            .arg(timeout.to_string())
            .arg("--data-binary")
            .arg(format!("@{}", body_file.display()))
            .args(["-K", "-"])
            .arg(format!("{}{}", self.base, path))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("run curl")?;
        child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("curl stdin"))?
            .write_all(cfg.as_bytes())
            .context("curl config")?;
        let out = child.wait_with_output().context("curl")?;
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        if !out.status.success() {
            bail!(
                "AI request failed: {} {}",
                String::from_utf8_lossy(&out.stderr).trim(),
                truncate(&stdout, 300)
            );
        }
        Ok(stdout)
    }
}

/// 5xx, 429 and connection failures (curl 7 refused, 52 empty reply,
/// 56 reset); not timeouts, which would only repeat a long wait.
fn is_transient(err: &str) -> bool {
    [
        "error: 5",
        "error: 429",
        "curl: (7)",
        "curl: (52)",
        "curl: (56)",
    ]
    .iter()
    .any(|m| err.contains(m))
}

/// OSV's record for a CVE/GHSA id (summary, details, affected ranges), or None.
pub fn osv_record(id: &str) -> Option<Value> {
    let out = Command::new("curl")
        .args(["-sS", "--fail", "-m", "20"])
        .arg(format!("https://api.osv.dev/v1/vulns/{id}"))
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let v: Value = serde_json::from_slice(&out.stdout).ok()?;
    // Keep what helps the review; the full record can be huge.
    let affected: Vec<Value> = v["affected"]
        .as_array()
        .into_iter()
        .flatten()
        .take(8)
        .map(|a| {
            json!({
                "package": a["package"],
                "ranges": a["ranges"],
            })
        })
        .collect();
    Some(json!({
        "summary": v["summary"],
        "details": v["details"].as_str().map(|d| truncate(d, 2500)),
        "aliases": v["aliases"],
        "affected": affected,
    }))
}

fn system_prompt(hermes: bool) -> String {
    let mut s = String::from(
        "You review findings of an automatic vulnerability scan on a self-hosted home server \
(NixOS with Docker containers, managed by Neo). Scanners produce many false alarms: CVEs matched to an \
unrelated project with the same name, CVEs in parts of a base image the app never uses, fixes already \
backported by the distribution or nixpkgs, issues that need local access on a single-user machine. \
You get one package and one or more vulnerabilities reported in it. Judge each vulnerability on its own: \
does it matter on THIS server, given the evidence?\n\n\
Answer with only one JSON object, no other text, with one entry per vulnerability:\n\
{\"verdicts\": [{\"vulnerability\": the id as given,\n \"status\": \"attention\" | \"waiting\" | \"harmless\",\n \
\"confidence\": \"low\" | \"medium\" | \"high\",\n \"summary\": one plain sentence for a non-expert,\n \
\"explanation\": 2-4 sentences with the reasons, citing the evidence,\n \"action\": what the operator can do, one sentence}]}\n\n\
status meanings:\n\
- attention: real and relevant here (reachable code, exposed service, or serious enough to act on).\n\
- waiting: real, but only the image maintainers or nixpkgs can fix it and it is not urgent here.\n\
- harmless: false alarm, wrong product, already fixed, or the vulnerable code is not reachable on this server.\n\
'loaded_by' lists running processes that have the package's files mapped in memory; an empty list with \
load_checked=true means no running process uses the library right now. Services on the internet without \
the tinyauth login deserve the most care.",
    );
    if hermes {
        s.push_str(
            "\n\nYou may look at the system read-only (versions, files, /proc) to check the finding, \
but do not change anything and do not install or update anything.",
        );
    }
    s
}

/// The package and its evidence once, then each vulnerability.
fn user_prompt(batch: &[(&Group, Option<Value>)]) -> String {
    let Some((g, _)) = batch.first() else {
        return String::new();
    };
    let package = json!({
        "package": g.package,
        "installed_version": g.version,
        "found_in": match g.origin { super::model::Origin::Nix => "NixOS system closure (nixpkgs)", super::model::Origin::Image => "Docker image" },
        "package_type": g.pkg_class,
        "package_paths": g.pkg_paths.iter().take(5).collect::<Vec<_>>(),
        "where": g.targets.iter().take(12).map(|t| json!({
            "image_or_system": t.name, "containers": t.containers, "services": t.services,
        })).collect::<Vec<_>>(),
        "loaded_by": g.evidence.loaded_by,
        "load_checked": g.evidence.load_checked,
        "exposure": g.evidence.exposure,
        "notes": g.evidence.notes,
    });
    let vulns: Vec<Value> = batch
        .iter()
        .map(|(g, osv)| {
            json!({
                "vulnerability": g.vuln_id,
                "severity": g.severity,
                "cvss": g.cvss,
                "fixed_version": g.fixed_version,
                "scanner_status": g.status,
                "reported_by_scanners": g.scanners,
                "title": g.title,
                "description": g.description.as_deref().map(|d| truncate(d, 1200)),
                "url": g.url,
                "osv": osv,
            })
        })
        .collect();
    format!(
        "Package:\n```json\n{}\n```\n\nVulnerabilities ({}):\n```json\n{}\n```",
        serde_json::to_string_pretty(&package).unwrap_or_default(),
        vulns.len(),
        serde_json::to_string_pretty(&vulns).unwrap_or_default()
    )
}

#[derive(Deserialize)]
struct RawVerdict {
    #[serde(default)]
    vulnerability: String,
    status: String,
    #[serde(default)]
    confidence: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    explanation: String,
    #[serde(default)]
    action: String,
}

/// The JSON object in a model answer (it may come wrapped in prose or a code
/// fence): `{"verdicts": [...]}`, or a single verdict object. Entries with an
/// unknown status are skipped; their findings stay open.
pub fn parse_verdicts(content: &str) -> Result<Vec<(String, Verdict)>> {
    let start = content
        .find('{')
        .ok_or_else(|| anyhow!("no JSON in AI answer: {}", truncate(content, 200)))?;
    let end = content
        .rfind('}')
        .ok_or_else(|| anyhow!("no JSON in AI answer"))?;
    let v: Value = serde_json::from_str(&content[start..=end])
        .with_context(|| format!("AI verdict: {}", truncate(content, 300)))?;
    let entries: Vec<Value> = match v.get("verdicts").and_then(Value::as_array) {
        Some(a) => a.clone(),
        None => vec![v],
    };
    let out: Vec<(String, Verdict)> = entries
        .into_iter()
        .filter_map(|e| serde_json::from_value::<RawVerdict>(e).ok())
        .filter_map(|raw| {
            let status = match raw.status.trim().to_ascii_lowercase().as_str() {
                "attention" | "affected" | "needs_attention" => Status::Attention,
                "waiting" | "upstream" => Status::Waiting,
                "harmless" | "not_affected" | "false_positive" => Status::Harmless,
                _ => return None,
            };
            let confidence = match raw.confidence.trim().to_ascii_lowercase().as_str() {
                c @ ("low" | "medium" | "high") => c.to_string(),
                _ => "medium".into(),
            };
            Some((
                raw.vulnerability.trim().to_string(),
                Verdict {
                    status,
                    confidence,
                    summary: truncate(raw.summary.trim(), 300),
                    explanation: truncate(raw.explanation.trim(), 1500),
                    action: truncate(raw.action.trim(), 300),
                    by: String::new(),
                    at: now_secs(),
                },
            ))
        })
        .collect();
    if out.is_empty() {
        bail!(
            "AI answer has no usable verdict: {}",
            truncate(content, 300)
        );
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verdicts_from_fenced_answer() {
        let v = parse_verdicts(
            "Here you go:\n```json\n{\"verdicts\":[{\"vulnerability\":\"CVE-1\",\"status\":\"harmless\",\"confidence\":\"HIGH\",\"summary\":\"s\",\"explanation\":\"e\",\"action\":\"a\"},{\"vulnerability\":\"CVE-2\",\"status\":\"maybe\"}]}\n```",
        )
        .unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].0, "CVE-1");
        assert_eq!(v[0].1.status, Status::Harmless);
        assert_eq!(v[0].1.confidence, "high");
    }

    #[test]
    fn transient_errors() {
        assert!(is_transient(
            "curl: (22) The requested URL returned error: 503 {}"
        ));
        assert!(is_transient("curl: (7) Failed to connect"));
        assert!(!is_transient(
            "curl: (22) The requested URL returned error: 401"
        ));
        assert!(!is_transient("curl: (28) Operation timed out"));
    }

    #[test]
    fn single_object_answer_and_garbage() {
        let v = parse_verdicts("{\"status\":\"waiting\"}").unwrap();
        assert_eq!(v[0].1.status, Status::Waiting);
        assert!(parse_verdicts("{\"status\":\"maybe\"}").is_err());
        assert!(parse_verdicts("no json").is_err());
    }
}
