import type {
  TextFontStyle,
  TextLayerSettings,
  TextOverlaySettings,
} from "../types/backend";

export type ResolvedTextLayerSettings = Omit<
  TextLayerSettings,
  | "id"
  | "enabled"
  | "fontStyle"
  | "bold"
  | "italic"
  | "underline"
  | "strikethrough"
> & {
  id: string;
  enabled: boolean;
  fontStyle: TextFontStyle;
  bold: boolean;
  italic: boolean;
  underline: boolean;
  strikethrough: boolean;
};

export type ResolvedTextOverlaySettings = {
  panelOpen: boolean;
  layers: ResolvedTextLayerSettings[];
  selectedLayerIds: string[];
};

export const DEFAULT_TEXT_LAYER: ResolvedTextLayerSettings = {
  id: "",
  enabled: true,
  text: "Add Text",
  fontStyle: "clean",
  bold: false,
  italic: false,
  underline: false,
  strikethrough: false,
  fontSize: 48,
  color: "#ffffff",
  opacity: 1,
  x: 0.5,
  y: 0.5,
  outlineEnabled: true,
  outlineColor: "#000000",
  outlineWidth: 3,
};

export const DEFAULT_TEXT_OVERLAY: ResolvedTextOverlaySettings = {
  panelOpen: false,
  layers: [],
  selectedLayerIds: [],
};

export const TEXT_STYLE_PRESETS: Record<
  TextFontStyle,
  Pick<
    TextLayerSettings,
    "fontSize" | "outlineEnabled" | "outlineColor" | "outlineWidth"
  >
> = {
  clean: {
    fontSize: 48,
    outlineEnabled: true,
    outlineColor: "#000000",
    outlineWidth: 3,
  },
  meme: {
    fontSize: 64,
    outlineEnabled: true,
    outlineColor: "#000000",
    outlineWidth: 5,
  },
  minimal: {
    fontSize: 40,
    outlineEnabled: false,
    outlineColor: "#000000",
    outlineWidth: 0,
  },
  caption: {
    fontSize: 44,
    outlineEnabled: true,
    outlineColor: "#000000",
    outlineWidth: 4,
  },
  creator: {
    fontSize: 50,
    outlineEnabled: true,
    outlineColor: "#000000",
    outlineWidth: 3,
  },
  gaming: {
    fontSize: 52,
    outlineEnabled: true,
    outlineColor: "#000000",
    outlineWidth: 4,
  },
  cyberpunk: {
    fontSize: 50,
    outlineEnabled: true,
    outlineColor: "#000000",
    outlineWidth: 3,
  },
  cinematic: {
    fontSize: 54,
    outlineEnabled: true,
    outlineColor: "#000000",
    outlineWidth: 2,
  },
  retro: {
    fontSize: 48,
    outlineEnabled: true,
    outlineColor: "#000000",
    outlineWidth: 3,
  },
  handwritten: {
    fontSize: 58,
    outlineEnabled: true,
    outlineColor: "#000000",
    outlineWidth: 2,
  },
};

export const TEXT_FONT_STYLE_KEYS: TextFontStyle[] = [
  "clean",
  "minimal",
  "caption",
  "meme",
  "creator",
  "gaming",
  "cyberpunk",
  "cinematic",
  "retro",
  "handwritten",
];

const TEXT_FONT_STYLE_SET = new Set<string>(TEXT_FONT_STYLE_KEYS);

export function normalizeTextFontStyle(
  style: TextFontStyle | string | null | undefined,
): TextFontStyle {
  return typeof style === "string" && TEXT_FONT_STYLE_SET.has(style)
    ? (style as TextFontStyle)
    : DEFAULT_TEXT_LAYER.fontStyle;
}

type LegacyTextOverlaySettings = Omit<TextLayerSettings, "id"> & {
  enabled?: boolean;
};

function isTextOverlayContainer(
  overlay: TextOverlaySettings | LegacyTextOverlaySettings,
): overlay is TextOverlaySettings {
  return Array.isArray((overlay as TextOverlaySettings).layers);
}

