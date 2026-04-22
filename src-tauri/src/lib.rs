pub mod subtitles;
pub mod video;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            app.manage(video::queue::BatchManager::new());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            video::detect_orientation,
            video::convert_to_ratio,
            video::check_file_ready,
            video::release_processing_lock,
            video::start_batch,
            video::cancel_batch,
            video::get_batch_status,
            video::clear_batch,
            video::presets::get_all_presets,
            video::presets::save_preset,
            video::presets::delete_preset
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
