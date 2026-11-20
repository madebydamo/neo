//! Restore keys of a backup store (docs/design/backup-restore.md §6, "Restore
//! keys"): the keys that can open an encrypted backup may also read it.
//!
//! After it writes `neo-backup.age`, an encrypted client uploads
//! `neo-backup.recipients` next to it: the public keys the header is encrypted
//! to, one `server <key>` line (its host key) and an `admin <key>` line per key
//! in System → SSH. The store believes a line only when the header holds a
//! stanza for that key (age's ssh-ed25519 tag), and gives each admin key the
//! gate's read-only mode for that client's account. The client can only grant
//! read access to its own backups, which it already controls.

use anyhow::{Context, Result};
use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD};
use base64::Engine;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use super::{StoreConfig, RECIPIENT_KEYS_DIR};
use crate::utils::fsutil::write_atomic;

/// Written by backup.sh next to the header.
pub const FILE: &str = "neo-backup.recipients";
const HEADER: &str = "neo-backup.age";
const KEY_TYPE: &str = "ssh-ed25519";
/// More lines than any server has admins: the rest is ignored.
const MAX_KEYS: usize = 64;
const MAX_COMMENT: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// The client's host key.
    Server,
    /// A key from System → SSH on the client.
    Admin,
}

/// One ssh-ed25519 key the header is encrypted to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recipient {
    pub role: Role,
    /// Base64 of the SSH wire-format public key (the `AAAA…` part).
    pub key: String,
    pub comment: String,
}

impl Recipient {
    /// `ssh-ed25519 AAAA… comment`
    pub fn line(&self) -> String {
        if self.comment.is_empty() {
            format!("{KEY_TYPE} {}", self.key)
        } else {
            format!("{KEY_TYPE} {} {}", self.key, self.comment)
        }
    }

    /// The comment, else the key type.
    pub fn label(&self) -> String {
        if self.comment.is_empty() {
            KEY_TYPE.into()
        } else {
            self.comment.clone()
        }
    }

    /// `SHA256:…` as `ssh-keygen -l` prints it.
    pub fn fingerprint(&self) -> String {
        let blob = STANDARD.decode(&self.key).unwrap_or_default();
        format!("SHA256:{}", STANDARD_NO_PAD.encode(Sha256::digest(&blob)))
    }
}

/// What a client's backup folders say about who can open them.
#[derive(Debug, Default)]
pub struct Unlock {
    /// At least one folder had a list that matched its header.
    pub known: bool,
    /// Verified keys of every folder, without duplicates (server keys first).
    pub keys: Vec<Recipient>,
    /// age tags of every header (to check the keys in the settings).
    pub tags: BTreeSet<String>,
}

impl Unlock {
    /// The role of a settings key (`type base64 [comment]`) when a header is
    /// encrypted to it.
    pub fn role_of(&self, line: &str) -> Option<Role> {
        let key = line.split_whitespace().nth(1)?;
        if let Some(r) = self.keys.iter().find(|r| r.key == key) {
            return Some(r.role);
        }
        // In a header without a list (an old client): it opens, role unknown.
        let tag = age_tag(&ed25519_blob(line.split_whitespace().next()?, key)?);
        self.tags.contains(&tag).then_some(Role::Admin)
    }
}

/// Read the lists and headers of a client's backup folders.
pub fn collect(home: &Path, folders: &[String]) -> Unlock {
    let mut u = Unlock::default();
    let mut seen = BTreeSet::new();
    for folder in folders {
        let dir = home.join(folder);
        let Ok(header) = fs::read(dir.join(HEADER)) else {
            continue;
        };
        let tags = header_tags(&header);
        if let Ok(text) = fs::read_to_string(dir.join(FILE)) {
            let verified: Vec<Recipient> = parse(&text)
                .into_iter()
                .filter(|r| {
                    STANDARD
                        .decode(&r.key)
                        .is_ok_and(|b| tags.contains(&age_tag(&b)))
                })
                .collect();
            u.known |= !verified.is_empty();
            for r in verified {
                if seen.insert(r.key.clone()) {
                    u.keys.push(r);
                }
            }
        }
        u.tags.extend(tags);
    }
    u.keys.sort_by_key(|r| r.role != Role::Server);
    u
}

