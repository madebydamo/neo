//! `--from-store <login>`: a backup store read over its server's normal SSH
//! (docs/design/backup-store-ssh.md §4). Shared by the local and the remote
//! mode and by `neo backup`.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::utils::backup::store_ssh::{self, Connect, Opens, StoreRoute};
use crate::utils::backup::Source;

/// The stores of `--from-store`, each with its way in, and the key that opens
/// the picked client.
pub(crate) struct Stores {
    pub routes: Vec<StoreRoute>,
    /// A key file of yours that opens the picked client.
    pub key: Option<PathBuf>,
}

/// Ask each login what its store holds, show it with your keys, and pick a
/// client and folder when `pick` (no config repo given): `client`, the only
/// one your keys open, or ask (`ask`).
pub(crate) fn open_stores(
    logins: &[String],
    key: Option<&Path>,
    client: Option<&str>,
    pick: bool,
    ask: bool,
) -> Result<Stores> {
    let extra: Vec<PathBuf> = key.map(Path::to_path_buf).into_iter().collect();
    let mine = store_ssh::my_keys(&extra);
    let mut routes = Vec::new();
    let mut listings = Vec::new();
    for login in logins {
        let listing = store_ssh::fetch(login)?;
        store_ssh::show(&listing, &mine);
        routes.push(StoreRoute::new(
            &listing,
            Connect::Jump {
                login: login.clone(),
            },
        )?);
        listings.push(listing);
    }
    let mut found_key = None;
    if pick && !listings.is_empty() {
        // The store that holds the client asked for, else the first.
        let i = client
            .and_then(|c| listings.iter().position(|l| l.client(c).is_some()))
            .unwrap_or(0);
        let picked = store_ssh::pick(&listings[i], &mine, client, ask)?;
        let report = listings[i]
            .client(&picked.client)
            .context("picked client vanished")?;
        match store_ssh::opens(report, &mine) {
            Opens::Yes(m) => {
                println!(
                    "→ {} from {}, with {}",
                    picked.client, listings[i].host, m.label
                );
                found_key = m.private;
            }
            o => println!(
                "! {}: {} — trying with the keys you gave",
                picked.client,
                store_ssh::explain(report, &o)
            ),
        }
        routes[i].picked = Some(picked);
    }
    Ok(Stores {
        routes,
        key: found_key,
    })
}

/// `ssh+store://<client>@<login>/<folder>`: the store's route with that
/// backup picked (asks the login for the store's address and host key).
/// None for any other form.
pub(crate) fn spec_route(spec: &str) -> Result<Option<StoreRoute>> {
    let Some((client, login, folder)) = store_ssh::parse_spec(spec)? else {
        return Ok(None);
    };
    let listing = store_ssh::fetch(&login)?;
    let report = listing
        .client(&client)
        .with_context(|| format!("the store on {login} holds no client {client}"))?;
    let mine = store_ssh::my_keys(&[]);
    println!(
        "  {}: {}",
        client,
        store_ssh::explain(report, &store_ssh::opens(report, &mine))
    );
    let mut route = StoreRoute::new(&listing, Connect::Jump { login })?;
    route.picked = Some(store_ssh::Picked {
        client,
        folder,
        format: report.format.clone(),
    });
    Ok(Some(route))
}

/// `--from-backup`: `ssh+store://…` ([`spec_route`]) or any form
/// [`Source::from_location`] takes.
pub(crate) fn backup_source(spec: &str, work: &Path) -> Result<Source> {
    match spec_route(spec)? {
        Some(route) => route.picked_source(work)?.context("no backup picked"),
        None => Source::from_location(spec),
    }
}
