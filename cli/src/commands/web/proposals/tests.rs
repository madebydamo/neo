use serde_json::Value;

use super::check::{
    fingerprint_of, locate, refused, resolve, saved_value, stale_secrets, type_check,
};
use super::plan::{plan_change, Plan, Source};
use super::save::stale_unsets;
use super::store::{claim, get, interrupted, live, put, set_status, TTL};
use super::types::{ChangeRequest, Planned, Proposal, SourceRef, Status};
use crate::commands::web::redact::tests::accounts;
use crate::commands::web::settings::edit::PatchOp;
use crate::commands::web::types::OptionType;
use crate::utils::command::now_secs;
use crate::utils::secrets_yaml;
use serde_json::json;

fn t(v: serde_json::Value) -> OptionType {
    serde_json::from_value(v).unwrap()
}

fn p(s: &str) -> Vec<String> {
    s.split('.').map(str::to_string).collect()
}

fn change(v: Value) -> ChangeRequest {
    serde_json::from_value(v).unwrap()
}

/// Plan changes on the `accounts` option of service `x`.
fn plan(changes: Vec<Value>) -> Result<Plan, String> {
    let o = accounts();
    let mut plan = Plan::default();
    for (i, c) in changes.into_iter().enumerate() {
        plan_change(
            &change(c),
            i,
            "x",
            &o,
            p("services.x.accounts"),
            None,
            &mut plan,
        )?;
    }
    Ok(plan)
}

#[test]
fn locate_maps_option_paths_to_panes() {
    let (core, pane, name, prefix) = locate("services.pihole.localIP").unwrap();
    assert!(!core);
    assert_eq!((pane.as_str(), name.as_str()), ("pihole", "localIP"));
    assert_eq!(prefix, vec!["services", "pihole"]);
    let (core, pane, name, _) = locate("core.volumes.appdata").unwrap();
    assert!(core);
    assert_eq!((pane.as_str(), name.as_str()), ("core", "volumes.appdata"));
    assert!(locate("pihole.localIP").is_err());
    assert!(locate("services.pihole").is_err());
}

#[test]
fn refused_paths() {
    assert!(refused("core.plugins"));
    assert!(refused("core.ssh.authorizedKeys"));
    assert!(refused("services.tinyauth.enabled"));
    assert!(!refused("services.tinyauth.sessionExpiry"));
    assert!(!refused("services.pihole.enabled"));
}

#[test]
fn type_check_shapes() {
    assert!(type_check(&t(json!({"kind":"bool"})), &json!(true)).is_ok());
    assert!(type_check(&t(json!({"kind":"bool"})), &json!("yes")).is_err());
    assert!(type_check(&t(json!({"kind":"port"})), &json!(70000)).is_err());
    assert!(type_check(&t(json!({"kind":"int"})), &json!(1.5)).is_err());
    let e = t(json!({"kind":"enum","values":["switch","boot"]}));
    assert!(type_check(&e, &json!("boot")).is_ok());
    assert!(type_check(&e, &json!("reboot")).is_err());
    let n = t(json!({"kind":"nullOr","elem":{"kind":"str"}}));
    assert!(type_check(&n, &json!(null)).is_ok());
    assert!(type_check(&n, &json!(3)).is_err());
    let l = t(json!({"kind":"listOf","elem":{"kind":"enum","values":["local","web"]}}));
    assert!(type_check(&l, &json!(["local"])).is_ok());
    assert!(type_check(&l, &json!(["moon"])).is_err());
    // Flattened nested submodule fields accept the nested object.
    let a = accounts().r#type;
    assert!(type_check(&a, &json!({"t": {"oauth": {"clientId": "c"}}})).is_ok());
    assert!(type_check(&a, &json!({"t": {"nope": 1}})).is_err());
}

#[test]
fn add_an_entry_field_by_field() {
    let Plan { views, planned, asks } = plan(vec![
        json!({"op":"set","path":"services.x.accounts","key":"tmr","field":"username","value":"tmr"}),
        json!({"op":"ask","path":"services.x.accounts","key":"tmr","field":"password","group":"tmr-password"}),
    ])
    .unwrap();
    assert_eq!(views[0].label, "Accounts › tmr › Username");
    assert_eq!(views[0].old, "not set");
    assert_eq!(views[0].action, "set");
    assert_eq!(views[1].label, "Accounts › tmr › Password");
    assert_eq!(views[1].new, "you enter it below");
    assert!(views[1].secret);
    assert_eq!(asks.len(), 1);
    assert_eq!(asks[0].name, "ask_g_tmr-password");
    match &planned[0] {
        Planned::Patch(PatchOp::Set { path, value }) => {
            assert_eq!(path, &p("services.x.accounts.tmr.username"));
            assert_eq!(value, &json!("tmr"));
        }
        other => panic!("{other:?}"),
    }
    match &planned[1] {
        Planned::Ask { path, input } => {
            assert_eq!(path, &p("services.x.accounts.tmr.password"));
            assert_eq!(input, "ask_g_tmr-password");
        }
        other => panic!("{other:?}"),
    }
    // The persisted plan holds no value for the asked secret.
    let json = serde_json::to_string(&planned).unwrap();
    assert!(!json.contains("value\":\"pw"));
}

