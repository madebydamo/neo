//! Turning the requested changes into what the card shows and what Save will do.
use super::*;

/// The setting a copy or move reads, as found when proposing.
#[derive(Debug)]
pub(super) struct Source {
    pub(super) pane: String,
    pub(super) from: SourceRef,
    pub(super) tg: Target,
    /// Saved value (encrypted values or secrets inside), else the evaluated one.
    pub(super) value: Value,
}

impl Source {
    pub(super) fn label(&self, pane: &str) -> String {
        if self.pane == pane {
            self.tg.label()
        } else {
            format!("{} › {}", self.pane, self.tg.label())
        }
    }

    pub(super) fn shown(&self) -> Value {
        redact_value(&self.tg.t, self.tg.secret, &self.value)
    }

    pub(super) fn has_secret(&self) -> bool {
        has_secret(&self.tg.t, self.tg.secret)
    }
}

/// Example of setting one field of an entry, for error messages.
pub(super) fn field_example(path: &str, key: &str) -> String {
    format!(
        r#"{{"op":"set","path":"{path}","key":"{key}","field":"username","value":"…"}} and {{"op":"ask","path":"{path}","key":"{key}","field":"password"}}"#
    )
}

/// Card rows, planned writes and secret inputs of a proposal being built.
#[derive(Default, Debug)]
pub(super) struct Plan {
    pub(super) views: Vec<ChangeView>,
    pub(super) planned: Vec<Planned>,
    pub(super) asks: Vec<AskInput>,
}

/// Plan one change against its option. Errors are plain sentences the
/// assistant can act on.
pub(super) fn plan_change(
    ch: &ChangeRequest,
    index: usize,
    pane: &str,
    o: &OptionSchema,
    path: Vec<String>,
    source: Option<&Source>,
    plan: &mut Plan,
) -> Result<(), String> {
    let tg = resolve(o, path, ch.key.as_deref(), ch.field.as_deref())
        .map_err(|m| format!("{}: {m}", ch.path))?;
    let old = if tg.new_entry {
        "not set".to_string()
    } else if tg.secret {
        if is_set(&tg.current) {
            "set"
        } else {
            "not set"
        }
        .to_string()
    } else {
        display(&redact_value(&tg.t, false, &tg.current))
    };
    let mut view = ChangeView {
        service: pane.to_string(),
        label: tg.label(),
        path: std::iter::once(ch.path.as_str())
            .chain(ch.key.as_deref())
            .chain(ch.field.as_deref())
            .collect::<Vec<_>>()
            .join("."),
        action: String::new(),
        old,
        new: String::new(),
        secret: tg.secret,
        warnings: vec![],
        notes: vec![],
    };
    let cx = ChangeCx {
        ch,
        was_set: !tg.new_entry && is_set(&tg.current),
        tg,
    };

    if ch.from.is_some() && !matches!(ch.op, ChangeOp::Copy | ChangeOp::Move) {
        return Err(cx.at("from only works with op copy or move".into()));
    }
    let mut source_view = None;
    match ch.op {
        ChangeOp::Copy | ChangeOp::Move => {
            source_view = cx.plan_copy(source, pane, &mut view, plan)?;
        }
        ChangeOp::Generate => cx.plan_generate(&mut view, plan)?,
        ChangeOp::Ask => cx.plan_ask(index, pane, &mut view, plan)?,
        ChangeOp::Set => cx.plan_set(&mut view, plan)?,
        ChangeOp::Unset => cx.plan_unset(o, &mut view, plan)?,
        ChangeOp::Add | ChangeOp::Remove => cx.plan_add_remove(&mut view, plan)?,
    }
    plan.views.push(view);
    plan.views.extend(source_view);
    Ok(())
}

/// One change being planned: the request and the setting it resolved to.
pub(super) struct ChangeCx<'a> {
    pub(super) ch: &'a ChangeRequest,
    pub(super) tg: Target,
    pub(super) was_set: bool,
}

