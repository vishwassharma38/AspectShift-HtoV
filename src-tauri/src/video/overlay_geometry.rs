//! Phase 1 — Canonical overlay geometry (video-space normalization).
//!
//! One source of truth for freeform overlay position semantics:
//!
//! ```text
//! canonical x / canonical y
//! ```
//!
//! - Normalized relative to the **video frame**, not the preview viewport.
//!   `x = 0.0` → left edge, `0.5` → center, `1.0` → right edge
//!   (likewise `y = 0.0` → top, `1.0` → bottom).
//! - Any **finite** value is valid, including values outside `[0, 1]`
//!   (e.g. `-0.2`, `1.2`). `NaN`/`Infinity` are never valid positions.
//! - Anchor is **center**: the position marks the overlay-box center.
//!   Preview uses `translate(-50%, -50%)`; the renderer uses ASS `\an5` /
//!   `overlay_w/2`. Phase 1 preserves this; it does not redesign anchors.
//! - Automatic subtitles (margin-box layout) are a separate product concept
//!   and are out of scope for this module.
//!
//! Both consumers derive coordinates from the same meaning:
//!
//! ```text
//! previewPx = canonical * previewFrameSize   (frontend `overlayGeometry.ts`)
//! videoPx   = canonical * videoFrameSize     (here)
//! ```
//!
//! Frame-containment boundaries that remain are interaction or product
//! policy, not geometry: unrelated numeric validation (opacity,
//! font size, scale), and automatic-subtitle margin layout. Drag
//! interaction is unbounded (no min/max bounds). The video frame
//! clips visibility; it never bounds these coordinates.

/// A canonical overlay coordinate is valid only when finite.
pub fn is_finite_coordinate(value: f32) -> bool {
    value.is_finite()
}

/// Both axes must be finite; values outside `[0, 1]` are allowed.
pub fn is_finite_position(x: f32, y: f32) -> bool {
    is_finite_coordinate(x) && is_finite_coordinate(y)
}

/// Canonical `x` → video pixels. Pure scale; no clamping.
pub fn to_video_x(x: f32, video_width: u32) -> f32 {
    x * video_width as f32
}

/// Canonical `y` → video pixels. Pure scale; no clamping.
pub fn to_video_y(y: f32, video_height: u32) -> f32 {
    y * video_height as f32
}

/// Canonical `(x, y)` → video pixels. Pure scale; no clamping.
pub fn to_video_position(x: f32, y: f32, video_width: u32, video_height: u32) -> (f32, f32) {
    (to_video_x(x, video_width), to_video_y(y, video_height))
}

/// FFmpeg `overlay` expression for a center-anchored manual X position.
///
/// Emits the existing `main_w*x-overlay_w/2` form so the filter graph keeps
/// bit-identical output while the center-anchor template lives in one place.
/// Takes the already-resolved coordinate (call sites preserve their legacy
/// clamp until Phase 4 removes it).
pub fn overlay_center_x_expression(x: f32) -> String {
    format!("main_w*{x:.6}-overlay_w/2")
}

/// FFmpeg `overlay` expression for a center-anchored manual Y position.
pub fn overlay_center_y_expression(y: f32) -> String {
    format!("main_h*{y:.6}-overlay_h/2")
}

#[cfg(test)]
mod tests {
    use super::{
        is_finite_coordinate, is_finite_position, overlay_center_x_expression,
        overlay_center_y_expression, to_video_position, to_video_x, to_video_y,
    };

    #[test]
    fn centered_overlay_maps_to_frame_center() {
        let (x, y) = to_video_position(0.5, 0.5, 1080, 1920);
        assert!((x - 540.0).abs() < 1e-4);
        assert!((y - 960.0).abs() < 1e-4);
    }

    #[test]
    fn origin_maps_to_top_left() {
        assert_eq!(to_video_x(0.0, 1080), 0.0);
        assert_eq!(to_video_y(0.0, 1920), 0.0);
    }

    #[test]
    fn unit_maps_to_bottom_right() {
        assert_eq!(to_video_x(1.0, 1080), 1080.0);
        assert_eq!(to_video_y(1.0, 1920), 1920.0);
    }

    #[test]
    fn finite_off_canvas_values_reach_video_space_unclamped() {
        // x = -0.1 → 10% of the width left of the frame.
        assert!((to_video_x(-0.1, 1080) - -108.0).abs() < 1e-3);
        // x = 1.1 → 10% beyond the right edge.
        assert!((to_video_x(1.1, 1080) - 1188.0).abs() < 1e-3);
        // y = 1.1 → 10% beyond the bottom edge.
        assert!((to_video_y(1.1, 1920) - 2112.0).abs() < 1e-3);
    }

    #[test]
    fn normalized_meaning_is_resolution_independent() {
        // Same canonical x must yield the same fraction on any frame.
        for width in [720, 1080, 1280, 1920] {
            let px = to_video_x(0.75, width);
            assert!((px / width as f32 - 0.75).abs() < 1e-6);
        }
    }

    #[test]
    fn non_finite_coordinates_are_rejected() {
        assert!(!is_finite_coordinate(f32::NAN));
        assert!(!is_finite_coordinate(f32::INFINITY));
        assert!(!is_finite_coordinate(f32::NEG_INFINITY));
        assert!(is_finite_coordinate(-0.2));
        assert!(is_finite_coordinate(1.2));
        assert!(!is_finite_position(0.5, f32::NAN));
        assert!(is_finite_position(-0.1, 1.1));
    }

    #[test]
    fn overlay_center_expressions_preserve_legacy_filter_form() {
        assert_eq!(
            overlay_center_x_expression(0.5),
            "main_w*0.500000-overlay_w/2"
        );
        assert_eq!(
            overlay_center_y_expression(0.5),
            "main_h*0.500000-overlay_h/2"
        );
        // Off-canvas values format without clamping; clamping (if any)
        // happens at the call site until Phase 4.
        assert_eq!(
            overlay_center_x_expression(-0.1),
            "main_w*-0.100000-overlay_w/2"
        );
    }
}
