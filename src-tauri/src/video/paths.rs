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
        logo: target.job.effects.logo.as_ref().map(|l| l.enabled).unwrap_or(false),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::video::types::{AspectRatio, EncodingProfile, OutputJob, VideoEffectsSettings};

    fn make_target(label: &str, target_type: TargetType) -> OutputTarget {
        OutputTarget {
            id: label.to_string(),
            label: OutputTarget::sanitize_label(label),
            target_type: target_type.clone(),
            job: OutputJob {
                id: label.to_string(),
                ratio: AspectRatio::Ratio1x1,
                encoding: EncodingProfile::standard(),
                effects: VideoEffectsSettings::default(),
                platform_config: None,
                preset_name: if let TargetType::Platform = target_type { Some(label.to_string()) } else { None },
                source_preset_id: String::new(),
            },
        }
    }

    impl Default for VideoEffectsSettings {
        fn default() -> Self {
            Self {
                blur: None,
                overlays: None,
                subtitles: None,
                color_filter: None,
                blur_sigma: None,
                remove_audio: None,
                export_subtitles: None,
                burn_subtitles: None,
                skip_existing: None,
                output_format: None,
                logo: None,
                transform: None,
            }
        }
    }

    // CASE 1: Single aspect ratio, subfolders ON → must create subfolder
    #[test]
    fn case1_single_ratio_subfolders_on() {
        let target = make_target("1:1", TargetType::AspectRatio);
        let path = resolve_output_path(
            Path::new("/output"),
            Path::new("/input/video1.mp4"),
            &target,
            true,
        );
        let s = path.to_string_lossy().replace('\\', "/");
        assert!(
            s.contains("/output/1x1/"),
            "Expected /output/1x1/... got {:?}",
            path
        );
    }

    // CASE 2: Multiple aspect ratios, subfolders ON → each gets its own subfolder
    #[test]
    fn case2_multi_ratio_subfolders_on() {
        let targets = vec![
            make_target("1:1", TargetType::AspectRatio),
            make_target("4:5", TargetType::AspectRatio),
        ];
        let paths: Vec<_> = targets
            .iter()
            .map(|t| {
                resolve_output_path(Path::new("/output"), Path::new("/input/video1.mp4"), t, true)
            })
            .collect();
        assert!(paths[0].to_string_lossy().replace('\\', "/").contains("/output/1x1/"));
        assert!(paths[1].to_string_lossy().replace('\\', "/").contains("/output/4x5/"));
    }

    // CASE 3: Mixed targets (aspect ratio + platform), subfolders ON → both get subfolders
    #[test]
    fn case3_mixed_targets_subfolders_on() {
        let ratio_target = make_target("1:1", TargetType::AspectRatio);
        let platform_target = make_target("Instagram Square", TargetType::Platform);
        let p1 = resolve_output_path(Path::new("/output"), Path::new("/v.mp4"), &ratio_target, true);
        let p2 = resolve_output_path(Path::new("/output"), Path::new("/v.mp4"), &platform_target, true);
        assert!(p1.to_string_lossy().replace('\\', "/").contains("/output/1x1/"));
        assert!(p2.to_string_lossy().replace('\\', "/").contains("/output/instagram_square/"));
    }

    // CASE 4: Single platform preset, subfolders ON → must create subfolder
    #[test]
    fn case4_single_platform_subfolders_on() {
        let target = make_target("TikTok (9:16)", TargetType::Platform);
        let path = resolve_output_path(Path::new("/output"), Path::new("/v.mp4"), &target, true);
        assert!(path.to_string_lossy().replace('\\', "/").contains("/output/tiktok_9x16/"));
    }

    // CASE 5: Any targets, subfolders OFF → all outputs go to root directory
    #[test]
    fn case5_subfolders_off() {
        let targets = vec![
            make_target("1:1", TargetType::AspectRatio),
            make_target("Reddit", TargetType::Platform),
        ];
        for t in &targets {
            let path = resolve_output_path(Path::new("/output"), Path::new("/v.mp4"), t, false);
            let s = path.to_string_lossy().replace('\\', "/");
            assert!(
                !s.contains("/output/1x1/") && !s.contains("/output/reddit/"),
                "Expected root output, got {:?}",
                path
            );
            assert!(s.starts_with("/output/"));
        }
    }
}
