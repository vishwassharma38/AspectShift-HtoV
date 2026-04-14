pub mod video;

use tauri::Manager;

#[tauri::command]
fn get_ffmpeg_path(app: tauri::AppHandle) -> String {
  let resource_dir = app.path().resource_dir().unwrap();

  let ffmpeg = if cfg!(target_os = "windows") {
    "ffmpeg.exe"
  } else {
    "ffmpeg"
  };

  resource_dir
    .join(ffmpeg)
    .to_string_lossy()
    .into_owned()
}

#[tauri::command]
fn get_whisper_paths(app: tauri::AppHandle) -> (String, String) {
  let resource_dir = app.path().resource_dir().unwrap();

  let whisper_bin = if cfg!(target_os = "windows") {
    "whisper.exe"
  } else {
    "whisper"
  };

  let whisper_path = resource_dir
    .join(whisper_bin)
    .to_string_lossy()
    .into_owned();

  let model_path = resource_dir
    .join("ggml-base.bin")
    .to_string_lossy()
    .into_owned();

  (whisper_path, model_path)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  tracing_subscriber::fmt()
    .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
    .init();

  tauri::Builder::default()
    .plugin(tauri_plugin_updater::Builder::new().build())
    .setup(|app| {
      app.manage(video::queue::BatchManager::new());
      Ok(())
    })
    .invoke_handler(tauri::generate_handler![
      get_ffmpeg_path,
      get_whisper_paths,
      video::detect_orientation,
      video::convert_to_ratio,
      video::check_file_ready,
      video::release_processing_lock,
      video::start_batch,
      video::cancel_batch,
      video::get_batch_status,
      video::clear_batch
    ])

    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}