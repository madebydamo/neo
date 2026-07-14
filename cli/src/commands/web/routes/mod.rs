mod actions;
mod activation;
mod branches;
mod changes;
mod helpers;
mod pages;
mod save;
mod ssh;
mod units;
mod ws;

use rocket::routes;

pub fn routes() -> Vec<rocket::Route> {
    routes![
        pages::index,
        pages::nav_services,
        // Canonical configuration URLs (more specific paths before legacy aliases).
        pages::configuration,
        pages::configuration_services,
        pages::configuration_settings,
        pages::configuration_versioning,
        pages::configuration_option,
        pages::configuration_core,
        // Legacy partial aliases (still used as fallbacks; non-HTMX redirects).
        pages::option_pane,
        pages::services_grid,
        pages::core_grid,
        pages::core_pane,
        helpers::run_helper,
        save::save_service,
        save::save_core_section,
        changes::changes_action_bar,
        changes::changes_summary,
        changes::revert_settings,
        changes::apply_settings,
        actions::flake_update,
        actions::actions_activate,
        actions::actions_reset,
        branches::branches,
        branches::git_switch,
        activation::activation_monitor,
        activation::activation_log,
        activation::activation_status,
        activation::update_monitor,
        activation::update_log,
        activation::update_status,
        units::unit_restart,
        units::unit_start,
        units::unit_stop,
        units::container_update,
        units::clear_appdata,
        units::sse_logs,
        ws::ws_status,
        ssh::ssh_public_key_card,
        ssh::ssh_regenerate,
    ]
}
