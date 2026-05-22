mod game_discovery;
mod migration;
mod svd;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            game_discovery::detect_game_data_dir,
            migration::load_migration_targets,
            migration::run_migration,
            svd::load_svd_package_summary,
            svd::run_svd_export,
            svd::run_svd_pack
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
