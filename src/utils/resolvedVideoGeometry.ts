import type { OrientationInfo, VideoTransform } from "../types/backend";

export type FitMode = "cover" | "contain";

export interface ResolvedVideoGeometry {
  sourceWidth: number;
  sourceHeight: number;
  rotation: number;
  quarterTurns: 0 | 1 | 2 | 3;
  effectiveWidth: number;
  effectiveHeight: number;
  displayAspectRatio: number;
  targetAspectRatio: number;
  fitMode: FitMode;
  scale: number;
  frameWidth: number;
  frameHeight: number;
  renderedWidth: number;
  renderedHeight: number;
  cropBounds: { x: number; y: number; width: number; height: number };
}

function toQuarterTurns(rotationDeg: number): 0 | 1 | 2 | 3 {
  const normalized = ((rotationDeg % 360) + 360) % 360;
  const turns = Math.round(normalized / 90) % 4;
  return turns as 0 | 1 | 2 | 3;
}

export function getSourceDisplaySize(
  orientation: OrientationInfo | null,
): { width: number; height: number } | null {
  if (!orientation) return null;
  return {
    width: Math.max(1, orientation.displayWidth),
    height: Math.max(1, orientation.displayHeight),
  };
}

export function resolveVideoGeometry(input: {
  orientation: OrientationInfo | null;
  transform: VideoTransform | null | undefined;
  targetAspectRatio: number;
  frameWidth: number;
  frameHeight: number;
  fitMode?: FitMode;
}): ResolvedVideoGeometry | null {
  const src = getSourceDisplaySize(input.orientation);
  if (!src || !isFinite(input.targetAspectRatio) || input.targetAspectRatio <= 0)
    return null;
  if (input.frameWidth <= 0 || input.frameHeight <= 0) return null;

  const quarterTurns = toQuarterTurns(input.transform?.rotate ?? 0);
  const rotation = quarterTurns * 90;
  const swapsDimensions = quarterTurns === 1 || quarterTurns === 3;

  const effectiveWidth = swapsDimensions ? src.height : src.width;
  const effectiveHeight = swapsDimensions ? src.width : src.height;
  const displayAspectRatio = effectiveWidth / effectiveHeight;

  const fitMode = input.fitMode ?? "cover";
  const scale =
    fitMode === "contain"
      ? Math.min(input.frameWidth / effectiveWidth, input.frameHeight / effectiveHeight)
      : Math.max(input.frameWidth / effectiveWidth, input.frameHeight / effectiveHeight);

  const renderedWidth = effectiveWidth * scale;
  const renderedHeight = effectiveHeight * scale;

  return {
    sourceWidth: src.width,
    sourceHeight: src.height,
    rotation,
    quarterTurns,
    effectiveWidth,
    effectiveHeight,
    displayAspectRatio,
    targetAspectRatio: input.targetAspectRatio,
    fitMode,
    scale,
    frameWidth: input.frameWidth,
    frameHeight: input.frameHeight,
    renderedWidth,
    renderedHeight,
    cropBounds: {
      x: Math.max(0, (renderedWidth - input.frameWidth) / 2),
      y: Math.max(0, (renderedHeight - input.frameHeight) / 2),
      width: input.frameWidth,
      height: input.frameHeight,
    },
  };
}
