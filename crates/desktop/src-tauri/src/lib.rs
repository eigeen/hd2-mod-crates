mod command_error;
mod game_discovery;
mod migration;
mod svd;
mod task;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            focus_main_window(app);
        }))
        .manage(task::TaskRegistry::default())
        .plugin(tauri_plugin_clipboard_manager::init())
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

/// Bring the existing application window forward when another instance starts.
fn focus_main_window(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_focus();
}
