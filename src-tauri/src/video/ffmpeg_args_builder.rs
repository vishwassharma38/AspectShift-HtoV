use crate::video::preset_adapter::Preset;

pub fn build_ffmpeg_args(
    input: &str,
    output: &str,
    filter_graph: &str,
    preset: &Preset
) -> Vec<String> {
    let mut args = vec![
        "-i".to_string(), input.to_string(),
        "-vf".to_string(), filter_graph.to_string(),
    ];

    if preset.remove_audio {
        args.push("-an".to_string());
    } else {
        args.extend_from_slice(&[
            "-c:a".to_string(), "aac".to_string(), 
            "-b:a".to_string(), "128k".to_string()
        ]);
    }

    let quality_args = preset.quality.get_ffmpeg_args();
    for arg in quality_args {
        args.push(arg.to_string());
    }

    args.extend_from_slice(&["-y".to_string(), output.to_string()]);

    args
}
