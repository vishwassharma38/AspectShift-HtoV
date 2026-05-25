pub mod os_utils;
pub mod runtime_paths;
pub mod subtitles;
pub mod video;
pub mod dependency_manager;
pub mod manifest_service;
pub mod download_manager;
pub mod auth;

#[cfg(all(feature = "dev-auth", not(debug_assertions)))]
compile_error!("dev-auth simulation paths must not be compiled in production builds.");

use std::sync::Arc;

use dotenvy::dotenv;
use tauri::{AppHandle, Manager, State};
use video::types::StructuredError;
use dependency_manager::{AppDepsState, DependencyId, DepsManager};
use download_manager::DownloadManager;
use auth::manager::auth_manager::AuthManager;
use auth::providers::ActiveLicenseProvider;
use auth::auth_commands::{activate_license, clear_license, get_auth_state, refresh_license};

#[tauri::command]
async fn get_dependency_state(
    manager: State<'_, DepsManager>,
) -> Result<AppDepsState, StructuredError> {
    Ok(manager.get_state().await)
}

#[tauri::command]
async fn rescan_dependencies(
    app: AppHandle,
    manager: State<'_, DepsManager>,
) -> Result<AppDepsState, StructuredError> {
    manager.refresh(&app).await.map_err(StructuredError::from)
}

#[tauri::command]
async fn install_dependency(
    app: AppHandle,
    id: DependencyId,
    deps_manager: State<'_, DepsManager>,
    download_manager: State<'_, DownloadManager>,
) -> Result<AppDepsState, StructuredError> {
    download_manager
        .install_dependency(&app, id)
        .await
        .map_err(StructuredError::from)?;
    deps_manager.refresh(&app).await.map_err(StructuredError::from)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    dotenv().ok();

    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::new()
            .level(log::LevelFilter::Info)
            .build())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            if let Ok(paths) = runtime_paths::RuntimePaths::from_app(app.handle()) {
                let _ = paths.ensure_runtime_tree();
            }
            let _ = video::lock::cleanup_stale_locks(app.handle());

            let deps_manager = DepsManager::new();
            let download_manager = DownloadManager::new();
            let app_handle = app.handle().clone();
            let deps_for_init = deps_manager.clone();
            app.manage(deps_manager);
            app.manage(download_manager);

            #[cfg(feature = "dev-auth")]
            let provider: Arc<dyn auth::providers::r#trait::LicenseProvider> =
                Arc::new(ActiveLicenseProvider::new());
            #[cfg(not(feature = "dev-auth"))]
            let provider: Arc<dyn auth::providers::r#trait::LicenseProvider> =
                Arc::new(ActiveLicenseProvider);

            let auth_manager = AuthManager::new(provider);
            let auth_manager_for_init = auth_manager.clone();
            app.manage(auth_manager);

            tauri::async_runtime::spawn(async move {
                if let Err(e) = deps_for_init.refresh(&app_handle).await {
                    log::error!("Initial dependency refresh failed: {}", e);
                }
            });
            let app_handle_for_auth = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                auth_manager_for_init.run_launch_validation(&app_handle_for_auth).await;
            });

            app.manage(video::queue::BatchManager::new());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_dependency_state,
            rescan_dependencies,
            install_dependency,
            get_auth_state,
            activate_license,
            refresh_license,
            clear_license,
            video::allow_path_scope,
            video::get_first_video_in_folder,
            video::get_videos_in_folder,
            video::detect_orientation,
            video::compute_preview_layout,
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
