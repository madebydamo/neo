mod actions;
mod activation;
mod branches;
mod changes;
mod helpers;
mod icons;
mod locks;
mod nix_repair;
mod oauth;
mod pages;
mod power;
mod resources;
mod save;
mod secrets;
mod settings_file;
mod snapshots;
mod ssh;
mod units;
mod ws;

use rocket::routes;

pub fn routes() -> Vec<rocket::Route> {
    routes![
        pages::site_webmanifest,
        icons::service_icon,
        pages::index,
        pages::nav_services,
        // Configuration shell + HTMX partials.
        pages::configuration,
        pages::configuration_services,
        pages::configuration_settings,
        pages::configuration_versioning,
        pages::configuration_option,
        pages::configuration_core,
        helpers::run_helper,
        oauth::run_oauth,
        save::save_service,
        save::save_core_section,
        changes::changes_action_bar,
        changes::changes_summary,
        changes::revert_settings,
        changes::apply_settings,
        actions::flake_update,
        actions::actions_activate,
        actions::actions_reset,
        branches::versioning_tree,
        branches::versioning_services,
        branches::versioning_diff,
        branches::versioning_activate,
        branches::versioning_gen_switch,
        power::versioning_gen_boot,
        power::system_reboot,
        power::system_poweroff,
        branches::versioning_note,
        activation::op_monitor,
        locks::locks_state,
        nix_repair::nix_repair_start,
        units::unit_restart,
        units::unit_start,
        units::unit_stop,
        units::container_update,
        units::clear_appdata,
        units::services_status,
        snapshots::service_snapshots,
        snapshots::service_snapshot_create,
        snapshots::service_snapshot_restore,
        snapshots::versioning_zfs,
        snapshots::versioning_zfs_snapshot,
        snapshots::versioning_zfs_restore,
        snapshots::versioning_zfs_reboot,
        snapshots::versioning_zfs_cancel,
        snapshots::versioning_zfs_dismiss,
        snapshots::versioning_zfs_delete,
        snapshots::versioning_zfs_pin,
        snapshots::versioning_zfs_unpin,
        snapshots::versioning_zfs_comment_form,
        snapshots::versioning_zfs_comment,
        snapshots::service_snapshot_delete,
        snapshots::service_snapshot_pin,
        snapshots::service_snapshot_unpin,
        snapshots::service_snapshot_comment_form,
        snapshots::service_snapshot_comment,
        units::sse_logs,
        resources::resources_snapshot,
        ws::ws_status,
        ws::ws_op,
        ssh::ssh_public_key_card,
        ssh::ssh_regenerate,
        secrets::secrets_card,
        secrets::secrets_split,
        secrets::secrets_enable,
        secrets::secrets_disable,
        secrets::secrets_rekey,
        settings_file::download_settings,
        settings_file::upload_settings,
    ]
}
