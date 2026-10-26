// Presentation pass over extracted options: display labels, choice labels,
// visibleWhen resolution and the General / ui.group section split.
use crate::commands::web::types::{ChoiceItem, OptionSchema, OptionSection, OptionType};

/// "publicPaths" → "Public paths", "extra_args" → "Extra args", "URL" stays "URL".
pub fn humanize(segment: &str) -> String {
    let mut words: Vec<String> = Vec::new();
    let mut cur = String::new();
    let chars: Vec<char> = segment.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if c == '_' || c == '-' || c == ' ' {
            if !cur.is_empty() {
                words.push(std::mem::take(&mut cur));
            }
            continue;
        }
        let prev = if i > 0 { Some(chars[i - 1]) } else { None };
        let next = chars.get(i + 1).copied();
        // Split camelCase and the end of an acronym run ("sshKey", "URLPath").
        let boundary = c.is_uppercase()
            && prev.is_some_and(|p| {
                p.is_lowercase()
                    || p.is_ascii_digit()
                    || (p.is_uppercase() && next.is_some_and(|n| n.is_lowercase()))
            });
        if boundary && !cur.is_empty() {
            words.push(std::mem::take(&mut cur));
        }
        cur.push(c);
    }
    if !cur.is_empty() {
        words.push(cur);
    }
    words
        .iter()
        .enumerate()
        .map(|(i, w)| {
            let acronym = w.len() > 1 && w.chars().all(|c| !c.is_lowercase());
            if acronym {
                w.clone()
            } else if i == 0 {
                let mut cs = w.chars();
                match cs.next() {
                    Some(f) => f.to_uppercase().collect::<String>() + &cs.as_str().to_lowercase(),
                    None => String::new(),
                }
            } else {
                w.to_lowercase()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn value_label(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn attach_choices(t: &mut OptionType, labels: Option<&std::collections::BTreeMap<String, String>>) {
    if let Some(values) = t.values.as_ref() {
        t.choices = Some(
            values
                .iter()
                .map(|v| {
                    let raw = value_label(v);
                    let label = labels.and_then(|m| m.get(&raw).cloned()).unwrap_or(raw);
                    ChoiceItem {
                        value: v.clone(),
                        label,
                    }
                })
                .collect(),
        );
    }
    if let Some(elem) = t.elem.as_mut() {
        attach_choices(elem, labels);
    }
    if let Some(fields) = t.fields.as_mut() {
        label_options(fields);
    }
}

/// Fill `label`, `crumb`, `visibleWhen` and `type.choices` (recursing into submodule fields).
pub fn label_options(opts: &mut [OptionSchema]) {
    for o in opts.iter_mut() {
        let (parent, last) = match o.name.rsplit_once('.') {
            Some((p, l)) => (Some(p), l),
            None => (None, o.name.as_str()),
        };
        let ui_label =
            o.ui.as_ref()
                .and_then(|u| u.label.clone())
                .filter(|l| !l.is_empty());
        o.label = ui_label.clone().unwrap_or_else(|| humanize(last));
        // Parent context only when the label is derived (ui.label reads on its own).
        o.crumb = match (parent, &ui_label) {
            (Some(p), None) => p.split('.').map(humanize).collect::<Vec<_>>().join(" › "),
            _ => String::new(),
        };
        o.visible_when =
            o.ui.as_ref()
                .and_then(|u| u.visible_when.as_deref())
                .filter(|s| !s.is_empty())
                .map(|sib| match parent {
                    Some(p) => format!("{p}.{sib}"),
                    None => sib.to_string(),
                })
                .unwrap_or_default();
        let labels = o.ui.as_ref().and_then(|u| u.choice_labels.clone());
        attach_choices(&mut o.r#type, labels.as_ref());
    }
}

/// Drop crumb segments that repeat the section title ("Git" inside Git).
/// Section icons that are SVGs render tinted (CSS mask) so they follow the theme.
pub fn is_svg(url: &str) -> bool {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    path.to_ascii_lowercase().ends_with(".svg")
}

fn crumb_without_group(crumb: &str, group_label: &str) -> String {
    if crumb.is_empty() || group_label.is_empty() {
        return crumb.to_string();
    }
    crumb
        .split(" › ")
        .filter(|part| !part.eq_ignore_ascii_case(group_label))
        .collect::<Vec<_>>()
        .join(" › ")
}

/// General (ungrouped, always first) + `ui.group` sections ordered by rank, then
/// first appearance. Option order inside a section follows the extract order.
pub fn build_sections(opts: &[OptionSchema]) -> Vec<OptionSection> {
    let mut general = OptionSection {
        id: "general".to_string(),
        label: "General".to_string(),
        collapsible: false,
        ..Default::default()
    };
    let mut groups: Vec<(i64, OptionSection)> = Vec::new();
    for o in opts {
        let summary = o.ui.as_ref().is_some_and(|u| u.summary);
        // Clone so the section title can be borrowed while the option is placed.
        let group = o.ui.as_ref().and_then(|u| u.group.clone());
        let mut placed = o.clone();
        let section = match group {
            None => &mut general,
            Some(g) => {
                let label = if g.label.is_empty() {
                    humanize(&g.id)
                } else {
                    g.label.clone()
                };
                placed.crumb = crumb_without_group(&placed.crumb, &label);
                let idx = match groups.iter().position(|(_, s)| s.id == g.id) {
                    Some(i) => i,
                    None => {
                        groups.push((
                            g.rank,
                            OptionSection {
                                id: g.id.clone(),
                                label,
                                description: g.description.clone(),
                                icon: g.icon.clone(),
                                collapsible: true,
                                ..Default::default()
                            },
                        ));
                        groups.len() - 1
                    }
                };
                &mut groups[idx].1
            }
        };
        if summary {
            section.summary.push(placed.name.clone());
        }
        section.options.push(placed);
    }
    // Stable sort keeps first-appearance order among equal ranks.
    groups.sort_by_key(|(rank, _)| *rank);
    let mut out = Vec::with_capacity(groups.len() + 1);
    if !general.options.is_empty() {
        out.push(general);
    }
    out.extend(groups.into_iter().map(|(_, s)| s));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn svg_icons_are_tinted() {
        assert!(is_svg("/service-icons/abc-shield.svg"));
        assert!(is_svg("https://cdn.example/Icon.SVG?v=2"));
        assert!(!is_svg("/service-icons/abc-logo.png"));
    }

    #[test]
    fn humanize_camel_snake_and_acronyms() {
        assert_eq!(humanize("publicPaths"), "Public paths");
        assert_eq!(humanize("additionalMountPoints"), "Additional mount points");
        assert_eq!(humanize("extra_args"), "Extra args");
        assert_eq!(humanize("sshKey"), "Ssh key");
        assert_eq!(humanize("URLPath"), "URL path");
        assert_eq!(
            humanize("immich-machine-learning"),
            "Immich machine learning"
        );
        assert_eq!(humanize("enabled"), "Enabled");
        assert_eq!(humanize(""), "");
    }

    fn opt(v: serde_json::Value) -> OptionSchema {
        serde_json::from_value(v).expect("option schema")
    }

    #[test]
    fn labels_crumbs_visible_when_and_choices() {
        let mut opts = vec![
            opt(serde_json::json!({
                "name": "auth.publicPaths",
                "type": {"kind": "listOf", "elem": {"kind": "str"}},
                "typeLabel": "list of str",
                "ui": {"label": "Public paths", "visibleWhen": "enabled"}
            })),
            opt(serde_json::json!({
                "name": "containers.filebrowser",
                "type": {"kind": "str"},
                "typeLabel": "str"
            })),
            opt(serde_json::json!({
                "name": "ingress",
                "type": {"kind": "listOf", "elem": {"kind": "enum", "values": ["local", "web"]}, "values": ["local", "web"]},
                "typeLabel": "list of enum",
                "ui": {"choiceLabels": {"web": "Internet"}}
            })),
        ];
        label_options(&mut opts);
        assert_eq!(opts[0].label, "Public paths");
        assert_eq!(opts[0].crumb, "");
        assert_eq!(opts[0].visible_when, "auth.enabled");
        assert_eq!(opts[1].label, "Filebrowser");
        assert_eq!(opts[1].crumb, "Containers");
        let choices = opts[2].r#type.choices.as_ref().unwrap();
        assert_eq!(choices[0].label, "local");
        assert_eq!(choices[1].label, "Internet");
    }

    #[test]
    fn sections_general_first_then_groups_by_rank() {
        let g = |id: &str, rank: i64| serde_json::json!({"id": id, "label": id, "rank": rank});
        let opts = vec![
            opt(
                serde_json::json!({"name": "enabled", "type": {"kind": "bool"}, "typeLabel": "bool"}),
            ),
            opt(
                serde_json::json!({"name": "containers.x", "type": {"kind": "str"}, "typeLabel": "str", "ui": {"group": g("advanced", 300)}}),
            ),
            opt(
                serde_json::json!({"name": "subdomain", "type": {"kind": "str"}, "typeLabel": "str", "ui": {"group": g("access", 100), "summary": true}}),
            ),
            opt(serde_json::json!({"name": "port", "type": {"kind": "int"}, "typeLabel": "int"})),
        ];
        let s = build_sections(&opts);
        let ids: Vec<&str> = s.iter().map(|x| x.id.as_str()).collect();
        assert_eq!(ids, ["general", "access", "advanced"]);
        assert_eq!(s[0].options.len(), 2);
        assert!(!s[0].collapsible);
        assert_eq!(s[1].summary, ["subdomain"]);
    }

    #[test]
    fn group_crumb_drops_the_section_title() {
        let mut opts = vec![
            opt(serde_json::json!({
                "name": "git.userName",
                "type": {"kind": "str"},
                "typeLabel": "str",
                "ui": {"group": {"id": "git", "label": "Git", "rank": 20}}
            })),
            opt(serde_json::json!({
                "name": "nix.remoteBuild.host",
                "type": {"kind": "str"},
                "typeLabel": "str",
                "ui": {"group": {"id": "nix", "label": "Nix", "rank": 30}}
            })),
            opt(serde_json::json!({
                "name": "ssh.authorizedKeys",
                "type": {"kind": "listOf", "elem": {"kind": "str"}},
                "typeLabel": "list of str",
                "ui": {"group": {"id": "ssh", "label": "SSH", "rank": 10}}
            })),
            opt(serde_json::json!({
                "name": "auth.publicPaths",
                "type": {"kind": "listOf", "elem": {"kind": "str"}},
                "typeLabel": "list of str",
                "ui": {"group": {"id": "access", "label": "Access & security", "rank": 100}}
            })),
        ];
        label_options(&mut opts);
        let s = build_sections(&opts);
        let crumb = |id: &str, name: &str| {
            s.iter()
                .find(|sec| sec.id == id)
                .unwrap()
                .options
                .iter()
                .find(|o| o.name == name)
                .unwrap()
                .crumb
                .clone()
        };
        assert_eq!(crumb("git", "git.userName"), "");
        assert_eq!(crumb("nix", "nix.remoteBuild.host"), "Remote build");
        assert_eq!(crumb("ssh", "ssh.authorizedKeys"), "");
        assert_eq!(crumb("access", "auth.publicPaths"), "Auth");
    }

    #[test]
    fn no_general_section_when_everything_is_grouped() {
        let opts = vec![opt(serde_json::json!({
            "name": "subdomain", "type": {"kind": "str"}, "typeLabel": "str",
            "ui": {"group": {"id": "access", "label": "Access", "rank": 100}}
        }))];
        let s = build_sections(&opts);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].id, "access");
    }
}
