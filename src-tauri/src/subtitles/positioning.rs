pub fn get_subtitle_style(aspect_ratio: f32) -> String {
    let margin_v = if (aspect_ratio - (9.0 / 16.0)).abs() < 0.02 {
        140
    } else if (aspect_ratio - 1.0).abs() < 0.02 {
        110
    } else if (aspect_ratio - (4.0 / 5.0)).abs() < 0.02 || (aspect_ratio - (2.0 / 3.0)).abs() < 0.02
    {
        95
    } else {
        72
    };

    format!("Alignment=2,MarginV={margin_v}")
}