impl ChangeCx<'_> {
    pub(super) fn at(&self, m: String) -> String {
        format!("{}: {m}", self.ch.path)
    }

    /// Position inside the option for messages: `tmr.password`.
    pub(super) fn inside(&self, sub: &str) -> String {
        let mut parts: Vec<&str> = Vec::new();
        parts.extend(self.ch.key.as_deref().filter(|k| !k.is_empty()));
        parts.extend(self.ch.field.as_deref().filter(|f| !f.is_empty()));
        if !sub.is_empty() {
            parts.push(sub);
        }
        parts.join(".")
    }

    pub(super) fn never(&self, sub: &str) -> String {
        let what = self.inside(sub);
        let what = if what.is_empty() {
            "this setting".to_string()
        } else {
            what
        };
        self.at(format!(
            "{what} is a secret; never send it. Use op copy or move when another setting already has the value, ask (the user types or generates it on the card) or generate."
        ))
    }

    pub(super) fn set_action(&self, view: &mut ChangeView) {
        view.action = if self.tg.new_entry && self.ch.field.is_none() {
            "add"
        } else if self.was_set {
            "change"
        } else {
            "set"
        }
        .to_string();
    }

    pub(super) fn type_check_value(&self, value: &Value) -> Result<(), String> {
        type_check(&self.tg.t, value).map_err(|e| {
            let w = self.inside("");
            self.at(if w.is_empty() { e } else { format!("{w}: {e}") })
        })
    }

    /// The source's card row when it moves.
    pub(super) fn plan_copy(
        &self,
        source: Option<&Source>,
        pane: &str,
        view: &mut ChangeView,
        plan: &mut Plan,
    ) -> Result<Option<ChangeView>, String> {
        let (ch, tg) = (self.ch, &self.tg);
        let Some(src) = source else {
            return Err(self.at(
                "copy and move need from: the path (and key, field) of the setting to read".into(),
            ));
        };
        let from = src.from.dotted();
        if src.tg.new_entry || !is_set(&src.value) {
            return Err(self.at(format!("{from} has no value to copy")));
        }
        if tg.path.starts_with(&src.tg.path) || src.tg.path.starts_with(&tg.path) {
            return Err(self.at(format!(
                "{from} and the target overlap; name two separate settings or entries"
            )));
        }
        if let Some(sub) = secret_leak(&src.tg.t, src.tg.secret, &tg.t, tg.secret, &src.value) {
            let what = if sub.is_empty() {
                from.clone()
            } else {
                format!("{from}.{sub}")
            };
            return Err(self.at(format!(
                "{what} is a secret and the target is not, so it would become readable; secrets can only go to secret settings"
            )));
        }
        let shown = without_nulls(&src.shown());
        self.type_check_value(&shown)?;
        let moving = ch.op == ChangeOp::Move;
        self.set_action(view);
        view.new = display(&shown);
        view.secret = view.secret || src.has_secret();
        view.notes.push(format!(
            "{} {}",
            if moving { "Moved from" } else { "Copied from" },
            src.label(pane)
        ));
        let dropped = dropped_secrets(&tg.t, tg.secret, &tg.current, &shown);
        view.warnings.extend(
            dropped
                .iter()
                .map(|d| format!("Removes {}", self.inside(d))),
        );
        if tg.secret && self.was_set {
            view.warnings.push("Replaces the current secret".into());
        }
        if strip_null(&tg.t).kind == "attrsOf" {
            let cur = redact_value(&tg.t, tg.secret, &tg.current);
            let (added, changed, removed) = key_diff(&cur, &shown);
            note_entries(view, &added, &changed, &removed);
        }
        plan.planned.push(Planned::Copy {
            from: src.from.clone(),
            from_path: src.tg.path.clone(),
            to: tg.path.clone(),
            remove_source: moving,
            secret: src.has_secret(),
        });
        Ok(moving.then(|| ChangeView {
            service: src.pane.clone(),
            label: src.tg.label(),
            path: from,
            action: "remove".into(),
            old: display(&shown),
            new: format!("moved to {}", view.label),
            secret: src.has_secret(),
            warnings: vec![],
            notes: vec![],
        }))
    }

    pub(super) fn plan_generate(
        &self,
        view: &mut ChangeView,
        plan: &mut Plan,
    ) -> Result<(), String> {
        let tg = &self.tg;
        let h = tg
            .helper
            .as_ref()
            .filter(|h| h.kind == "button" && h.inputs.is_empty() && !h.script.is_empty());
        let Some(h) = h.filter(|_| tg.secret) else {
            return Err(self.at(
                "generate only works on secrets with a Generate button; use op ask so the user enters it".into(),
            ));
        };
        self.set_action(view);
        view.new = "will be generated".into();
        plan.planned.push(Planned::Generate {
            path: tg.path.clone(),
            script: h.script.clone(),
        });
        Ok(())
    }

    pub(super) fn plan_ask(
        &self,
        index: usize,
        pane: &str,
        view: &mut ChangeView,
        plan: &mut Plan,
    ) -> Result<(), String> {
        let tg = &self.tg;
        if !tg.secret {
            return Err(self.at("ask is only for secrets; use set for other values".into()));
        }
        if matches!(
            strip_null(&tg.t).kind.as_str(),
            "attrsOf" | "listOf" | "submodule"
        ) {
            return Err(self.at("ask fills one secret value; name it with key and field".into()));
        }
        let name = ask_input(self.ch.group.as_deref(), index);
        let used = format!("{pane}: {}", tg.label());
        match plan.asks.iter_mut().find(|a| a.name == name) {
            Some(a) => a.used_for.push(used),
            None => plan.asks.push(AskInput {
                name: name.clone(),
                label: tg.label(),
                used_for: vec![used],
            }),
        }
        self.set_action(view);
        view.new = "you enter it below".into();
        plan.planned.push(Planned::Ask {
            path: tg.path.clone(),
            input: name,
        });
        Ok(())
    }

    pub(super) fn plan_set(&self, view: &mut ChangeView, plan: &mut Plan) -> Result<(), String> {
        let (ch, tg) = (self.ch, &self.tg);
        let Some(value) = ch.value.clone() else {
            return Err(self.at("set needs a value (use unset to remove)".into()));
        };
        if tg.secret {
            return Err(self.never(""));
        }
        if let Some(sub) = secret_in(&tg.t, false, &value) {
            return Err(self.never(&sub));
        }
        self.type_check_value(&value)?;
        let dropped = dropped_secrets(&tg.t, false, &tg.current, &value);
        if !dropped.is_empty() {
            let what = dropped
                .iter()
                .map(|d| self.inside(d))
                .collect::<Vec<_>>()
                .join(", ");
            let key = ch.key.as_deref().unwrap_or("<entry>");
            return Err(self.at(format!(
                "this would drop the secret {what}; set the other fields one by one with field instead of writing the whole entry, e.g. {}",
                field_example(&ch.path, key)
            )));
        }
        self.set_action(view);
        view.new = display(&value);
        let t = strip_null(&tg.t);
        let (added, changed, removed) = if t.kind == "attrsOf" {
            key_diff(&tg.current, &value)
        } else {
            Default::default()
        };
        note_entries(view, &added, &changed, &removed);
        if !removed.is_empty() {
            view.action = "change".into();
        }
        let cleanup = t.elem.as_deref().is_some_and(|e| has_secret(e, false));
        plan.planned.push(Planned::Patch(PatchOp::Set {
            path: tg.path.clone(),
            value,
        }));
        // A removed entry's secrets leave the secrets file too.
        if cleanup {
            for k in removed {
                let mut p = tg.path.clone();
                p.push(k);
                plan.planned.push(Planned::Patch(PatchOp::Unset {
                    path: p,
                    secret: true,
                }));
            }
        }
        Ok(())
    }

    pub(super) fn plan_unset(
        &self,
        o: &OptionSchema,
        view: &mut ChangeView,
        plan: &mut Plan,
    ) -> Result<(), String> {
        let (ch, tg) = (self.ch, &self.tg);
        if tg.new_entry {
            let entries = o
                .current
                .as_ref()
                .and_then(|c| c.as_object())
                .map(|m| m.keys().cloned().collect::<Vec<_>>().join(", "))
                .unwrap_or_default();
            return Err(self.at(format!(
                "there is no entry {} (entries: {entries})",
                ch.key.as_deref().unwrap_or_default()
            )));
        }
        if ch.key.is_some() && ch.field.is_none() {
            view.action = "remove".into();
            view.new = "removed".into();
        } else {
            view.action = if tg.nested { "remove" } else { "reset" }.into();
            view.new = if tg.secret {
                "cleared".into()
            } else {
                format!(
                    "default ({})",
                    display(&redact_value(&tg.t, false, &tg.default))
                )
            };
        }
        plan.planned.push(Planned::Patch(PatchOp::Unset {
            secret: has_secret(&tg.t, tg.secret),
            path: tg.path.clone(),
        }));
        Ok(())
    }

    pub(super) fn plan_add_remove(
        &self,
        view: &mut ChangeView,
        plan: &mut Plan,
    ) -> Result<(), String> {
        let (ch, tg) = (self.ch, &self.tg);
        let Some(value) = ch.value.clone() else {
            return Err(self.at("add/remove need a value".into()));
        };
        let t = strip_null(&tg.t);
        if t.kind != "listOf" || tg.secret {
            return Err(self.at("add/remove only work on lists".into()));
        }
        let elem = t.elem.as_deref().cloned().unwrap_or_default();
        if let Some(sub) = secret_in(&elem, false, &value) {
            return Err(self.never(&sub));
        }
        type_check(&elem, &value).map_err(|m| self.at(m))?;
        let mut list = tg.current.as_array().cloned().unwrap_or_default();
        if ch.op == ChangeOp::Add {
            if !list.contains(&value) {
                list.push(value.clone());
            }
        } else {
            list.retain(|v| v != &value);
        }
        let verb = if ch.op == ChangeOp::Add {
            "add"
        } else {
            "remove"
        };
        view.action = verb.into();
        view.new = format!("{verb} {}", display(&redact_value(&elem, false, &value)));
        plan.planned.push(Planned::Patch(PatchOp::Set {
            path: tg.path.clone(),
            value: Value::Array(list),
        }));
        Ok(())
    }
}

