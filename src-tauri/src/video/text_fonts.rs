use crate::video::types::{TextFontStyle, VideoError};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

const FONT_ROOT_RELATIVE: &[&str] = &["fonts", "text-overlay"];
const REQUIRED_FACES: &[FontFace] = &[
    FontFace::Regular,
    FontFace::Bold,
    FontFace::Italic,
    FontFace::BoldItalic,
];

#[derive(Clone, Copy)]
enum FontFace {
    Regular,
    Bold,
    Italic,
    BoldItalic,
}

impl FontFace {
    fn suffix(self) -> &'static str {
        match self {
            Self::Regular => "Regular",
            Self::Bold => "Bold",
            Self::Italic => "Italic",
            Self::BoldItalic => "BoldItalic",
        }
    }
}

#[derive(Clone, Copy)]
pub struct TextFont {
    pub ass_name: &'static str,
    pub directory: &'static str,
    pub regular_file: &'static str,
}

impl TextFont {
    fn family_dir(self, root: &Path) -> PathBuf {
        root.join(self.directory)
    }

    fn regular_path(self, root: &Path) -> PathBuf {
        self.family_dir(root).join(self.regular_file)
    }

    fn required_file_name(self, face: FontFace) -> String {
        let path = Path::new(self.regular_file);
        let stem = path
            .file_stem()
            .and_then(OsStr::to_str)
            .unwrap_or(self.regular_file);
        let ext = path.extension().and_then(OsStr::to_str).unwrap_or("ttf");
        let prefix = stem.strip_suffix("-Regular").unwrap_or(stem);
        format!("{prefix}-{}.{}", face.suffix(), ext)
    }

    fn required_paths(self, root: &Path) -> Vec<PathBuf> {
        REQUIRED_FACES
            .iter()
            .map(|face| self.family_dir(root).join(self.required_file_name(*face)))
            .collect()
    }
}

pub fn family(style: &TextFontStyle) -> TextFont {
    match style {
        TextFontStyle::Clean => TextFont {
            ass_name: "Fira Sans",
            directory: "fira-sans",
            regular_file: "FiraSans-Regular.ttf",
        },
        TextFontStyle::Minimal => TextFont {
            ass_name: "Lato",
            directory: "lato",
            regular_file: "Lato-Regular.ttf",
        },
        TextFontStyle::Caption => TextFont {
            ass_name: "Inter",
            directory: "inter",
            regular_file: "Inter-Regular.ttf",
        },
        TextFontStyle::Meme => TextFont {
            ass_name: "Anton",
            directory: "anton",
            regular_file: "Anton-Regular.ttf",
        },
        TextFontStyle::Creator => TextFont {
            ass_name: "Montserrat",
            directory: "montserrat",
            regular_file: "Montserrat-Regular.ttf",
        },
        TextFontStyle::Gaming => TextFont {
            ass_name: "Exo 2",
            directory: "exo-2",
            regular_file: "Exo2-Regular.ttf",
        },
        TextFontStyle::Cyberpunk => TextFont {
            ass_name: "Orbitron",
            directory: "orbitron",
            regular_file: "Orbitron-Regular.ttf",
        },
        TextFontStyle::Cinematic => TextFont {
            ass_name: "Cormorant Garamond",
            directory: "cormorant-garamond",
            regular_file: "CormorantGaramond-Regular.ttf",
        },
        TextFontStyle::Retro => TextFont {
            ass_name: "Bungee",
            directory: "bungee",
            regular_file: "Bungee-Regular.ttf",
        },
        TextFontStyle::Handwritten => TextFont {
            ass_name: "Caveat",
            directory: "caveat",
            regular_file: "Caveat-Regular.ttf",
        },
    }
}

fn all_styles() -> [TextFontStyle; 10] {
    [
        TextFontStyle::Clean,
        TextFontStyle::Minimal,
        TextFontStyle::Caption,
        TextFontStyle::Meme,
        TextFontStyle::Creator,
        TextFontStyle::Gaming,
        TextFontStyle::Cyberpunk,
        TextFontStyle::Cinematic,
        TextFontStyle::Retro,
        TextFontStyle::Handwritten,
    ]
}

fn source_font_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join("fonts")
        .join("text-overlay")
}

fn resource_font_root(app: &AppHandle) -> Result<PathBuf, VideoError> {
    let resource_dir = app.path().resource_dir()?;
    Ok(FONT_ROOT_RELATIVE
        .iter()
        .fold(resource_dir, |path, segment| path.join(segment)))
}

fn canonicalize_if_possible(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn format_status(path: &Path) -> String {
    if path.is_file() {
        format!("found: {}", path.display())
    } else if path.is_dir() {
        format!("directory: {}", path.display())
    } else {
        format!("missing: {}", path.display())
    }
}

fn is_supported_font_file(path: &Path) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .map(|ext| matches!(ext.to_ascii_lowercase().as_str(), "ttf" | "otf" | "ttc"))
        .unwrap_or(false)
}