/// The lines of neo-backup.recipients that hold a valid ed25519 key.
pub fn parse(text: &str) -> Vec<Recipient> {
    text.lines()
        .filter_map(|line| {
            let mut w = line.split_whitespace();
            let role = match w.next()? {
                "server" => Role::Server,
                "admin" => Role::Admin,
                _ => return None,
            };
            let kind = w.next()?;
            let key = w.next()?;
            ed25519_blob(kind, key)?;
            Some(Recipient {
                role,
                key: key.to_string(),
                comment: clean_comment(&w.collect::<Vec<_>>().join(" ")),
            })
        })
        .take(MAX_KEYS)
        .collect()
}

/// The wire-format key of an `ssh-ed25519 AAAA…` pair, when it is one.
fn ed25519_blob(kind: &str, key: &str) -> Option<Vec<u8>> {
    if kind != KEY_TYPE {
        return None;
    }
    let blob = STANDARD.decode(key).ok()?;
    // string "ssh-ed25519", string <32 bytes>
    let mut expect = Vec::with_capacity(51);
    expect.extend_from_slice(&(KEY_TYPE.len() as u32).to_be_bytes());
    expect.extend_from_slice(KEY_TYPE.as_bytes());
    expect.extend_from_slice(&32u32.to_be_bytes());
    (blob.len() == expect.len() + 32 && blob.starts_with(&expect)).then_some(blob)
}

/// A comment safe inside an authorized_keys line and a web page.
fn clean_comment(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_graphic() || *c == ' ')
        .filter(|c| !matches!(c, '"' | '\\' | '<' | '>' | '&'))
        .take(MAX_COMMENT)
        .collect::<String>()
        .trim()
        .to_string()
}

/// age's ssh-ed25519 stanza tag: the first 4 bytes of SHA-256 of the
/// wire-format key, base64 without padding.
pub fn age_tag(blob: &[u8]) -> String {
    STANDARD_NO_PAD.encode(&Sha256::digest(blob)[..4])
}

/// Tags of the ssh-ed25519 stanzas of an age header, armored or binary.
pub fn header_tags(file: &[u8]) -> BTreeSet<String> {
    let text = String::from_utf8_lossy(file);
    let binary;
    let header: &str = if text.starts_with("-----BEGIN AGE ENCRYPTED FILE-----") {
        let body: String = text
            .lines()
            .skip(1)
            .take_while(|l| !l.starts_with("-----END"))
            .map(str::trim)
            .collect();
        binary = STANDARD.decode(body).unwrap_or_default();
        // The header is text; the payload after it is not.
        std::str::from_utf8(&binary)
            .unwrap_or_else(|e| std::str::from_utf8(&binary[..e.valid_up_to()]).unwrap_or(""))
    } else {
        &text
    };
    header
        .lines()
        .take_while(|l| !l.starts_with("---"))
        .filter_map(|l| l.strip_prefix("-> ssh-ed25519 "))
        .filter_map(|rest| rest.split_whitespace().next())
        .map(String::from)
        .collect()
}

/// The keys file of a client's account: each admin key that opens its
/// backups, with read-only access. Keys already in the settings keep the
/// access they have there.
pub fn keys_file(
    store: &StoreConfig,
    format: &str,
    gate_prefix: &str,
    unlock: &Unlock,
    settings_keys: &[String],
) -> String {
    let in_settings: BTreeSet<&str> = settings_keys
        .iter()
        .filter_map(|l| l.split_whitespace().nth(1))
        .collect();
    unlock
        .keys
        .iter()
        .filter(|r| r.role == Role::Admin && !in_settings.contains(r.key.as_str()))
        .map(|r| {
            format!(
                "restrict,command=\"{} --read-only {format} {gate_prefix}\" {}\n",
                store.gate,
                r.line()
            )
        })
        .collect()
}

