use crate::video::types::{OutputTarget, TargetType};
use std::path::{Path, PathBuf};

fn format_ratio_tag(tag: &str) -> String {
    OutputTarget::sanitize_label(tag)
}

pub fn resolve_output_path(
    base_dir: &Path,
    input_path: &Path,
    target: &OutputTarget,
    use_subfolders: bool,
) -> PathBuf {
    let stem = input_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("video");
    let output_format = target.job.effects.output_format_value();
    let ext = output_format.get_extension();

    let platform_tag = match target.target_type {
        TargetType::Platform => Some(target.label.clone()),
        _ => None,
    };

    // DO NOT manually format ratio or labels.
    // Use centralized helpers only.
    let tags = crate::video::types::OutputTags {
        ratio: format_ratio_tag(target.job.ratio.get_tag()),
        platform: platform_tag,
        blur: target.job.effects.blur_enabled(),
        white_background: target.job.effects.white_background_enabled(),
        logo: target
            .job
            .effects
            .logo
            .as_ref()
            .map(|l| l.enabled)
            .unwrap_or(false),
        subtitles: target.job.effects.burn_subtitles_enabled()
            || target.job.effects.export_subtitles_enabled()
            || target.job.effects.subtitles.is_some(),
        no_audio: target.job.effects.remove_audio_enabled(),
    };

    let filename = tags.get_output_filename(stem, &ext);

    if use_subfolders {
        base_dir.join(&target.label).join(filename)
    } else {
        base_dir.join(filename)
    }
}

pub fn resolve_temp_output_path(final_output: &Path) -> PathBuf {
    let parent = final_output.parent().unwrap_or_else(|| Path::new(""));
    let stem = final_output
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");

    match final_output.extension().and_then(|e| e.to_str()) {
        Some(ext) if !ext.is_empty() => parent.join(format!("{stem}.tmp.{ext}")),
        _ => parent.join(format!("{stem}.tmp")),
    }
}

fn is_temporary_render_output(path: &Path) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if ext.eq_ignore_ascii_case("partial") || ext.eq_ignore_ascii_case("tmp") {
        return true;
    }

    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    stem.ends_with(".partial") || stem.ends_with(".tmp")
}

pub fn cleanup_orphan_temp_outputs(root: &Path) -> std::io::Result<usize> {
    fn visit(path: &Path, removed: &mut usize) -> std::io::Result<()> {
        if !path.exists() {
            return Ok(());
        }
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let p = entry.path();
            if p.is_dir() {
                visit(&p, removed)?;
            } else if is_temporary_render_output(&p) {
                if std::fs::remove_file(&p).is_ok() {
                    *removed += 1;
                }
            }
        }
        Ok(())
    }

    let mut removed = 0usize;
    visit(root, &mut removed)?;
    Ok(removed)
}
