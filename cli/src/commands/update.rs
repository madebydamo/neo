use anyhow::Result;
use toml_edit::DocumentMut;

use crate::utils::{
    neo_cli_get, resolve_suffix, resolve_template, run_nix, run_write_flake, template, OperationKind,
    OperationLog,
};

pub fn update(
    config_path: &str,
    config: &DocumentMut,
    profile: &str,
    dry_run: bool,
    nix_cmd: &str,
    update_suffix: Option<&str>,
) -> Result<()> {
    if dry_run {
        println!(
            "DRY-RUN: nix flake update in {} (profile={})",
            config_path, profile
        );
        println!("DRY-RUN: would roll back the files the last template provided and apply the current template (if bootstrapMethod is template)");
        println!("DRY-RUN: would nix run .#neo -- migrate (from updated input)");
        return Ok(());
    }

    let suffix = resolve_suffix(update_suffix, "NEO_UPDATE_SUFFIX");
    let op = OperationLog::new(OperationKind::Update, &suffix);
    op.write_state("in_progress", "starting", None, None);
    let _tee = op.capture_stdio();

    // Roll back the files the previous template provided and apply the
    // (possibly newly updated) template; files the operator added stay
    // (utils/template.rs). A cloned repo is its own source of truth.
    let bootstrap_method = neo_cli_get(config, profile, "bootstrapMethod").unwrap_or("template");

    if bootstrap_method != "clone" {
        let template = resolve_template(config, profile);
        let report = op.step("flake init", || {
            template::install(config_path, &template, nix_cmd)
        })?;
        report.print();
        template::stage(config_path, &report)?;
    }

    op.step("write-flake", || run_write_flake(config_path, nix_cmd))?;
    op.step("flake update", || {
        run_nix(config_path, nix_cmd, &["flake", "update"])
    })?;
    op.step("migrate", || {
        run_nix(config_path, nix_cmd, &["run", ".#neo", "--", "migrate"])
    })?;
    op.write_state("success", "complete", None, None);
    println!("Flake updated and migrated in {}", config_path);
    Ok(())
}