/// Write the keys files of every client into the sshd's third
/// AuthorizedKeysFile folder; remove those of clients without one.
pub fn write_all(files: &BTreeMap<String, String>) -> Result<()> {
    let dir = Path::new(RECIPIENT_KEYS_DIR);
    fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    for (client, body) in files {
        let path = dir.join(format!("backup-{client}"));
        if body.is_empty() {
            let _ = fs::remove_file(&path);
        } else if fs::read_to_string(&path).ok().as_deref() != Some(body.as_str()) {
            // sshd's StrictModes: root's, not writable by others.
            write_atomic(&path, body.as_bytes(), Some(0o644))?;
        }
    }
    if let Ok(entries) = fs::read_dir(dir) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let known = name
                .strip_prefix("backup-")
                .is_some_and(|c| files.contains_key(c));
            if !known {
                let _ = fs::remove_file(e.path());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::fsutil::TempDir;

    // Two real ed25519 public keys.
    const A: &str = "AAAAC3NzaC1lZDI1NTE5AAAAIBktWOqlijyQVF+F5bFUnf2NfR8szcJF6k0dSdfFb7D2";
    const B: &str = "AAAAC3NzaC1lZDI1NTE5AAAAIEM0aHTni0+3khj/7T7yfLbNpdxD6QK3DfrmygMmkCLX";
    const C: &str = "AAAAC3NzaC1lZDI1NTE5AAAAIAQItBeU//8TCFE7j20Ga6U8suh2NQXobaW71+B7PcHe";

    fn tag(key: &str) -> String {
        age_tag(&STANDARD.decode(key).unwrap())
    }

    /// An age header (not a real one: only the stanza lines matter here).
    fn header(keys: &[&str]) -> String {
        let mut h = String::from("age-encryption.org/v1\n");
        for k in keys {
            h.push_str(&format!("-> ssh-ed25519 {} c2FsdA\nYm9keQ\n", tag(k)));
        }
        h.push_str("--- bWFj\n\u{0}\u{1}binary payload");
        h
    }

    fn armored(text: &str) -> String {
        let b = STANDARD.encode(text.as_bytes());
        let mut out = String::from("-----BEGIN AGE ENCRYPTED FILE-----\n");
        for chunk in b.as_bytes().chunks(64) {
            out.push_str(std::str::from_utf8(chunk).unwrap());
            out.push('\n');
        }
        out.push_str("-----END AGE ENCRYPTED FILE-----\n");
        out
    }

    /// `echo secret | age -a -R <A and B>` (age 1.3.2).
    const REAL_HEADER: &str = "-----BEGIN AGE ENCRYPTED FILE-----\nYWdlLWVuY3J5cHRpb24ub3JnL3YxCi0+IHNzaC1lZDI1NTE5IHJseUg1USBpQ0Fr\nc20yaEFEWXJEWm54dmcxWWdYanZycFExeWhub2hTNGRobDljQ1JnCm5TVktwd1FM\naUJqTmIzYWZVSTc0TERrbnp6aXIvRk1TOElWNGsyTjF6b0EKLT4gc3NoLWVkMjU1\nMTkgSVpna2tBICtKME1jZXhheFZEMkdlYk5YOXJtT212TE5TM0JWU21rSTVidThN\ncjJsVW8KWGhjNlBQa1BwT1kzamtCSCtXSDdIbk5MTUpLcldIWTBqbEtyRVJveklU\nNAotLS0gZ1FESytjeGFVRUJJZHkwLzB6Sjd0bmRtMXJYUURBaUkyb0wxSEEwTGtR\nZwrKcVBNO2jOnxc350D4ss4bjXR4c8iD73of/CQfWpe4aGtiLKc3jEw=\n-----END AGE ENCRYPTED FILE-----\n";

    #[test]
    fn tags_of_a_real_age_header() {
        assert_eq!(tag(A), "rlyH5Q");
        assert_eq!(tag(B), "IZgkkA");
        let want: BTreeSet<String> = [tag(A), tag(B)].into();
        assert_eq!(header_tags(REAL_HEADER.as_bytes()), want);
    }

    #[test]
    fn tags_of_armored_and_binary_headers() {
        let h = header(&[A, B]);
        let want: BTreeSet<String> = [tag(A), tag(B)].into();
        assert_eq!(header_tags(h.as_bytes()), want);
        assert_eq!(header_tags(armored(&h).as_bytes()), want);
        assert!(header_tags(b"garbage").is_empty());
    }

    #[test]
    fn parses_only_valid_ed25519_lines() {
        let text = format!(
            "# neo-backup.recipients version=1\n\
             server ssh-ed25519 {A} root@gossau\n\
             admin ssh-ed25519 {B} alexis \"<x>\" mail\n\
             admin ssh-rsa {C} rsa\n\
             admin ssh-ed25519 AAAAB3NzaC1yc2E bad\n\
             other ssh-ed25519 {C} x\n"
        );
        let r = parse(&text);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].role, Role::Server);
        assert_eq!(r[0].comment, "root@gossau");
        assert_eq!(r[1].comment, "alexis x mail");
    }

    #[test]
    fn only_keys_in_the_header_count() {
        let t = TempDir::new("neo-recipients").unwrap();
        let home = t.path();
        fs::create_dir_all(home.join("enc")).unwrap();
        fs::write(home.join("enc").join(HEADER), armored(&header(&[A, B]))).unwrap();
        // C claims to be an admin, but the header is not encrypted to it.
        fs::write(
            home.join("enc").join(FILE),
            format!("server ssh-ed25519 {A} root@x\nadmin ssh-ed25519 {B} alexis\nadmin ssh-ed25519 {C} intruder\n"),
        )
        .unwrap();
        let u = collect(home, &["enc".into()]);
        assert!(u.known);
        let keys: Vec<&str> = u.keys.iter().map(|r| r.key.as_str()).collect();
        assert_eq!(keys, [A, B]);

        assert_eq!(
            u.role_of(&format!("ssh-ed25519 {A} root@x")),
            Some(Role::Server)
        );
        assert_eq!(u.role_of(&format!("ssh-ed25519 {B}")), Some(Role::Admin));
        assert_eq!(u.role_of(&format!("ssh-ed25519 {C} me")), None);

        let store: StoreConfig = serde_json::from_str(
            r#"{"url":"https://b.example","path":"/s","storage":"folder","internet":true,"login":true,"gate":"/gate"}"#,
        )
        .unwrap();
        // The host key and keys in the settings keep their own line.
        let f = keys_file(
            &store,
            "encrypted",
            "-",
            &u,
            &[format!("ssh-ed25519 {A} root@x")],
        );
        assert_eq!(
            f,
            format!("restrict,command=\"/gate --read-only encrypted -\" ssh-ed25519 {B} alexis\n")
        );
        let none = keys_file(
            &store,
            "encrypted",
            "-",
            &u,
            &[format!("ssh-ed25519 {B} other name")],
        );
        assert!(none.is_empty());
    }

    #[test]
    fn no_list_means_unknown() {
        let t = TempDir::new("neo-recipients").unwrap();
        fs::write(t.path().join(HEADER), header(&[A])).unwrap();
        let u = collect(t.path(), &[".".into()]);
        assert!(!u.known && u.keys.is_empty());
        // A settings key the header is encrypted to still shows as opening it.
        assert_eq!(u.role_of(&format!("ssh-ed25519 {A}")), Some(Role::Admin));
    }

    #[test]
    fn fingerprints_like_ssh_keygen() {
        let r = Recipient {
            role: Role::Admin,
            key: A.into(),
            comment: String::new(),
        };
        assert!(r.fingerprint().starts_with("SHA256:"));
        assert_eq!(r.fingerprint().len(), "SHA256:".len() + 43);
    }
}
