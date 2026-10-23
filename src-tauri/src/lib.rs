mod commands;
mod models;
mod rules;
mod state;
mod store;
mod util;

use tauri::Manager;

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::env::temp_dir().join("mole-desktop"));
            let _ = std::fs::create_dir_all(&data_dir);
            app.manage(AppState::new(data_dir));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::system::system_snapshot,
            commands::clean::scan_clean,
            commands::clean::run_clean,
            commands::clean::reveal_path,
            commands::clean::home_dir,
            commands::clean::list_installers,
            commands::clean::delete_installers,
            commands::apps::list_apps,
            commands::apps::app_leftovers,
            commands::apps::uninstall_app,
            commands::analyze::analyze_dir,
            commands::analyze::analyze_shortcuts,
            commands::projects::scan_projects,
            commands::projects::purge_projects,
            commands::projects::purge_roots,
            commands::optimize::list_optimize_tasks,
            commands::optimize::run_optimize,
            commands::misc::app_info,
            commands::misc::get_history,
            commands::misc::clear_history,
            commands::misc::get_settings,
            commands::misc::save_settings,
            commands::misc::open_data_dir,
            commands::misc::open_external,
            commands::update::check_for_update,
            commands::update::download_and_install_update,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Mole Desktop");
}
