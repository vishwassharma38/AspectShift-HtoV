pub mod os_utils;
pub mod subtitles;
pub mod video;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::new()
            .level(log::LevelFilter::Info)
            .build())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // Automatic cleanup of stale lock files at startup
            let _ = video::lock::cleanup_stale_locks(app.handle());

            app.manage(video::queue::BatchManager::new());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            video::detect_orientation,
            video::convert_to_ratio,
            video::check_file_ready,
            video::start_batch,
            video::cancel_batch,
            video::get_batch_status,
            video::clear_batch,
            video::open_output_folder,
            video::get_all_presets,
            video::config::get_config,
            video::config::update_config,
            video::config::reset_config,
            video::presets::get_builtin_platform_presets,
            video::presets::get_all_aspect_ratio_targets,
            video::presets::save_preset,
            video::presets::delete_preset
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