/// Card lines for entries an attrsOf gains, changes or loses.
pub(super) fn note_entries(
    view: &mut ChangeView,
    added: &[String],
    changed: &[String],
    removed: &[String],
) {
    view.notes
        .extend(added.iter().map(|k| format!("Adds entry {k}")));
    view.notes
        .extend(changed.iter().map(|k| format!("Changes entry {k}")));
    view.warnings
        .extend(removed.iter().map(|k| format!("Removes entry {k}")));
}

/// Options of a pane, read once per proposal.
pub(super) async fn load_pane_once(
    config: &AppConfig,
    panes: &mut HashMap<(bool, String), Vec<OptionSchema>>,
    is_core: bool,
    pane: &str,
) -> Result<(), String> {
    if let Entry::Vacant(e) = panes.entry((is_core, pane.to_string())) {
        e.insert(load_options(config, is_core, pane).await?);
    }
    Ok(())
}

/// Find what a copy or move reads. The options are already in `panes`.
pub(super) fn find_source(
    panes: &HashMap<(bool, String), Vec<OptionSchema>>,
    doc: &toml_edit::DocumentMut,
    keys: &Tree,
    from: &SourceRef,
) -> Result<Source, String> {
    let (is_core, pane, name, prefix) = locate(&from.path)?;
    let at = |m: String| format!("from {}: {m}", from.path);
    let opts = panes
        .get(&(is_core, pane.clone()))
        .ok_or_else(|| at("could not read settings".into()))?;
    let o = opts
        .iter()
        .find(|o| o.name == name)
        .ok_or_else(|| at("no such setting".into()))?;
    let mut path = prefix;
    path.extend(name.split('.').map(str::to_string));
    let tg = resolve(o, path, from.key.as_deref(), from.field.as_deref()).map_err(at)?;
    let value = saved_value(doc, keys, &tg.path).unwrap_or_else(|| without_nulls(&tg.current));
    Ok(Source {
        pane,
        from: from.clone(),
        tg,
        value,
    })
}

