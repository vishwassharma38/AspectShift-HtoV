import React from "react";
import { VideoCanvas } from "./VideoCanvas";
import type { PreviewMode } from "../services/previewController";

/**
 * PreviewHost — the single hosting layer for the single preview system.
 *
 * HARD ARCHITECTURAL INVARIANT:
 *
 * > There must only ever be one active preview renderer for the current
 * > project. The pop-out window is a different host for the same preview
 * > system, not a second preview implementation.
 *
 * `PreviewHost` renders the SAME `VideoCanvas` implementation in every
 * display mode (`embedded` / `popout` / `fullscreen`). It owns no video,
 * overlay, playback, or geometry logic of its own — it only decides *where*
 * the shared preview renderer is mounted. Only one host is active at a time:
 * the embedded preview is hidden/inactive while the pop-out hosts the preview.
 *
 * Do NOT create a `PopOutVideoCanvas` or any second canvas/rendering
 * implementation; add future preview/overlay features to `VideoCanvas` and
 * both hosts inherit them automatically.
 */

type VideoCanvasProps = React.ComponentProps<typeof VideoCanvas>;

export interface PreviewHostProps extends VideoCanvasProps {
  mode: PreviewMode;
}

export const PreviewHost: React.FC<PreviewHostProps> = ({
  mode,
  ...canvasProps
}) => {
  return (
    <div
      className="preview-host"
      data-preview-mode={mode}
      data-testid={`preview-host-${mode}`}
      style={{ width: "100%", height: "100%", minHeight: 0 }}
    >
      <VideoCanvas {...canvasProps} />
    </div>
  );
};
