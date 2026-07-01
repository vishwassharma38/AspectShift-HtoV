import type { SubtitleOverlaySettings } from "../types/backend";
import { normalizeTextFontStyle } from "./textOverlay";

export type ResolvedSubtitleOverlaySettings = Omit<
  SubtitleOverlaySettings,
  "fontStyle" | "fontSize" | "outlineWidth" | "bold" | "italic"
> & {
  fontStyle: NonNullable<SubtitleOverlaySettings["fontStyle"]>;
  bold: boolean;
  italic: boolean;
  fontSize: number | null;
  outlineWidth: number | null;
};

export const DEFAULT_SUBTITLE_OVERLAY: ResolvedSubtitleOverlaySettings = {
  fontStyle: "clean",
  bold: true,
  italic: false,
  fontSize: null,
  color: "#ffffff",
  opacity: 1,
  outlineEnabled: true,
  outlineColor: "#000000",
  outlineWidth: null,
  manualPosition: false,
  x: 0.5,
  y: 0.86,
};

function clampFinite(
  value: number | null | undefined,
  min: number,
  max: number,
  fallback: number,
): number {
  const numeric = Number(value);
  return Number.isFinite(numeric)
    ? Math.max(min, Math.min(max, numeric))
    : fallback;
}

function optionalInt(
  value: number | null | undefined,
  min: number,
  max: number,
): number | null {
  if (value === null || value === undefined) return null;
  return Math.round(clampFinite(value, min, max, min));
}

export function resolveSubtitleOverlay(
  overlay?: Partial<SubtitleOverlaySettings> | null,
): ResolvedSubtitleOverlaySettings {
  return {
    ...DEFAULT_SUBTITLE_OVERLAY,
    ...overlay,
    fontStyle: normalizeTextFontStyle(overlay?.fontStyle),
    bold: overlay?.bold ?? DEFAULT_SUBTITLE_OVERLAY.bold,
    italic: !!overlay?.italic,
    fontSize: overlay?.fontSize ?? DEFAULT_SUBTITLE_OVERLAY.fontSize,
    outlineWidth: overlay?.outlineWidth ?? DEFAULT_SUBTITLE_OVERLAY.outlineWidth,
    manualPosition: !!overlay?.manualPosition,
  };
}

export function normalizeSubtitleOverlay(
  overlay?: Partial<SubtitleOverlaySettings> | null,
): ResolvedSubtitleOverlaySettings {
  const resolved = resolveSubtitleOverlay(overlay);
  return {
    ...resolved,
    fontSize: optionalInt(resolved.fontSize, 12, 240),
    color: /^#[0-9a-f]{6}$/i.test(resolved.color)
      ? resolved.color
      : DEFAULT_SUBTITLE_OVERLAY.color,
    opacity: clampFinite(
      resolved.opacity,
      0,
      1,
      DEFAULT_SUBTITLE_OVERLAY.opacity,
    ),
    x: clampFinite(resolved.x, 0, 1, DEFAULT_SUBTITLE_OVERLAY.x),
    y: clampFinite(resolved.y, 0, 1, DEFAULT_SUBTITLE_OVERLAY.y),
    outlineColor: /^#[0-9a-f]{6}$/i.test(resolved.outlineColor)
      ? resolved.outlineColor
      : DEFAULT_SUBTITLE_OVERLAY.outlineColor,
    outlineWidth: optionalInt(resolved.outlineWidth, 0, 20),
  };
}