#[test]
fn secrets_are_refused_everywhere() {
    // Whole map with a plaintext password (the incident's first try).
    let e = plan(vec![
        json!({"op":"set","path":"services.x.accounts","value":{
        "tinyauth": {"username":"admin"}, "tmr": {"username":"tmr","password":"pw"}}}),
    ])
    .unwrap_err();
    assert!(e.contains("tmr.password is a secret; never send it"), "{e}");
    // One entry with a password, and the field itself.
    let e = plan(vec![json!({"op":"set","path":"services.x.accounts","key":"tmr","value":{"username":"tmr","password":"pw"}})]).unwrap_err();
    assert!(e.contains("tmr.password is a secret"), "{e}");
    let e = plan(vec![json!({"op":"set","path":"services.x.accounts","key":"tmr","field":"password","value":"pw"})]).unwrap_err();
    assert!(e.contains("tmr.password is a secret"), "{e}");
    // ask only on secrets.
    assert!(plan(vec![
        json!({"op":"ask","path":"services.x.accounts","key":"tmr","field":"username"})
    ])
    .is_err());
}

#[test]
fn whole_entry_set_must_not_drop_secrets() {
    let e = plan(vec![json!({"op":"set","path":"services.x.accounts","key":"tinyauth","value":{"username":"root"}})]).unwrap_err();
    assert!(e.contains("drop the secret"), "{e}");
    assert!(e.contains("\"field\":\"username\""), "{e}");
    // A new entry without its password is fine.
    let Plan { views, .. } = plan(vec![
        json!({"op":"set","path":"services.x.accounts","key":"tmr","value":{"username":"tmr"}}),
    ])
    .unwrap();
    assert_eq!(views[0].action, "add");
    assert_eq!(views[0].label, "Accounts › tmr");
    assert_eq!(views[0].old, "not set");
    assert_eq!(views[0].new, "username: tmr");
}

#[test]
fn keyed_preview_shows_the_entry_not_the_map() {
    let Plan { views, .. } = plan(vec![json!({"op":"set","path":"services.x.accounts","key":"tinyauth","field":"notes","value":"main login"})]).unwrap();
    assert_eq!(views[0].action, "set");
    assert_eq!(views[0].old, "empty");
    let Plan { views, .. } = plan(vec![json!({"op":"set","path":"services.x.accounts","key":"tinyauth","field":"username","value":"root"})]).unwrap();
    assert_eq!(views[0].action, "change");
    assert_eq!(views[0].old, "admin");
    assert!(!views[0].old.contains("thispassword"));
}

#[test]
fn whole_map_set_lists_removed_entries() {
    let Plan { views, planned, .. } = plan(vec![
        json!({"op":"set","path":"services.x.accounts","value":{"tmr":{"username":"tmr"}}}),
    ])
    .unwrap();
    assert_eq!(views[0].warnings, vec!["Removes entry tinyauth"]);
    assert_eq!(views[0].notes, vec!["Adds entry tmr"]);
    assert!(!views[0].old.contains("thispassword"), "{}", views[0].old);
    // The set first, then the removed entry's secrets leave the secrets file.
    assert!(
        matches!(&planned[0], Planned::Patch(PatchOp::Set { path, .. }) if path == &p("services.x.accounts"))
    );
    assert!(
        matches!(&planned[1], Planned::Patch(PatchOp::Unset { path, secret: true }) if path == &p("services.x.accounts.tinyauth"))
    );
}