fn discover_font_files(directory: &Path) -> Result<Vec<PathBuf>, VideoError> {
    let mut pending = vec![directory.to_path_buf()];
    let mut files = Vec::new();

    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                pending.push(path);
            } else if file_type.is_file() && is_supported_font_file(&path) {
                files.push(canonicalize_if_possible(&path));
            }
        }
    }

    files.sort();
    files.dedup();
    Ok(files)
}

fn root_diagnostics(root: &Path) -> String {
    let mut lines = vec![
        format!("Resolved font root: {}", root.display()),
        format!(
            "Root status: {}",
            if root.is_dir() {
                "found"
            } else {
                "missing directory"
            }
        ),
    ];

    for style in all_styles() {
        let font = family(&style);
        lines.push(format!(
            "{} ({}) family directory: {}",
            font.ass_name,
            font.directory,
            if font.family_dir(root).is_dir() {
                "found"
            } else {
                "missing"
            }
        ));
    }

    lines.join("\n")
}

fn font_diagnostics(root: &Path, font: TextFont, found: &[PathBuf]) -> String {
    let mut lines = vec![
        format!("Font family: {}", font.ass_name),
        format!("Resolved root: {}", root.display()),
        format!(
            "Resolved family directory: {}",
            font.family_dir(root).display()
        ),
        "Expected required faces:".to_string(),
    ];

    for path in font.required_paths(root) {
        lines.push(format!("  - {}", format_status(&path)));
    }

    lines.push("Discovered font files:".to_string());
    if found.is_empty() {
        lines.push("  - none".to_string());
    } else {
        for path in found {
            lines.push(format!("  - {}", path.display()));
        }
    }

    lines.join("\n")
}

fn validate_font_root(root: &Path) -> Result<(), String> {
    if !root.is_dir() {
        return Err(root_diagnostics(root));
    }

    for style in all_styles() {
        let font = family(&style);
        let family_dir = font.family_dir(root);
        let found = discover_font_files(&family_dir).unwrap_or_default();
        let missing: Vec<PathBuf> = font
            .required_paths(root)
            .into_iter()
            .filter(|path| !path.is_file())
            .collect();

        if !missing.is_empty() {
            return Err(font_diagnostics(root, font, &found));
        }
    }

    Ok(())
}

fn resolve_bundled_font_root(app: &AppHandle) -> Result<PathBuf, VideoError> {
    let bundled = resource_font_root(app)?;
    if validate_font_root(&bundled).is_ok() {
        return Ok(canonicalize_if_possible(&bundled));
    }

    #[cfg(debug_assertions)]
    {
        let development = source_font_root();
        if validate_font_root(&development).is_ok() {
            tracing::warn!(
                "Bundled text font resources are invalid in dev mode; using source resources. Bundled diagnostics:\n{}",
                validate_font_root(&bundled).unwrap_err()
            );
            return Ok(canonicalize_if_possible(&development));
        }
    }

    Err(VideoError::InvalidInput(format!(
        "Bundled text font resources are missing or incomplete.\n{}",
        validate_font_root(&bundled).unwrap_err()
    )))
}

pub fn resolve_text_overlay_font(
    app: &AppHandle,
    style: &TextFontStyle,
) -> Result<PathBuf, VideoError> {
    let directory = resolve_bundled_font_root(app)?;
    resolve_text_overlay_font_from_root(&directory, style)
}

fn resolve_text_overlay_font_from_root(
    directory: &Path,
    style: &TextFontStyle,
) -> Result<PathBuf, VideoError> {
    let font = family(style);
    let path = font.regular_path(directory);
    let discovered = discover_font_files(&font.family_dir(directory)).unwrap_or_default();
    if !path.is_file() {
        return Err(VideoError::InvalidInput(format!(
            "Bundled text font '{}' is missing its regular face.\n{}",
            font.ass_name,
            font_diagnostics(directory, font, &discovered)
        )));
    }

    Ok(canonicalize_if_possible(&path))
}

pub fn resolve_text_overlay_font_files(
    app: &AppHandle,
    style: &TextFontStyle,
) -> Result<Vec<PathBuf>, VideoError> {
    let directory = resolve_bundled_font_root(app)?;
    resolve_text_overlay_font_files_from_root(&directory, style)
}

pub fn resolve_subtitle_overlay_font_files(
    app: &AppHandle,
    style: &TextFontStyle,
) -> Result<Vec<PathBuf>, VideoError> {
    let directory = resolve_bundled_font_root(app)?;
    resolve_text_overlay_font_files_from_root(&directory, style)
}

