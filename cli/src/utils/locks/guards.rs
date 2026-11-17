//! Unit start guards: `<run>/guard/<unit>` markers that a system-wide drop-in
//! (`AssertPathExists=!/run/neo/guard/%n`, nix/modules/core/operation-locks.nix)
//! turns into "systemd refuses to start this unit". Each marker holds the id
//! of the holder that set it, so a stale-holder cleanup never removes a newer
//! holder's guard.

use std::fs;
use std::path::Path;

use super::{full_unit_name, write_holder_in_place, LockGuard};

impl LockGuard {
    /// Make systemd refuse to start `units` until [`Self::release_unit_guards`]
    /// or drop. Marker files contain this holder's id so a stale-holder cleanup
    /// never removes a newer holder's guard.
    ///
    /// Errors carry one flat message (callers print them with `{e}`).
    pub fn guard_units(&mut self, units: &[String]) -> anyhow::Result<()> {
        let names: Vec<String> = units.iter().map(|u| full_unit_name(u)).collect();
        if let Some(bad) = names.iter().find(|n| !marker_name_ok(n)) {
            anyhow::bail!("invalid unit name for start guard: {bad}");
        }
        let Some((_, file, holder)) = self.holder.as_mut() else {
            anyhow::bail!("no holder file (lock directory not writable)");
        };
        for n in &names {
            if !holder.guarded_units.contains(n) {
                holder.guarded_units.push(n.clone());
            }
        }
        // Record first: a crash after this leaves markers the cleanup knows about.
        write_holder_in_place(file, holder).map_err(|e| anyhow::anyhow!("holder file: {e}"))?;
        fs::create_dir_all(&self.guard_dir)
            .map_err(|e| anyhow::anyhow!("{}: {e}", self.guard_dir.display()))?;
        for n in &names {
            let p = self.guard_dir.join(n);
            fs::write(&p, &holder.id).map_err(|e| anyhow::anyhow!("{}: {e}", p.display()))?;
        }
        Ok(())
    }

    /// Remove all start guards (units may be started again). The scopes stay held.
    pub fn release_unit_guards(&mut self) {
        let Some((_, file, holder)) = self.holder.as_mut() else {
            return;
        };
        if holder.guarded_units.is_empty() {
            return;
        }
        for n in &holder.guarded_units {
            remove_marker_if_owned(&self.guard_dir, n, &holder.id);
        }
        holder.guarded_units.clear();
        let _ = write_holder_in_place(file, holder);
    }

    /// Units currently guarded by this lock.
    pub fn guarded_units(&self) -> Vec<String> {
        self.holder
            .as_ref()
            .map(|(_, _, h)| h.guarded_units.clone())
            .unwrap_or_default()
    }
}

/// A unit guard marker is named exactly like the unit (`%n`).
fn marker_name_ok(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && !name.starts_with('.')
        && !name.contains('/')
        && !name.contains('\0')
}

pub(super) fn remove_marker_if_owned(guard_dir: &Path, unit: &str, holder_id: &str) {
    if !marker_name_ok(unit) {
        return;
    }
    let p = guard_dir.join(unit);
    if fs::read_to_string(&p).is_ok_and(|owner| owner.trim() == holder_id) {
        let _ = fs::remove_file(&p);
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{info, tmp_root};
    use super::super::*;
    use std::fs;

    #[test]
    fn unit_guards_created_and_removed() {
        let root = tmp_root("guard");
        let m = LockManager::new(&root);
        let mut g = m
            .try_acquire(
                &LockSpec::service_data("a", ["docker-a"]),
                &info("restore a"),
            )
            .unwrap();
        g.guard_units(&["docker-a".to_string(), "neo-a.target".to_string()])
            .unwrap();
        assert!(root.join("guard/docker-a.service").exists());
        assert!(root.join("guard/neo-a.target").exists());
        assert_eq!(m.holders()[0].guarded_units.len(), 2);
        g.release_unit_guards();
        assert!(!root.join("guard/docker-a.service").exists());
        g.guard_units(&["docker-a".to_string()]).unwrap();
        drop(g);
        assert!(!root.join("guard/docker-a.service").exists());
        assert!(g_invalid(&m).is_err());
    }

    fn g_invalid(m: &LockManager) -> anyhow::Result<()> {
        let mut g = m.try_acquire(&LockSpec::unit("x"), &info("x")).unwrap();
        g.guard_units(&["../evil".to_string()])
    }

    #[test]
    fn stale_holder_cleaned_with_its_guards_only() {
        let root = tmp_root("stale");
        let m = LockManager::new(&root);
        fs::create_dir_all(root.join("locks")).unwrap();
        fs::create_dir_all(root.join("guard")).unwrap();
        // A holder file nobody has locked = its process died.
        let dead = Holder {
            id: "999999-0".into(),
            scopes: vec![HeldScope {
                scope: "unit/docker-a.service".into(),
                mode: LockMode::Exclusive,
            }],
            kind: "restore".into(),
            label: "Restore".into(),
            op_id: None,
            pid: 999_999,
            started_at: 0,
            guarded_units: vec!["docker-a.service".into(), "docker-b.service".into()],
        };
        fs::write(
            root.join("locks/999999-0.holder"),
            serde_json::to_string(&dead).unwrap(),
        )
        .unwrap();
        fs::write(root.join("guard/docker-a.service"), "999999-0").unwrap();
        // Same unit name guarded by someone else: must survive.
        fs::write(root.join("guard/docker-b.service"), "other-1").unwrap();
        assert!(m.holders().is_empty());
        assert!(!root.join("locks/999999-0.holder").exists());
        assert!(!root.join("guard/docker-a.service").exists());
        assert!(root.join("guard/docker-b.service").exists());
    }
}
