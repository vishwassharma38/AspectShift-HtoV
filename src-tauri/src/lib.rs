// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
use tauri::Manager;

#[tauri::command]
fn get_ffmpeg_path(app: tauri::AppHandle) -> String {
  let resource_dir = app.path().resource_dir().unwrap();
  #[cfg(target_os = "windows")]
  let bin = resource_dir.join("resources/ffmpeg.exe");
  #[cfg(not(target_os = "windows"))]
  let bin = resource_dir.join("resources/ffmpeg");
  bin.to_string_lossy().into_owned()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  tauri::Builder::default()
    .plugin(tauri_plugin_updater::Builder::new().build())
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}