fn resolve_text_overlay_font_files_from_root(
    directory: &Path,
    style: &TextFontStyle,
) -> Result<Vec<PathBuf>, VideoError> {
    let font = family(style);
    let family_dir = font.family_dir(directory);
    let discovered = if family_dir.is_dir() {
        discover_font_files(&family_dir)?
    } else {
        Vec::new()
    };
    let missing: Vec<PathBuf> = font
        .required_paths(directory)
        .into_iter()
        .filter(|path| !path.is_file())
        .collect();

    if !missing.is_empty() {
        return Err(VideoError::InvalidInput(format!(
            "Bundled text font '{}' is incomplete.\n{}",
            font.ass_name,
            font_diagnostics(directory, font, &discovered)
        )));
    }

    Ok(discovered)
}

pub fn resolve_bundled_font_dir(
    app: &AppHandle,
    style: &TextFontStyle,
) -> Result<PathBuf, VideoError> {
    let font_path = resolve_text_overlay_font(app, style)?;
    font_path.parent().map(PathBuf::from).ok_or_else(|| {
        VideoError::InvalidInput(format!(
            "Bundled text font '{}' has no parent directory",
            family(style).ass_name
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::{all_styles, family, validate_font_root};
    use crate::video::types::TextFontStyle;
    use std::collections::HashSet;

    #[test]
    fn bundled_text_fonts_are_discovered_from_family_directories() {
        let mut all_regular_files = HashSet::new();
        let font_dir = super::source_font_root();

        validate_font_root(&font_dir).expect("source text fonts should be complete");

        for style in all_styles() {
            let font = family(&style);
            let family_dir = super::canonicalize_if_possible(&font.family_dir(&font_dir));
            assert!(
                all_regular_files.insert(font.regular_file),
                "duplicate regular font face: {}",
                font.regular_file
            );
            let files = super::resolve_text_overlay_font_files_from_root(&font_dir, &style)
                .expect("font files should resolve");
            assert!(files.len() >= 4);
            assert!(
                files.iter().all(|path| path.starts_with(&family_dir)),
                "all discovered files for {} should stay inside its family directory",
                font.ass_name
            );
        }

        assert_eq!(all_regular_files.len(), 10);
    }

    #[test]
    fn meme_uses_bundled_anton() {
        let font = family(&TextFontStyle::Meme);
        assert_eq!(font.ass_name, "Anton");
        assert_eq!(font.directory, "anton");
        assert_eq!(font.regular_file, "Anton-Regular.ttf");
    }

    #[test]
    fn flat_resource_root_is_reported_as_incomplete() {
        let root =
            std::env::temp_dir().join(format!("aspectshift_flat_fonts_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("temp font root should be created");
        std::fs::write(root.join("FiraSans-Regular.ttf"), b"placeholder")
            .expect("placeholder font should be created");

        let error = super::resolve_text_overlay_font_files_from_root(&root, &TextFontStyle::Clean)
            .expect_err("flat resource root should fail");
        let _ = std::fs::remove_dir_all(&root);

        let message = error.to_string();
        assert!(message.contains("Bundled text font 'Fira Sans' is incomplete"));
        assert!(message.contains("Resolved family directory"));
        assert!(message.contains("fira-sans"));
        assert!(message.contains("FiraSans-Regular.ttf"));
    }

    #[test]
    fn missing_bundled_text_font_returns_clear_error() {
        let missing_root = std::env::temp_dir().join(format!(
            "aspectshift_missing_fonts_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&missing_root).expect("temp font root should be created");

        let error =
            super::resolve_text_overlay_font_from_root(&missing_root, &TextFontStyle::Clean)
                .expect_err("missing font should fail");
        let _ = std::fs::remove_dir_all(&missing_root);

        let message = error.to_string();
        assert!(message.contains("Bundled text font 'Fira Sans' is missing its regular face"));
        assert!(message.contains("Resolved family directory"));
        assert!(message.contains("FiraSans-Regular.ttf"));
    }

    #[test]
    fn missing_bundled_text_font_variant_returns_clear_error() {
        let missing_root = std::env::temp_dir().join(format!(
            "aspectshift_missing_font_variants_{}",
            uuid::Uuid::new_v4()
        ));
        let family_dir = missing_root.join("fira-sans");
        std::fs::create_dir_all(&family_dir).expect("temp font family should be created");
        std::fs::write(family_dir.join("FiraSans-Regular.ttf"), b"placeholder")
            .expect("placeholder font should be created");

        let error =
            super::resolve_text_overlay_font_files_from_root(&missing_root, &TextFontStyle::Clean)
                .expect_err("missing variants should fail");
        let _ = std::fs::remove_dir_all(&missing_root);

        let message = error.to_string();
        assert!(message.contains("Bundled text font 'Fira Sans' is incomplete"));
        assert!(message.contains("FiraSans-Bold.ttf"));
        assert!(message.contains("FiraSans-Italic.ttf"));
    }
}