#[test]
fn generate_and_unset_nested() {
    let Plan { views, planned, .. } = plan(vec![
        json!({"op":"generate","path":"services.x.accounts","key":"tmr","field":"password"}),
        json!({"op":"unset","path":"services.x.accounts","key":"tinyauth"}),
    ])
    .unwrap();
    assert_eq!(views[0].new, "will be generated");
    assert!(
        matches!(&planned[0], Planned::Generate { path, script } if path == &p("services.x.accounts.tmr.password") && script == "/nix/store/x-gen")
    );
    assert_eq!(views[1].action, "remove");
    assert!(matches!(
        &planned[1],
        Planned::Patch(PatchOp::Unset { secret: true, .. })
    ));
    // No Generate button on the TOTP secret; unknown entries cannot be removed.
    assert!(plan(vec![
        json!({"op":"generate","path":"services.x.accounts","key":"tmr","field":"totpSecret"})
    ])
    .is_err());
    assert!(plan(vec![
        json!({"op":"unset","path":"services.x.accounts","key":"nope"})
    ])
    .is_err());
    assert!(plan(vec![
        json!({"op":"set","path":"services.x.accounts","key":"tmr","field":"nope","value":1})
    ])
    .is_err());
}

/// Plan a copy/move on `accounts` of service `x` from `accounts` of `y`
/// (`entry` there), as read from the saved settings.
fn plan_from(c: Value, pane: &str, entry: &str) -> Result<Plan, String> {
    let o = accounts();
    let prefix = format!("services.{pane}.accounts");
    let tg = resolve(&o, p(&prefix), Some(entry), None)?;
    let value = tg.current.clone();
    let src = Source {
        pane: pane.to_string(),
        from: SourceRef {
            path: prefix,
            key: Some(entry.to_string()),
            field: None,
        },
        tg,
        value,
    };
    let mut plan = Plan::default();
    plan_change(
        &change(c),
        0,
        "x",
        &o,
        p("services.x.accounts"),
        Some(&src),
        &mut plan,
    )?;
    Ok(plan)
}

#[test]
fn copy_an_entry_keeps_secrets_off_the_card() {
    let c = json!({"op":"copy","path":"services.x.accounts","key":"biel","from":{"path":"services.y.accounts","key":"tinyauth"}});
    let Plan { views, planned, .. } = plan_from(c, "y", "tinyauth").unwrap();
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].label, "Accounts › biel");
    assert_eq!(views[0].action, "add");
    assert!(views[0].secret);
    assert!(views[0].new.contains("username: admin"), "{}", views[0].new);
    assert!(views[0].new.contains("password: set"), "{}", views[0].new);
    assert_eq!(views[0].notes, vec!["Copied from y › Accounts › tinyauth"]);
    match &planned[0] {
        Planned::Copy {
            from_path,
            to,
            remove_source,
            secret,
            ..
        } => {
            assert_eq!(from_path, &p("services.y.accounts.tinyauth"));
            assert_eq!(to, &p("services.x.accounts.biel"));
            assert!(!remove_source);
            assert!(secret);
        }
        other => panic!("{other:?}"),
    }
    // Nothing the assistant gets back or the store keeps has the value.
    let text = serde_json::to_string(&(&views, &planned)).unwrap();
    assert!(!text.contains("thispassword"), "{text}");
}

#[test]
fn rename_is_a_move_inside_one_setting() {
    let c = json!({"op":"move","path":"services.x.accounts","key":"main","from":{"path":"services.x.accounts","key":"tinyauth"}});
    let Plan { views, planned, .. } = plan_from(c, "x", "tinyauth").unwrap();
    assert_eq!(views[0].action, "add");
    assert_eq!(views[0].notes, vec!["Moved from Accounts › tinyauth"]);
    assert_eq!(views[1].action, "remove");
    assert_eq!(views[1].label, "Accounts › tinyauth");
    assert_eq!(views[1].new, "moved to Accounts › main");
    assert!(matches!(
        &planned[0],
        Planned::Copy {
            remove_source: true,
            ..
        }
    ));
}

#[test]
fn copy_refusals() {
    // Onto itself or inside itself.
    let c = json!({"op":"copy","path":"services.x.accounts","key":"tinyauth","from":{"path":"services.x.accounts","key":"tinyauth"}});
    assert!(plan_from(c, "x", "tinyauth")
        .unwrap_err()
        .contains("overlap"));
    // Nothing there.
    let c = json!({"op":"copy","path":"services.x.accounts","key":"b","from":{"path":"services.y.accounts","key":"nope"}});
    assert!(plan_from(c, "y", "nope").unwrap_err().contains("no value"));
    // Without a source, and from on another op.
    let e = plan(vec![
        json!({"op":"copy","path":"services.x.accounts","key":"b"}),
    ])
    .unwrap_err();
    assert!(e.contains("need from"), "{e}");
    let e = plan(vec![json!({"op":"set","path":"services.x.accounts","key":"b","field":"notes","value":"n","from":{"path":"services.y.accounts"}})]).unwrap_err();
    assert!(e.contains("from only works"), "{e}");
    // A whole entry into a text field: the password would become readable.
    let c = json!({"op":"copy","path":"services.x.accounts","key":"b","field":"notes","from":{"path":"services.y.accounts","key":"tinyauth"}});
    let e = plan_from(c, "y", "tinyauth").unwrap_err();
    assert!(e.contains("is a secret and the target is not"), "{e}");
    assert!(!e.contains("thispassword"), "{e}");
}

