use crate::video::types::{AspectRatio, ConversionOptions};
use crate::os_utils::OsUtils;
use std::path::{Path, PathBuf};

pub struct OutputPathResolver<'a> {
    pub input_path: &'a Path,
    pub output_dir: &'a Path,
    pub ratio: &'a AspectRatio,
    pub options: &'a ConversionOptions,
    pub preset_name: Option<&'a str>,
    pub subfolder: Option<String>,
}

pub fn resolve_output_path(resolver: OutputPathResolver) -> PathBuf {
    let stem = resolver.input_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("video");
    let ext = resolver.options.output_format.get_extension();

    let platform_tag = resolver.preset_name.map(|name| {
        let base_name = name.split('(').next().unwrap_or(name).trim();
        OsUtils::sanitize_path_component(base_name)
    });

    let tags = crate::video::types::OutputTags {
        ratio: resolver.ratio.get_tag().replace(':', "x"),
        platform: platform_tag,
        blur: resolver.options.blur_background,
        logo: resolver.options.logo.as_ref().map(|l| l.enabled).unwrap_or(false),
        subtitles: resolver.options.burn_subtitles || resolver.options.generate_subtitles,
        no_audio: resolver.options.remove_audio,
    };

    let filename = tags.get_output_filename(stem, &ext);
    
    if let Some(sub) = resolver.subfolder {
        resolver.output_dir.join(sub).join(filename)
    } else {
        resolver.output_dir.join(filename)
    }
}
