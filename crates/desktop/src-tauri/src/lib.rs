mod command_error;
mod game_discovery;
mod migration;
mod svd;
mod task;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(task::TaskRegistry::default())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            game_discovery::detect_game_data_dir,
            game_discovery::validate_game_data_dir,
            migration::load_equipment_options,
            migration::inspect_patch,
            migration::migrate_equipment,
            migration::repatch_mod,
            migration::cancel_task,
            svd::load_svd_package_summary,
            svd::run_svd_export,
            svd::run_svd_pack
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