#[test]
fn a_secret_field_can_be_copied_on_its_own() {
    let o = accounts();
    let tg = resolve(
        &o,
        p("services.y.accounts"),
        Some("tinyauth"),
        Some("password"),
    )
    .unwrap();
    let src = Source {
        pane: "y".into(),
        from: SourceRef {
            path: "services.y.accounts".into(),
            key: Some("tinyauth".into()),
            field: Some("password".into()),
        },
        value: tg.current.clone(),
        tg,
    };
    let mut plan = Plan::default();
    let to = |field: &str| {
        change(
            json!({"op":"copy","path":"services.x.accounts","key":"b","field":field,"from":{"path":"services.y.accounts","key":"tinyauth","field":"password"}}),
        )
    };
    plan_change(
        &to("password"),
        0,
        "x",
        &o,
        p("services.x.accounts"),
        Some(&src),
        &mut plan,
    )
    .unwrap();
    assert_eq!(plan.views[0].new, "set");
    assert!(plan.views[0].secret);
    // Into a plain field it would be readable with neo_settings_get.
    let e = plan_change(
        &to("notes"),
        1,
        "x",
        &o,
        p("services.x.accounts"),
        Some(&src),
        &mut plan,
    )
    .unwrap_err();
    assert!(e.contains("is a secret and the target is not"), "{e}");
}

#[test]
fn saved_value_lays_secrets_over_settings() {
    let doc: toml_edit::DocumentMut =
        "[services.y.accounts.tinyauth]\nusername = \"admin\"\nport = 8\n"
            .parse()
            .unwrap();
    let tree = secrets_yaml::parse("services:\n  y:\n    accounts:\n      tinyauth:\n        password: \"pw\"\n      only:\n        password: \"s\"\n").unwrap();
    let v = saved_value(&doc, &tree, &p("services.y.accounts.tinyauth")).unwrap();
    assert_eq!(v, json!({"username": "admin", "port": 8, "password": "pw"}));
    // Only in the secrets file, only in settings, and nowhere.
    assert_eq!(
        saved_value(&doc, &tree, &p("services.y.accounts.only.password")),
        Some(json!("s"))
    );
    assert_eq!(
        saved_value(&doc, &tree, &p("services.y.accounts.tinyauth.username")),
        Some(json!("admin"))
    );
    assert_eq!(saved_value(&doc, &tree, &p("services.y.nope")), None);
}

#[test]
fn stored_proposals_never_hold_inline_secrets() {
    // Without a secrets file the source of a copy holds the plain value.
    let doc: toml_edit::DocumentMut = "[services.y.accounts.a]\npassword = \"thispassword\"\n"
        .parse()
        .unwrap();
    let copy = Planned::Copy {
        from: SourceRef {
            path: "services.y.accounts".into(),
            key: Some("a".into()),
            field: None,
        },
        from_path: p("services.y.accounts.a"),
        to: p("services.x.accounts.a"),
        remove_source: false,
        secret: true,
    };
    let planned = vec![copy];
    let fp = fingerprint_of(&doc, &planned);
    assert!(!fp.contains("thispassword"), "{fp}");
    assert!(!serde_json::to_string(&planned)
        .unwrap()
        .contains("thispassword"));
    // It still notices a change, also of the source only.
    assert_eq!(fp, fingerprint_of(&doc, &planned));
    let changed: toml_edit::DocumentMut = "[services.y.accounts.a]\npassword = \"other\"\n"
        .parse()
        .unwrap();
    assert_ne!(fp, fingerprint_of(&changed, &planned));
}

#[test]
fn stale_secrets_stay_cleared_unless_refilled_later() {
    let set = |path: &str| PatchOp::Set {
        path: p(path),
        value: json!("v"),
    };
    let leaf = p("services.x.a.totpSecret");
    // The copy's own write covers the leaf and must not keep it alive.
    let ops = vec![set("services.x.a")];
    assert_eq!(
        stale_unsets(vec![(leaf.clone(), 0)], &ops),
        vec![PatchOp::Unset {
            path: leaf.clone(),
            secret: true
        }]
    );
    // A later ask or generate for it keeps the new value.
    let ops = vec![set("services.x.a"), set("services.x.a.totpSecret")];
    assert!(stale_unsets(vec![(leaf.clone(), 0)], &ops).is_empty());
    // An earlier write is replaced by the copy anyway.
    let ops = vec![set("services.x.a.totpSecret"), set("services.x.a")];
    assert_eq!(stale_unsets(vec![(leaf, 1)], &ops).len(), 1);
}

