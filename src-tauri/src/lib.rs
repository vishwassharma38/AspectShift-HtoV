pub mod auth;
pub mod dependency_manager;
pub mod download_manager;
pub mod manifest_service;
pub mod os_utils;
pub mod runtime_paths;
pub mod subtitles;
pub mod video;

#[cfg(all(feature = "dev-auth", not(debug_assertions)))]
compile_error!("dev-auth simulation paths must not be compiled in production builds.");

use std::sync::Arc;

use auth::auth_commands::{
    activate_license, check_update_entitlement, clear_license, get_auth_state, refresh_license,
};
use auth::manager::auth_manager::AuthManager;
use auth::providers::ActiveLicenseProvider;
use dependency_manager::{AppDepsState, DependencyId, DepsManager};
use dotenvy::dotenv;
use download_manager::DownloadManager;
use tauri::{AppHandle, Manager, State};
use video::types::StructuredError;

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
    deps_manager
        .refresh(&app)
        .await
        .map_err(StructuredError::from)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    dotenv().ok();

    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .build(),
        )
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

            let provider: Arc<dyn auth::providers::r#trait::LicenseProvider> =
                Arc::new(ActiveLicenseProvider::new());

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
                auth_manager_for_init
                    .run_launch_validation(&app_handle_for_auth)
                    .await;
            });

            app.manage(video::queue::BatchManager::new());
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Focused(true) = event {
                let app_handle = window.app_handle().clone();
                if let Some(auth_manager) = window.app_handle().try_state::<AuthManager>() {
                    let manager = auth_manager.inner().clone();
                    tauri::async_runtime::spawn(async move {
                        manager.trigger_reactive_refresh(&app_handle).await;
                    });
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_dependency_state,
            rescan_dependencies,
            install_dependency,
            get_auth_state,
            activate_license,
            refresh_license,
            check_update_entitlement,
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
