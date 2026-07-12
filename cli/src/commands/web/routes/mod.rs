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
        pages::configuration,
        pages::option_pane,
        pages::services_grid,
        pages::core_grid,
        pages::core_pane,
        helpers::run_helper,
        save::save_service,
        save::save_core_section,
        changes::changes_action_bar,
        changes::changes_indicator,
        changes::reset_button,
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
        activation::activation_current,
        activation::update_monitor,
        activation::update_log,
        activation::update_status,
        units::unit_status,
        units::unit_logs,
        units::unit_restart,
        units::unit_start,
        units::unit_stop,
        units::container_update,
        units::sse_logs,
        ws::ws_status,
        ssh::ssh_public_key,
        ssh::ssh_public_key_txt,
        ssh::ssh_public_key_card,
        ssh::ssh_regenerate,
    ]
}