#[test]
fn copying_over_an_entry_clears_secrets_the_source_lacks() {
    use crate::commands::web::settings::edit::apply_ops;
    let keys = secrets_yaml::parse(
        "services:\n  x:\n    a:\n      password: \"old\"\n      totpSecret: \"old\"\n",
    )
    .unwrap();
    let to = p("services.x.a");
    let value = json!({"username": "u", "password": "new"});
    let stale = stale_secrets(&keys, &to, &value);
    assert_eq!(stale, vec![p("services.x.a.totpSecret")]);
    // End to end on the document and secrets tree, like save_patch_inner.
    let mut doc: toml_edit::DocumentMut = "[services.y.a]\nusername = \"u\"\n".parse().unwrap();
    let mut tree = keys.clone();
    let patterns = vec![p("services.*.a.password"), p("services.*.a.totpSecret")];
    let mut ops = vec![PatchOp::Set {
        path: to.clone(),
        value,
    }];
    ops.extend(stale.iter().map(|s| PatchOp::Unset {
        path: s.clone(),
        secret: true,
    }));
    apply_ops(&mut doc, &ops).unwrap();
    secrets_yaml::split_into(&mut doc, &mut tree, &patterns, None);
    for s in &stale {
        secrets_yaml::split_into(&mut doc, &mut tree, &patterns, Some(s));
    }
    assert!(!doc.to_string().contains("new"));
    let yaml = secrets_yaml::emit(&tree);
    assert!(
        yaml.contains("new") && !yaml.contains("totpSecret"),
        "{yaml}"
    );
}

#[test]
fn proposals_round_trip_without_secrets() {
    let Plan {
        views,
        planned,
        asks,
    } = plan(vec![
        json!({"op":"ask","path":"services.x.accounts","key":"a","field":"password","group":"g"}),
        json!({"op":"ask","path":"services.x.accounts","key":"b","field":"password","group":"g"}),
    ])
    .unwrap();
    assert_eq!(asks.len(), 1);
    assert_eq!(asks[0].used_for.len(), 2);
    let pr = Proposal {
        id: "0123456789abcdef".into(),
        summary: "s".into(),
        views,
        asks,
        planned,
        fingerprint: String::new(),
        status: Status::Pending,
        message: String::new(),
        created: now_secs(),
    };
    let text = serde_json::to_string(&vec![&pr]).unwrap();
    let back: Vec<Proposal> = serde_json::from_str(&text).unwrap();
    assert_eq!(back[0].planned.len(), 2);
    assert!(live(&back[0]));
    let old = Proposal {
        created: now_secs() - TTL - 1,
        ..back[0].clone()
    };
    assert!(!live(&old));
}

fn stored(id: &str, status: Status) -> Proposal {
    let p = Proposal {
        id: id.into(),
        summary: "s".into(),
        views: vec![],
        asks: vec![],
        planned: vec![],
        fingerprint: String::new(),
        status,
        message: String::new(),
        created: now_secs(),
    };
    put(p.clone());
    p
}

#[test]
fn only_one_concurrent_claim_wins() {
    let id = "c1aim0000000beef";
    stored(id, Status::Pending);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
    let wins: usize = (0..8)
        .map(|_| {
            let b = barrier.clone();
            std::thread::spawn(move || {
                b.wait();
                claim(id).is_some()
            })
        })
        .collect::<Vec<_>>()
        .into_iter()
        .map(|h| h.join().unwrap() as usize)
        .sum();
    assert_eq!(wins, 1);
    assert_eq!(get(id).unwrap().status, Status::Saving);
    // A failed save hands it back; it can be claimed again.
    set_status(id, Status::Failed, "nope");
    assert_eq!(claim(id).unwrap().status, Status::Failed);
    set_status(id, Status::Saved, "ok");
    assert!(claim(id).is_none());
}

#[test]
fn interrupted_save_becomes_actionable() {
    let mut p = stored("1nterrupted00000", Status::Saving);
    interrupted(&mut p);
    assert_eq!(p.status, Status::Failed);
    assert!(p.status.is_actionable());
    let mut done = Proposal {
        status: Status::Saved,
        ..p
    };
    interrupted(&mut done);
    assert_eq!(done.status, Status::Saved);
}
