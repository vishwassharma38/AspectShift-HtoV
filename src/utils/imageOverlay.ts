import type { ImageOverlay, ImageOverlaySettings } from "../types/backend";

export type ResolvedImageCrop = {
  x: number;
  y: number;
  width: number;
  height: number;
};

export type ResolvedImageOverlay = {
  id: string;
  path: string;
  x: number;
  y: number;
  scale: number;
  rotation: number;
  opacity: number;
  flipHorizontal: boolean;
  flipVertical: boolean;
  crop: ResolvedImageCrop;
};

export type ResolvedImageOverlaySettings = {
  panelOpen: boolean;
  overlays: ResolvedImageOverlay[];
  selectedOverlayId: string | null;
};

export const DEFAULT_IMAGE_CROP: ResolvedImageCrop = {
  x: 0,
  y: 0,
  width: 1,
  height: 1,
};

export const DEFAULT_IMAGE_OVERLAY: ResolvedImageOverlaySettings = {
  panelOpen: false,
  overlays: [],
  selectedOverlayId: null,
};

export function isGifPath(path: string): boolean {
  return path.trim().toLowerCase().endsWith(".gif");
}

function finiteOr(value: number | null | undefined, fallback: number): number {
  const numeric = Number(value);
  return Number.isFinite(numeric) ? numeric : fallback;
}

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

export function normalizeImageCrop(
  crop?: Partial<ResolvedImageCrop> | null,
): ResolvedImageCrop {
  const x = clampFinite(crop?.x, 0, 1, DEFAULT_IMAGE_CROP.x);
  const y = clampFinite(crop?.y, 0, 1, DEFAULT_IMAGE_CROP.y);
  let width = clampFinite(crop?.width, 0.01, 1, DEFAULT_IMAGE_CROP.width);
  let height = clampFinite(crop?.height, 0.01, 1, DEFAULT_IMAGE_CROP.height);
  // Keep the region inside the source image.
  if (x + width > 1) width = Math.max(0.01, 1 - x);
  if (y + height > 1) height = Math.max(0.01, 1 - y);
  return { x, y, width, height };
}

function legacyImageId(index: number): string {
  return `image-${index + 1}`;
}

export function resolveImageOverlay(
  overlay?: Partial<ImageOverlay> | null,
  index = 0,
): ResolvedImageOverlay {
  return {
    id: overlay?.id?.trim() || legacyImageId(index),
    path: overlay?.path ?? "",
    // Canonical, unbounded geometry: any finite x/y is valid (negative, >1).
    // The video frame clips visibility instead of bounding geometry.
    x: finiteOr(overlay?.x, 0.5),
    y: finiteOr(overlay?.y, 0.5),
    scale: finiteOr(overlay?.scale, 0.25),
    rotation: finiteOr(overlay?.rotation, 0),
    opacity: finiteOr(overlay?.opacity, 1),
    flipHorizontal: !!overlay?.flipHorizontal,
    flipVertical: !!overlay?.flipVertical,
    crop: normalizeImageCrop(
      overlay?.crop as Partial<ResolvedImageCrop> | null | undefined,
    ),
  };
}

export function resolveImageOverlaySettings(
  settings?: ImageOverlaySettings | null,
): ResolvedImageOverlaySettings {
  if (!settings) return DEFAULT_IMAGE_OVERLAY;
  const overlays = (settings.overlays ?? []).map((overlay, index) =>
    resolveImageOverlay(overlay, index),
  );
  const ids = new Set(overlays.map((o) => o.id));
  const selectedOverlayId =
    typeof settings.selectedOverlayId === "string" &&
    ids.has(settings.selectedOverlayId)
      ? settings.selectedOverlayId
      : null;
  return {
    panelOpen: !!settings.panelOpen,
    overlays,
    selectedOverlayId,
  };
}

export function normalizeImageOverlay(
  overlay?: Partial<ImageOverlay> | null,
  index = 0,
): ResolvedImageOverlay {
  const resolved = resolveImageOverlay(overlay, index);
  return {
    ...resolved,
    path: resolved.path,
    x: finiteOr(resolved.x, 0.5),
    y: finiteOr(resolved.y, 0.5),
    scale: clampFinite(resolved.scale, 0.01, 10, 0.25),
    rotation: clampFinite(resolved.rotation, -720, 720, 0),
    opacity: clampFinite(resolved.opacity, 0, 1, 1),
    crop: normalizeImageCrop(resolved.crop),
  };
}

export function normalizeImageOverlaySettings(
  settings?: ImageOverlaySettings | null,
): ResolvedImageOverlaySettings {
  const resolved = resolveImageOverlaySettings(settings);
  const seen = new Set<string>();
  const overlays = resolved.overlays.map((overlay, index) => {
    const normalized = normalizeImageOverlay(overlay, index);
    let id = normalized.id;
    if (seen.has(id)) {
      id = `${id}-${index + 1}`;
    }
    seen.add(id);
    return { ...normalized, id };
  });
  const ids = new Set(overlays.map((o) => o.id));
  return {
    panelOpen: resolved.panelOpen,
    overlays,
    selectedOverlayId:
      resolved.selectedOverlayId && ids.has(resolved.selectedOverlayId)
        ? resolved.selectedOverlayId
        : null,
  };
}
