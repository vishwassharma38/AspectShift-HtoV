use crate::video::preset_adapter::Preset;

fn uses_complex_graph(filter_graph: &str) -> bool {
    let has_named_labels = filter_graph.contains('[') && filter_graph.contains(']');
    let has_multiple_stages = filter_graph.contains(';');
    let has_explicit_input_specifier = filter_graph.contains("[0:") || filter_graph.contains("[1:");

    has_named_labels || has_multiple_stages || has_explicit_input_specifier
}

pub fn build_ffmpeg_args(
    input: &str,
    output: &str,
    filter_graph: &str,
    preset: &Preset
) -> Vec<String> {
    let mut args = vec![
        "-i".to_string(), input.to_string(),
    ];

    if let Some(logo) = &preset.logo {
        args.push("-i".to_string());
        args.push(logo.path.clone());
    }

    let use_filter_complex = uses_complex_graph(filter_graph);

    if use_filter_complex {
        args.push("-filter_complex".to_string());
    } else {
        args.push("-vf".to_string());
    }
    
    args.push(filter_graph.to_string());

    if use_filter_complex {
        args.push("-map".to_string());
        args.push("[v]".to_string());
        
        // Map audio if present
        if !preset.remove_audio {
            args.push("-map".to_string());
            args.push("0:a?".to_string());
        }
    }

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