function legacyLayerId(index: number): string {
  return index === 0 ? "legacy-text-overlay" : `text-layer-${index + 1}`;
}

export function resolveTextLayer(
  layer?: Partial<TextLayerSettings> | null,
  index = 0,
): ResolvedTextLayerSettings {
  return {
    ...DEFAULT_TEXT_LAYER,
    ...layer,
    id: layer?.id?.trim() || legacyLayerId(index),
    enabled: layer?.enabled ?? true,
    text: layer?.text ?? DEFAULT_TEXT_LAYER.text,
    fontStyle: normalizeTextFontStyle(layer?.fontStyle),
    bold: !!layer?.bold,
    italic: !!layer?.italic,
    underline: !!layer?.underline,
    strikethrough: !!layer?.strikethrough,
  };
}

export function resolveTextOverlay(
  overlay?: TextOverlaySettings | LegacyTextOverlaySettings | null,
): ResolvedTextOverlaySettings {
  if (!overlay) return DEFAULT_TEXT_OVERLAY;
  if (isTextOverlayContainer(overlay)) {
    const layers = (overlay.layers ?? []).map((layer, index) =>
      resolveTextLayer(layer, index),
    );
    const layerIds = new Set(layers.map((layer) => layer.id));
    return {
      panelOpen: !!overlay.panelOpen,
      layers,
      selectedLayerIds: (overlay.selectedLayerIds ?? []).filter((id) =>
        layerIds.has(id),
      ),
    };
  }

  const legacyLayer = resolveTextLayer(overlay, 0);
  if (!legacyLayer.enabled || !legacyLayer.text.trim()) {
    return DEFAULT_TEXT_OVERLAY;
  }
  return {
    panelOpen: !!overlay.enabled,
    layers: [legacyLayer],
    selectedLayerIds: legacyLayer.id ? [legacyLayer.id] : [],
  };
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

export function normalizeTextLayer(
  layer?: Partial<TextLayerSettings> | null,
  index = 0,
): ResolvedTextLayerSettings {
  const resolved = resolveTextLayer(layer, index);
  return {
    ...resolved,
    text: Array.from(resolved.text).slice(0, 500).join(""),
    fontSize: Math.round(
      clampFinite(resolved.fontSize, 12, 240, DEFAULT_TEXT_LAYER.fontSize),
    ),
    color: /^#[0-9a-f]{6}$/i.test(resolved.color)
      ? resolved.color
      : DEFAULT_TEXT_LAYER.color,
    opacity: clampFinite(resolved.opacity, 0, 1, DEFAULT_TEXT_LAYER.opacity),
    x: clampFinite(resolved.x, 0, 1, DEFAULT_TEXT_LAYER.x),
    y: clampFinite(resolved.y, 0, 1, DEFAULT_TEXT_LAYER.y),
    outlineColor: /^#[0-9a-f]{6}$/i.test(resolved.outlineColor)
      ? resolved.outlineColor
      : DEFAULT_TEXT_LAYER.outlineColor,
    outlineWidth: Math.round(
      clampFinite(
        resolved.outlineWidth,
        0,
        20,
        DEFAULT_TEXT_LAYER.outlineWidth,
      ),
    ),
  };
}

export function normalizeTextOverlay(
  overlay?: TextOverlaySettings | LegacyTextOverlaySettings | null,
): ResolvedTextOverlaySettings {
  const resolved = resolveTextOverlay(overlay);
  const seen = new Set<string>();
  const layers = resolved.layers.map((layer, index) => {
    const normalized = normalizeTextLayer(layer, index);
    let id = normalized.id;
    if (seen.has(id)) {
      id = `${id}-${index + 1}`;
    }
    seen.add(id);
    return { ...normalized, id };
  });
  const layerIds = new Set(layers.map((layer) => layer.id));
  return {
    panelOpen: resolved.panelOpen,
    layers,
    selectedLayerIds: resolved.selectedLayerIds.filter((id) =>
      layerIds.has(id),
    ),
  };
}
