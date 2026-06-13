use std::path::PathBuf;

use tauri::{AppHandle, Manager};
use tauri_plugin_opener::OpenerExt;

const LICENSE_DIRECTORY_NAME: &str = "LICENSE";

const ALLOWED_LICENSE_DOCUMENTS: &[&str] = &[
    "README.md",
    "GPL-3.0.txt",
    "FFMPEG-GPL.txt",
    "OTHER-LICENSES.txt",
    "THIRD-PARTY-NOTICES.txt",
];

fn validate_license_file_name(file_name: &str) -> Result<&str, String> {
    if file_name.trim().is_empty() {
        return Err("Invalid filename: filename cannot be empty.".to_string());
    }
    if file_name.contains('/') || file_name.contains('\\') || file_name.contains("..") {
        return Err(format!("Invalid filename: {file_name}"));
    }
    if ALLOWED_LICENSE_DOCUMENTS.contains(&file_name) {
        Ok(file_name)
    } else {
        Err(format!("Unknown license document: {file_name}"))
    }
}

fn resolve_license_document_path(app: &AppHandle, file_name: &str) -> Result<PathBuf, String> {
    let resource_dir = app
        .path()
        .resource_dir()
        .map_err(|e| format!("Failed to resolve resource directory: {e}"))?;

    Ok(resource_dir.join(LICENSE_DIRECTORY_NAME).join(file_name))
}

#[tauri::command]
pub async fn open_license_document(app: AppHandle, file_name: String) -> Result<(), String> {
    let file_name = validate_license_file_name(&file_name)?;
    let document_path = resolve_license_document_path(&app, file_name)?;

    let metadata = std::fs::metadata(&document_path).map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            format!(
                "License document not found in bundled resources: {}",
                document_path.display()
            )
        } else {
            format!(
                "Failed to read license document metadata for {}: {}",
                document_path.display(),
                err
            )
        }
    })?;

    if !metadata.is_file() {
        return Err(format!(
            "Bundled license document is not a file: {}",
            document_path.display()
        ));
    }

    app.opener()
        .open_path(document_path.to_string_lossy().into_owned(), None::<&str>)
        .map_err(|err| {
            format!(
                "Failed to open license document {}: {}",
                document_path.display(),
                err
            )
        })
}

#[cfg(test)]
mod tests {
    use super::{validate_license_file_name, ALLOWED_LICENSE_DOCUMENTS};

    #[test]
    fn allows_only_known_document_names() {
        for name in ALLOWED_LICENSE_DOCUMENTS {
            assert_eq!(validate_license_file_name(name), Ok(*name));
        }
    }

    #[test]
    fn rejects_path_traversal_and_unknown_names() {
        assert!(validate_license_file_name("../README.md").is_err());
        assert!(validate_license_file_name("LICENSE/README.md").is_err());
        assert!(validate_license_file_name("not-real.txt").is_err());
    }
}
