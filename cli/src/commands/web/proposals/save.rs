//! Save: turn a proposal's plan into patch operations, reading what copies and
//! moves need at that moment.
use super::*;

/// Patch ops for saving `p` now with the values the user entered on the card
/// (`asked`, by input name): refuses when an asked secret is empty (the card
/// stays pending) or when the touched settings changed since proposing
/// (stale); fills generated secrets and reads what copies and moves carry.
pub async fn ops_for_save(
    config: &AppConfig,
    p: &Proposal,
    asked: &HashMap<String, String>,
) -> Result<Vec<PatchOp>, (Status, String)> {
    for a in &p.asks {
        if asked.get(&a.name).is_none_or(|v| v.trim().is_empty()) {
            return Err((
                Status::Pending,
                format!("Enter {} on the card (or click Generate) first.", a.label),
            ));
        }
    }
    if fingerprint(config, &p.planned) != p.fingerprint {
        return Err((Status::Stale, "These settings changed after the assistant suggested this. Ask it to suggest the change again.".into()));
    }
    let failed = || (Status::Failed, "Could not generate a secret".to_string());
    let mut ops = Vec::new();
    let mut stale = Vec::new();
    let copies: Vec<&Vec<String>> = p
        .planned
        .iter()
        .filter_map(|i| match i {
            Planned::Copy { from_path, .. } => Some(from_path),
            _ => None,
        })
        .collect();
    let saved = if copies.is_empty() {
        None
    } else {
        Some(read_saved(config, &copies)?)
    };
    for item in &p.planned {
        match item {
            Planned::Patch(op) => ops.push(op.clone()),
            Planned::Generate { path, script } => {
                let r = run_helper_script(Path::new(script), "{}", &[])
                    .await
                    .map_err(|_| failed())?;
                if r.exit_code != 0 || r.timed_out {
                    return Err(failed());
                }
                let value = parse_helper_value(&r.stdout).map_err(|_| failed())?;
                ops.push(PatchOp::Set {
                    path: path.clone(),
                    value,
                });
            }
            Planned::Ask { path, input } => {
                let value = asked.get(input).cloned().unwrap_or_default();
                ops.push(PatchOp::Set {
                    path: path.clone(),
                    value: Value::String(value),
                });
            }
            Planned::Copy {
                from,
                from_path,
                to,
                remove_source,
                secret,
            } => {
                let Some(saved) = saved.as_ref() else {
                    continue;
                };
                let value = copied_value(config, saved, from, from_path).await?;
                let leaves = stale_secrets(&saved.keys, to, &value);
                stale.extend(leaves.into_iter().map(|leaf| (leaf, ops.len())));
                ops.push(PatchOp::Set {
                    path: to.clone(),
                    value,
                });
                if *remove_source {
                    ops.push(PatchOp::Unset {
                        path: from_path.clone(),
                        secret: *secret,
                    });
                }
            }
        }
    }
    let unsets = stale_unsets(stale, &ops);
    ops.extend(unsets);
    Ok(ops)
}

/// Unset ops for secrets a copy left out, `(leaf, index of the copy's op)`,
/// except those a later change of the proposal fills again.
pub(super) fn stale_unsets(stale: Vec<(Vec<String>, usize)>, ops: &[PatchOp]) -> Vec<PatchOp> {
    stale
        .into_iter()
        .filter(|(leaf, copy)| {
            !ops[copy + 1..].iter().any(
                |o| matches!(o, PatchOp::Set { path, .. } if leaf.starts_with(path.as_slice())),
            )
        })
        .map(|(path, _)| PatchOp::Unset { path, secret: true })
        .collect()
}

/// Saved settings for the copies of one save.
pub(super) struct Saved {
    pub(super) doc: toml_edit::DocumentMut,
    /// Secrets file, decrypted only when a copy needs a secret from it.
    pub(super) tree: Tree,
    pub(super) keys: Tree,
}

pub(super) fn read_saved(
    config: &AppConfig,
    copies: &[&Vec<String>],
) -> Result<Saved, (Status, String)> {
    let unreadable = || {
        (
            Status::Failed,
            "Could not read the saved settings. Nothing was changed.".to_string(),
        )
    };
    let doc = load_settings_doc(&config.settings_path).map_err(|_| unreadable())?;
    let keys = secret_keys(config).map_err(|e| {
        eprintln!("web: proposal cannot read the secrets file: {e:#}");
        unreadable()
    })?;
    let needs_secret = copies
        .iter()
        .any(|path| secrets::lookup(&keys, path).is_some());
    let tree = match secrets::backend_for(&config.settings_path) {
        Backend::Sops(path) if needs_secret => secrets_store::decrypt(&path).map_err(|e| {
            eprintln!("web: proposal cannot decrypt the secrets: {e:#}");
            unreadable()
        })?,
        _ => keys.clone(),
    };
    Ok(Saved { doc, tree, keys })
}

/// What a copy writes: the saved value, or the evaluated one when the source
/// is on its default. Never an encrypted placeholder.
pub(super) async fn copied_value(
    config: &AppConfig,
    saved: &Saved,
    from: &SourceRef,
    from_path: &[String],
) -> Result<Value, (Status, String)> {
    let dotted = from.dotted();
    let value = match saved_value(&saved.doc, &saved.tree, from_path) {
        Some(v) => Some(v),
        None => {
            let (is_core, pane, name, prefix) =
                locate(&from.path).map_err(|e| (Status::Stale, e))?;
            let opts = load_options(config, is_core, &pane)
                .await
                .map_err(|e| (Status::Failed, e))?;
            opts.iter().find(|o| o.name == name).and_then(|o| {
                let mut path = prefix;
                path.extend(name.split('.').map(str::to_string));
                let tg = resolve(o, path, from.key.as_deref(), from.field.as_deref()).ok()?;
                (!tg.new_entry && is_set(&tg.current)).then_some(tg.current)
            })
        }
    };
    let Some(value) = value.map(|v| without_nulls(&v)) else {
        return Err((
            Status::Stale,
            format!("{dotted} has no value any more. Ask the assistant to suggest this again."),
        ));
    };
    if has_placeholder(&value) {
        return Err((
            Status::Failed,
            format!("Could not read the secrets of {dotted}. Nothing was changed."),
        ));
    }
    Ok(value)
}