/// Check `req` against the option schemas and store it as a pending proposal.
/// Errors are plain sentences the assistant can act on.
pub async fn create(config: &AppConfig, req: ProposalRequest) -> Result<Proposal, Vec<String>> {
    let mut errors = Vec::new();
    let mut plan = Plan::default();
    if req.changes.is_empty() {
        errors.push("a proposal needs at least one change".to_string());
    }
    let mut panes: HashMap<(bool, String), Vec<OptionSchema>> = HashMap::new();
    // What copies and moves read: only touched when a change has `from`.
    let saved = if req.changes.iter().any(|c| c.from.is_some()) {
        match (
            load_settings_doc(&config.settings_path),
            secret_keys(config),
        ) {
            (Ok(doc), Ok(keys)) => Some((doc, keys)),
            _ => {
                errors.push("could not read the saved settings".to_string());
                None
            }
        }
    } else {
        None
    };

    for (index, ch) in req.changes.iter().enumerate() {
        let from_refused = ch.from.as_ref().is_some_and(|f| refused(&f.path));
        if refused(&ch.path) || from_refused {
            errors.push(format!(
                "{}: the assistant may not change this setting; ask the user to change it in the web UI",
                if from_refused {
                    ch.from.as_ref().map_or("", |f| f.path.as_str())
                } else {
                    ch.path.as_str()
                }
            ));
            continue;
        }
        let (is_core, pane, name, prefix) = match locate(&ch.path) {
            Ok(x) => x,
            Err(e) => {
                errors.push(e);
                continue;
            }
        };
        if let Err(e) = load_pane_once(config, &mut panes, is_core, &pane).await {
            errors.push(format!("{}: could not read settings ({e})", ch.path));
            continue;
        }
        let mut source = None;
        if let (Some(from), Some((doc, keys))) = (ch.from.as_ref(), saved.as_ref()) {
            let found = match locate(&from.path) {
                Ok((from_core, from_pane, ..)) => {
                    load_pane_once(config, &mut panes, from_core, &from_pane)
                        .await
                        .map_err(|e| format!("from {}: could not read settings ({e})", from.path))
                }
                Err(e) => Err(e),
            }
            .and_then(|()| find_source(&panes, doc, keys, from));
            match found {
                Ok(s) => source = Some(s),
                Err(e) => {
                    errors.push(e);
                    continue;
                }
            }
        }
        let Some(opts) = panes.get(&(is_core, pane.clone())) else {
            continue;
        };
        let Some(o) = opts.iter().find(|o| o.name == name) else {
            errors.push(format!("{}: no such setting", ch.path));
            continue;
        };
        let mut path: Vec<String> = prefix;
        path.extend(name.split('.').map(str::to_string));
        if let Err(e) = plan_change(ch, index, &pane, o, path, source.as_ref(), &mut plan) {
            errors.push(e);
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }
    // One setting is filled once; several only list where the value goes.
    let Plan {
        views,
        planned,
        asks,
    } = plan;
    let mut asks = asks;
    for a in asks.iter_mut() {
        if a.used_for.len() < 2 {
            a.used_for.clear();
        }
    }
    let p = Proposal {
        id: new_id(),
        summary: req.summary.trim().to_string(),
        fingerprint: fingerprint(config, &planned),
        views,
        asks,
        planned,
        status: Status::Pending,
        message: String::new(),
        created: now_secs(),
    };
    put(p.clone());
    Ok(p)
}
