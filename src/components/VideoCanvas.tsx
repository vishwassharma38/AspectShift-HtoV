import React, {
  useMemo,
  useRef,
  useEffect,
  useState,
  useCallback,
  useLayoutEffect,
} from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import type {
  LogoOptions,
  OrientationInfo,
  PreviewRenderLayout,
  SubtitleOverlaySettings,
  TextFontStyle,
  TextLayerSettings,
  TextOverlaySettings,
  VideoEffectsSettings,
} from "../types/backend";
import {
  type FitMode,
  resolveVideoGeometry,
} from "../utils/resolvedVideoGeometry";
import {
  DEFAULT_TEXT_LAYER,
  normalizeTextLayer,
  resolveTextOverlay,
  type ResolvedTextLayerSettings,
  type ResolvedTextOverlaySettings,
} from "../utils/textOverlay";
import {
  normalizeSubtitleOverlay,
  resolveSubtitleOverlay,
  type ResolvedSubtitleOverlaySettings,
} from "../utils/subtitleOverlay";

interface VideoCanvasProps {
  videoSrc: string;
  previewLayout: PreviewRenderLayout | null;
  effects: VideoEffectsSettings;
  onTextOverlayChange?: (textOverlay: TextOverlaySettings) => void;
  onSubtitleOverlayChange?: (subtitleOverlay: SubtitleOverlaySettings) => void;
  onLogoChange?: (logo: LogoOptions) => void;
  orientation: OrientationInfo | null;
  previewVolume: number;
  showGuides?: boolean;
  showSafeFrames?: boolean;
}

const TEXT_FONT_FAMILIES: Record<TextFontStyle, string> = {
  clean: '"AspectShift Text Clean"',
  minimal: '"AspectShift Text Minimal"',
  caption: '"AspectShift Text Caption"',
  meme: '"AspectShift Text Meme"',
  creator: '"AspectShift Text Creator"',
  gaming: '"AspectShift Text Gaming"',
  cyberpunk: '"AspectShift Text Cyberpunk"',
  cinematic: '"AspectShift Text Cinematic"',
  retro: '"AspectShift Text Retro"',
  handwritten: '"AspectShift Text Handwritten"',
};

const RATIO_LABELS: Record<string, string> = {
  "0.5625": "9:16",
  "1": "1:1",
  "0.8": "4:5",
  "0.6666666666666666": "2:3",
  "1.7777777777777777": "16:9",
};

export const VideoCanvas: React.FC<VideoCanvasProps> = ({
  videoSrc,
  previewLayout,
  effects,
  onTextOverlayChange,
  onSubtitleOverlayChange,
  onLogoChange,
  orientation,
  previewVolume,
  showGuides = true,
  showSafeFrames = true,
}) => {
  const containerRef = useRef<HTMLDivElement>(null);
  const canvasBoxRef = useRef<HTMLDivElement>(null);
  const textOverlayRefs = useRef<Record<string, HTMLDivElement | null>>({});
  const [containerDims, setContainerDims] = useState({ width: 0, height: 0 });
  const [editingLayerId, setEditingLayerId] = useState<string | null>(null);
  const textBeforeEditRef = useRef(DEFAULT_TEXT_LAYER.text);
  const textDragRef = useRef<{
    layerId: string;
    pointerId: number;
    startClientX: number;
    startClientY: number;
    startX: number;
    startY: number;
    frameWidth: number;
    frameHeight: number;
    minX: number;
    maxX: number;
    minY: number;
    maxY: number;
    moved: boolean;
  } | null>(null);
  const logoDragRef = useRef<{
    pointerId: number;
    startClientX: number;
    startClientY: number;
    startX: number;
    startY: number;
    frameWidth: number;
    frameHeight: number;
    minX: number;
    maxX: number;
    minY: number;
    maxY: number;
    moved: boolean;
  } | null>(null);
  const subtitleDragRef = useRef<{
    pointerId: number;
    startClientX: number;
    startClientY: number;
    startX: number;
    startY: number;
    frameWidth: number;
    frameHeight: number;
    minX: number;
    maxX: number;
    minY: number;
    maxY: number;
    moved: boolean;
  } | null>(null);
  // Tracks whether the video element has decoded enough to display.
  // Reset to false whenever videoSrc changes so the box stays hidden
  // until canplay fires, preventing a flash of the first frame at the
  // wrong size.
  const [videoReady, setVideoReady] = useState(false);
  const showWhiteBackground = !!effects.whiteBackground;
  const showBlur = !!effects.blur && !showWhiteBackground;
  const showBackgroundEffect = showBlur || showWhiteBackground;
  const textOverlay = useMemo<ResolvedTextOverlaySettings>(
    () => resolveTextOverlay(effects.textOverlay),
    [effects.textOverlay],
  );
  const subtitleOverlay = useMemo<ResolvedSubtitleOverlaySettings>(
    () => resolveSubtitleOverlay(effects.subtitleOverlay),
    [effects.subtitleOverlay],
  );
  const textOverlayStateRef = useRef(textOverlay);
  const subtitleOverlayRef = useRef(subtitleOverlay);
  useLayoutEffect(() => {
    textOverlayStateRef.current = textOverlay;
  }, [textOverlay]);
  useLayoutEffect(() => {
    subtitleOverlayRef.current = subtitleOverlay;
  }, [subtitleOverlay]);
  const setTextOverlayElement = useCallback(
    (layerId: string, element: HTMLDivElement | null) => {
      textOverlayRefs.current[layerId] = element;
      if (editingLayerId === layerId) return;
      if (element) {
        const layer = textOverlayStateRef.current.layers.find(
          (candidate) => candidate.id === layerId,
        );
        if (layer && element.textContent !== layer.text) {
          element.textContent = layer.text;
        }
      }
    },
    [editingLayerId],
  );
  const mainVideoRef = useRef<HTMLVideoElement | null>(null);
  const backgroundVideoRef = useRef<HTMLVideoElement | null>(null);
  const foregroundVideoRef = useRef<HTMLVideoElement | null>(null);

  const forceBlurBackgroundMuted = useCallback(
    (el: HTMLVideoElement | null = backgroundVideoRef.current) => {
      if (!el) return;
      el.defaultMuted = true;
      el.muted = true;
      el.volume = 0;
    },
    [],
  );

  const setBackgroundVideoRef = useCallback(
    (el: HTMLVideoElement | null) => {
      backgroundVideoRef.current = el;
      forceBlurBackgroundMuted(el);
    },
    [forceBlurBackgroundMuted],
  );

  // Reset readiness every time the source or preview media mode changes.
  useEffect(() => {
    setVideoReady(false);
  }, [videoSrc, showBlur, showWhiteBackground]);

  useEffect(() => {
    const normalized = Math.max(0, Math.min(100, previewVolume)) / 100;
    const isMuted = normalized <= 0;
    const syncElement = (el: HTMLVideoElement | null) => {
      if (!el) return;
      el.volume = normalized;
      el.muted = isMuted;
    };
    syncElement(mainVideoRef.current);
    syncElement(foregroundVideoRef.current);
    forceBlurBackgroundMuted();
  }, [previewVolume, videoSrc, showBackgroundEffect, forceBlurBackgroundMuted]);

  const syncBlurBackgroundToForeground = useCallback(() => {
    if (!showBlur) return;
    const bg = backgroundVideoRef.current;
    const fg = foregroundVideoRef.current;
    if (!bg || !fg || !Number.isFinite(fg.currentTime)) return;

    forceBlurBackgroundMuted(bg);
    bg.playbackRate = fg.playbackRate;
    if (Math.abs(bg.currentTime - fg.currentTime) > 0.05) {
      bg.currentTime = fg.currentTime;
    }
    if (fg.paused && !bg.paused) {
      bg.pause();
    } else if (!fg.paused && bg.paused) {
      void bg.play().catch(() => {});
    }
    forceBlurBackgroundMuted(bg);
  }, [showBlur, forceBlurBackgroundMuted]);

  useEffect(() => {
    syncBlurBackgroundToForeground();
  }, [syncBlurBackgroundToForeground, videoSrc, showBlur]);

  // Update container dims on resize
  useEffect(() => {
    if (!containerRef.current) return;
    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) {
        setContainerDims({
          width: entry.contentRect.width,
          height: entry.contentRect.height,
        });
      }
    });
    observer.observe(containerRef.current);
    return () => observer.disconnect();
  }, []);

  // Calculate the actual size of the canvas box within the container
  const canvasSize = useMemo(() => {
    const { width, height } = containerDims;
    // layout=null means geometry is not yet known — return zero so the box
    // collapses to nothing while we wait for orientation data.
    if (width === 0 || height === 0 || !previewLayout)
      return { width: 0, height: 0 };

    const ratio = previewLayout.targetWidth / previewLayout.targetHeight;
    const containerRatio = width / height;
    if (containerRatio > ratio) {
      // Container is wider than the target ratio -> height is the limiting factor
      return { width: height * ratio, height };
    } else {
      // Container is taller than the target ratio -> width is the limiting factor
      return { width, height: width / ratio };
    }
  }, [containerDims, previewLayout]);

  const targetScale = useMemo(() => {
    if (!previewLayout || previewLayout.targetWidth <= 0) return 1;
    return canvasSize.width / previewLayout.targetWidth;
  }, [canvasSize.width, previewLayout]);

  const fgFrameSize = useMemo(() => {
    if (!previewLayout) return { width: 0, height: 0 };
    return {
      width: previewLayout.foregroundFrameWidth * targetScale,
      height: previewLayout.foregroundFrameHeight * targetScale,
    };
  }, [previewLayout, targetScale]);

  const resolvedCoverGeometry = useMemo(() => {
    if (!previewLayout) return null;
    const fitMode: FitMode =
      previewLayout.backgroundFit === "contain" ? "contain" : "cover";
    return resolveVideoGeometry({
      orientation,
      transform: effects.transform,
      targetAspectRatio: previewLayout.targetWidth / previewLayout.targetHeight,
      frameWidth: canvasSize.width,
      frameHeight: canvasSize.height,
      fitMode,
    });
  }, [previewLayout, orientation, effects.transform, canvasSize]);

  const resolvedForegroundGeometry = useMemo(() => {
    if (!previewLayout) return null;
    const fitMode: FitMode =
      previewLayout.foregroundFit === "contain" ? "contain" : "cover";
    return resolveVideoGeometry({
      orientation,
      transform: effects.transform,
      targetAspectRatio:
        previewLayout.foregroundFrameWidth / previewLayout.foregroundFrameHeight,
      frameWidth: fgFrameSize.width,
      frameHeight: fgFrameSize.height,
      fitMode,
    });
  }, [previewLayout, orientation, effects.transform, fgFrameSize]);

  const transformStyle = useMemo(() => {
    const rotate = resolvedCoverGeometry?.rotation ?? 0;
    const flipH = !!effects.transform?.flip_h;
    const flipV = !!effects.transform?.flip_v;
    let t = `translate(-50%, -50%) rotate(${rotate}deg)`;
    if (flipH) t += " scaleX(-1)";
    if (flipV) t += " scaleY(-1)";
    return {
      transform: t,
      transformOrigin: "center center",
    };
  }, [
    resolvedCoverGeometry?.rotation,
    effects.transform?.flip_h,
    effects.transform?.flip_v,
  ]);

  const logoStyle = useMemo(() => {
    if (!effects.logo || !effects.logo.enabled || !effects.logo.path)
      return null;
    const { position, opacity, gap } = effects.logo;

    if (!previewLayout) return null;
    if (previewLayout.logoWidth === null || previewLayout.logoGap === null) {
      return null;
    }
    const logoWidth = (previewLayout.logoWidth ?? 0) * targetScale;
    const scaledGap = (previewLayout.logoGap ?? gap) * targetScale;

    const style: React.CSSProperties = {
      position: "absolute",
      width: logoWidth,
      opacity,
      transition: effects.logo.manualPosition ? "none" : "all 0.2s ease",
      zIndex: 10,
      cursor: "grab",
      userSelect: "none",
      touchAction: "none",
    };

    if (effects.logo.manualPosition) {
      style.left = `${(effects.logo.x ?? 0.5) * 100}%`;
      style.top = `${(effects.logo.y ?? 0.5) * 100}%`;
      style.transform = "translate(-50%, -50%)";
    } else {
      switch (position) {
        case "top_left":
          style.top = scaledGap;
          style.left = scaledGap;
          break;
        case "top_right":
          style.top = scaledGap;
          style.right = scaledGap;
          break;
        case "bottom_left":
          style.bottom = scaledGap;
          style.left = scaledGap;
          break;
        case "bottom_right":
          style.bottom = scaledGap;
          style.right = scaledGap;
          break;
      }
    }

    return style;
  }, [effects.logo, previewLayout, targetScale]);

  const handleLogoPointerDown = useCallback(
    (event: React.PointerEvent<HTMLImageElement>) => {
      if (event.button !== 0 || !effects.logo?.enabled) return;
      const frame = canvasBoxRef.current;
      if (!frame) return;
      event.preventDefault();
      event.stopPropagation();

      const frameRect = frame.getBoundingClientRect();
      const logoRect = event.currentTarget.getBoundingClientRect();
      if (frameRect.width <= 0 || frameRect.height <= 0) return;
      const minX = Math.min(0.5, logoRect.width / (2 * frameRect.width));
      const minY = Math.min(0.5, logoRect.height / (2 * frameRect.height));
      const startX = effects.logo.manualPosition
        ? effects.logo.x ?? 0.5
        : (logoRect.left + logoRect.width / 2 - frameRect.left) /
          frameRect.width;
      const startY = effects.logo.manualPosition
        ? effects.logo.y ?? 0.5
        : (logoRect.top + logoRect.height / 2 - frameRect.top) /
          frameRect.height;

      logoDragRef.current = {
        pointerId: event.pointerId,
        startClientX: event.clientX,
        startClientY: event.clientY,
        startX,
        startY,
        frameWidth: frameRect.width,
        frameHeight: frameRect.height,
        minX,
        maxX: 1 - minX,
        minY,
        maxY: 1 - minY,
        moved: false,
      };
      event.currentTarget.setPointerCapture(event.pointerId);
    },
    [effects.logo],
  );

  const handleLogoPointerMove = useCallback(
    (event: React.PointerEvent<HTMLImageElement>) => {
      const drag = logoDragRef.current;
      if (!drag || drag.pointerId !== event.pointerId || !effects.logo) return;
      const deltaX = event.clientX - drag.startClientX;
      const deltaY = event.clientY - drag.startClientY;
      if (!drag.moved && Math.hypot(deltaX, deltaY) >= 3) {
        drag.moved = true;
      }
      if (!drag.moved) return;
      event.preventDefault();
      onLogoChange?.({
        ...effects.logo,
        manualPosition: true,
        x: Math.max(
          drag.minX,
          Math.min(drag.maxX, drag.startX + deltaX / drag.frameWidth),
        ),
        y: Math.max(
          drag.minY,
          Math.min(drag.maxY, drag.startY + deltaY / drag.frameHeight),
        ),
      });
    },
    [effects.logo, onLogoChange],
  );

  const handleLogoPointerEnd = useCallback(
    (event: React.PointerEvent<HTMLImageElement>) => {
      const drag = logoDragRef.current;
      if (!drag || drag.pointerId !== event.pointerId) return;
      logoDragRef.current = null;
      if (event.currentTarget.hasPointerCapture(event.pointerId)) {
        event.currentTarget.releasePointerCapture(event.pointerId);
      }
    },
    [],
  );

  const applyTextOverlay = useCallback(
    (next: ResolvedTextOverlaySettings) => {
      onTextOverlayChange?.(next);
    },
    [onTextOverlayChange],
  );

  const updateSubtitleOverlay = useCallback(
    (patch: Partial<SubtitleOverlaySettings>) => {
      const current = subtitleOverlayRef.current;
      onSubtitleOverlayChange?.(
        normalizeSubtitleOverlay({ ...current, ...patch }),
      );
    },
    [onSubtitleOverlayChange],
  );

  const updateTextLayer = useCallback(
    (layerId: string, patch: Partial<TextLayerSettings>) => {
      const current = textOverlayStateRef.current;
      applyTextOverlay({
        ...current,
        layers: current.layers.map((layer) =>
          layer.id === layerId ? normalizeTextLayer({ ...layer, ...patch }) : layer,
        ),
      });
    },
    [applyTextOverlay],
  );

  const readEditableText = useCallback((element: HTMLDivElement | null) => {
    return Array.from(element?.textContent ?? "")
      .slice(0, 500)
      .join("");
  }, []);

  const selectTextLayer = useCallback(
    (layerId: string, event: React.PointerEvent<HTMLDivElement>) => {
      const current = textOverlayStateRef.current;
      const selected = new Set(current.selectedLayerIds);
      let selectedLayerIds: string[];
      if (event.ctrlKey || event.metaKey) {
        if (selected.has(layerId)) {
          selected.delete(layerId);
        } else {
          selected.add(layerId);
        }
        selectedLayerIds = Array.from(selected);
      } else if (event.shiftKey) {
        selected.add(layerId);
        selectedLayerIds = Array.from(selected);
      } else {
        selectedLayerIds = [layerId];
      }
      applyTextOverlay({ ...current, selectedLayerIds });
    },
    [applyTextOverlay],
  );

  const beginTextEditing = useCallback((layerId: string) => {
    const layer = textOverlayStateRef.current.layers.find(
      (candidate) => candidate.id === layerId,
    );
    if (!layer?.enabled) return;
    textBeforeEditRef.current = layer.text;
    setEditingLayerId(layerId);
    requestAnimationFrame(() => {
      const element = textOverlayRefs.current[layerId];
      if (!element) return;
      element.focus();
      if (layer.text === DEFAULT_TEXT_LAYER.text) {
        element.textContent = "";
        updateTextLayer(layerId, { text: "" });
        return;
      }
      const selection = window.getSelection();
      const range = document.createRange();
      range.selectNodeContents(element);
      selection?.removeAllRanges();
      selection?.addRange(range);
    });
  }, [updateTextLayer]);

  const commitTextEditing = useCallback(() => {
    if (!editingLayerId) return;
    const element = textOverlayRefs.current[editingLayerId];
    const nextText = readEditableText(element);
    updateTextLayer(editingLayerId, {
      text: nextText.trim() ? nextText : "",
    });
    setEditingLayerId(null);
    requestAnimationFrame(() => element?.blur());
  }, [editingLayerId, readEditableText, updateTextLayer]);

  const cancelTextEditing = useCallback(() => {
    if (!editingLayerId) return;
    const element = textOverlayRefs.current[editingLayerId];
    updateTextLayer(editingLayerId, { text: textBeforeEditRef.current });
    setEditingLayerId(null);
    requestAnimationFrame(() => element?.blur());
  }, [editingLayerId, updateTextLayer]);

  useLayoutEffect(() => {
    for (const layer of textOverlay.layers) {
      const element = textOverlayRefs.current[layer.id];
      if (!element || editingLayerId === layer.id) continue;
      if (element.textContent !== layer.text) {
        element.textContent = layer.text;
      }
    }
  }, [editingLayerId, textOverlay.layers]);

  useEffect(() => {
    if (
      editingLayerId &&
      !textOverlay.layers.some((layer) => layer.id === editingLayerId)
    ) {
      setEditingLayerId(null);
      textDragRef.current = null;
    }
  }, [editingLayerId, textOverlay.layers]);

  const clampTextToFrame = useCallback(() => {
    const frame = canvasBoxRef.current;
    const current = textOverlayStateRef.current;
    if (!frame) return;
    const frameRect = frame.getBoundingClientRect();
    if (frameRect.width <= 0 || frameRect.height <= 0) return;

    let changed = false;
    const layers = current.layers.map((layer) => {
      const element = textOverlayRefs.current[layer.id];
      if (!element || !layer.enabled) return layer;
      const textRect = element.getBoundingClientRect();
      const minX = Math.min(0.5, textRect.width / (2 * frameRect.width));
      const minY = Math.min(0.5, textRect.height / (2 * frameRect.height));
      const x = Math.max(minX, Math.min(1 - minX, layer.x));
      const y = Math.max(minY, Math.min(1 - minY, layer.y));
      if (Math.abs(x - layer.x) <= 0.0005 && Math.abs(y - layer.y) <= 0.0005) {
        return layer;
      }
      changed = true;
      return { ...layer, x, y };
    });
    if (changed) {
      applyTextOverlay({ ...current, layers });
    }
  }, [applyTextOverlay]);

  useLayoutEffect(() => {
    clampTextToFrame();
  }, [
    clampTextToFrame,
    canvasSize.width,
    canvasSize.height,
    textOverlay.layers,
  ]);

  const handleTextPointerDown = useCallback(
    (event: React.PointerEvent<HTMLDivElement>, layer: ResolvedTextLayerSettings) => {
      if (editingLayerId || event.button !== 0) return;
      const frame = canvasBoxRef.current;
      if (!frame) return;
      event.preventDefault();
      event.stopPropagation();
      selectTextLayer(layer.id, event);

      const frameRect = frame.getBoundingClientRect();
      const textRect = event.currentTarget.getBoundingClientRect();
      if (frameRect.width <= 0 || frameRect.height <= 0) return;
      const minX = Math.min(0.5, textRect.width / (2 * frameRect.width));
      const minY = Math.min(0.5, textRect.height / (2 * frameRect.height));
      textDragRef.current = {
        layerId: layer.id,
        pointerId: event.pointerId,
        startClientX: event.clientX,
        startClientY: event.clientY,
        startX: layer.x,
        startY: layer.y,
        frameWidth: frameRect.width,
        frameHeight: frameRect.height,
        minX,
        maxX: 1 - minX,
        minY,
        maxY: 1 - minY,
        moved: false,
      };
      event.currentTarget.setPointerCapture(event.pointerId);
    },
    [editingLayerId, selectTextLayer],
  );

  const handleTextPointerMove = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      const drag = textDragRef.current;
      if (!drag || drag.pointerId !== event.pointerId) return;
      const deltaX = event.clientX - drag.startClientX;
      const deltaY = event.clientY - drag.startClientY;
      if (!drag.moved && Math.hypot(deltaX, deltaY) >= 3) {
        drag.moved = true;
      }
      if (!drag.moved) return;
      event.preventDefault();
      const x = Math.max(
        drag.minX,
        Math.min(drag.maxX, drag.startX + deltaX / drag.frameWidth),
      );
      const y = Math.max(
        drag.minY,
        Math.min(drag.maxY, drag.startY + deltaY / drag.frameHeight),
      );
      updateTextLayer(drag.layerId, { x, y });
    },
    [updateTextLayer],
  );

  const handleTextPointerEnd = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      const drag = textDragRef.current;
      if (!drag || drag.pointerId !== event.pointerId) return;
      const shouldEdit = !drag.moved && event.type === "pointerup";
      textDragRef.current = null;
      if (event.currentTarget.hasPointerCapture(event.pointerId)) {
        event.currentTarget.releasePointerCapture(event.pointerId);
      }
      if (shouldEdit && !(event.ctrlKey || event.metaKey || event.shiftKey)) {
        beginTextEditing(drag.layerId);
      }
    },
    [beginTextEditing],
  );

  const getTextLayerStyle = useCallback((layer: ResolvedTextLayerSettings): React.CSSProperties => {
    const isEditing = editingLayerId === layer.id;
    const fontSize = Math.max(8, layer.fontSize * targetScale);
    const outlineWidth = layer.outlineEnabled
      ? Math.max(0, layer.outlineWidth * targetScale)
      : 0;
    return {
      position: "absolute",
      left: `${layer.x * 100}%`,
      top: `${layer.y * 100}%`,
      transform: "translate(-50%, -50%)",
      maxWidth: "100%",
      color: layer.color,
      opacity: layer.opacity,
      fontFamily: TEXT_FONT_FAMILIES[layer.fontStyle],
      fontSize,
      fontWeight: layer.bold ? 700 : 400,
      fontStyle: layer.italic ? "italic" : "normal",
      fontSynthesis: "weight style",
      textDecorationLine: [
        layer.underline ? "underline" : "",
        layer.strikethrough ? "line-through" : "",
      ]
        .filter(Boolean)
        .join(" ") || "none",
      textDecorationColor: "currentColor",
      textDecorationThickness: "0.08em",
      lineHeight:
        layer.fontStyle === "meme" || layer.fontStyle === "retro"
          ? 0.95
          : layer.fontStyle === "handwritten"
            ? 1.05
            : 1.15,
      letterSpacing:
        layer.fontStyle === "minimal"
          ? fontSize * 0.04
          : layer.fontStyle === "cyberpunk" || layer.fontStyle === "gaming"
            ? fontSize * 0.03
            : undefined,
      textAlign: "center",
      direction: "ltr",
      unicodeBidi: "plaintext",
      whiteSpace: "pre-wrap",
      overflowWrap: "anywhere",
      WebkitTextStroke:
        outlineWidth > 0
          ? `${outlineWidth}px ${layer.outlineColor}`
          : undefined,
      paintOrder: "stroke fill",
      zIndex: 15,
      cursor: isEditing ? "text" : "grab",
      userSelect: isEditing ? "text" : "none",
      touchAction: "none",
    };
  }, [editingLayerId, targetScale]);

  const handleSubtitlePointerDown = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      if (event.button !== 0) return;
      const frame = canvasBoxRef.current;
      if (!frame) return;
      event.preventDefault();
      event.stopPropagation();

      const frameRect = frame.getBoundingClientRect();
      const subtitleRect = event.currentTarget.getBoundingClientRect();
      if (frameRect.width <= 0 || frameRect.height <= 0) return;
      const minX = Math.min(0.5, subtitleRect.width / (2 * frameRect.width));
      const minY = Math.min(0.5, subtitleRect.height / (2 * frameRect.height));
      const current = subtitleOverlayRef.current;
      const startX = current.manualPosition
        ? current.x
        : (subtitleRect.left + subtitleRect.width / 2 - frameRect.left) /
          frameRect.width;
      const startY = current.manualPosition
        ? current.y
        : (subtitleRect.top + subtitleRect.height / 2 - frameRect.top) /
          frameRect.height;

      subtitleDragRef.current = {
        pointerId: event.pointerId,
        startClientX: event.clientX,
        startClientY: event.clientY,
        startX,
        startY,
        frameWidth: frameRect.width,
        frameHeight: frameRect.height,
        minX,
        maxX: 1 - minX,
        minY,
        maxY: 1 - minY,
        moved: false,
      };
      event.currentTarget.setPointerCapture(event.pointerId);
    },
    [],
  );

  const handleSubtitlePointerMove = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      const drag = subtitleDragRef.current;
      if (!drag || drag.pointerId !== event.pointerId) return;
      const deltaX = event.clientX - drag.startClientX;
      const deltaY = event.clientY - drag.startClientY;
      if (!drag.moved && Math.hypot(deltaX, deltaY) >= 3) {
        drag.moved = true;
      }
      if (!drag.moved) return;
      event.preventDefault();
      updateSubtitleOverlay({
        manualPosition: true,
        x: Math.max(
          drag.minX,
          Math.min(drag.maxX, drag.startX + deltaX / drag.frameWidth),
        ),
        y: Math.max(
          drag.minY,
          Math.min(drag.maxY, drag.startY + deltaY / drag.frameHeight),
        ),
      });
    },
    [updateSubtitleOverlay],
  );

  const handleSubtitlePointerEnd = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      const drag = subtitleDragRef.current;
      if (!drag || drag.pointerId !== event.pointerId) return;
      subtitleDragRef.current = null;
      if (event.currentTarget.hasPointerCapture(event.pointerId)) {
        event.currentTarget.releasePointerCapture(event.pointerId);
      }
    },
    [],
  );

  const subtitleStyle = useMemo(() => {
    if (!previewLayout) return null;
    const subtitleScale = Math.max(0.001, canvasSize.height / previewLayout.subtitle.playResY);
    const fontSize =
      (subtitleOverlay.fontSize ?? previewLayout.subtitle.fontSize) *
      subtitleScale;
    const marginV = previewLayout.subtitle.marginV * subtitleScale;
    const marginH = previewLayout.subtitle.marginH * subtitleScale;
    const outlineWidth =
      (subtitleOverlay.outlineEnabled
        ? (subtitleOverlay.outlineWidth ?? previewLayout.subtitle.outline)
        : 0) * subtitleScale;

    const style: React.CSSProperties = {
      position: "absolute",
      ...(subtitleOverlay.manualPosition
        ? {
            left: `${subtitleOverlay.x * 100}%`,
            top: `${subtitleOverlay.y * 100}%`,
            transform: "translate(-50%, -50%)",
            maxWidth: `calc(100% - ${marginH * 2}px)`,
          }
        : {
            bottom: marginV,
            left: marginH,
            right: marginH,
          }),
      textAlign: "center",
      color: subtitleOverlay.color,
      opacity: subtitleOverlay.opacity,
      fontSize: Math.max(12, fontSize),
      fontWeight: subtitleOverlay.bold ? 700 : 400,
      fontStyle: subtitleOverlay.italic ? "italic" : "normal",
      fontFamily: TEXT_FONT_FAMILIES[subtitleOverlay.fontStyle],
      fontSynthesis: "weight style",
      zIndex: 20,
      pointerEvents: "auto",
      lineHeight: 1.2,
      cursor: "grab",
      userSelect: "none",
      touchAction: "none",
      // Replicate ASS outline with multiple shadows for better coverage
      textShadow: outlineWidth > 0 ? `
        -${outlineWidth}px -${outlineWidth}px 0 ${subtitleOverlay.outlineColor},
         ${outlineWidth}px -${outlineWidth}px 0 ${subtitleOverlay.outlineColor},
        -${outlineWidth}px  ${outlineWidth}px 0 ${subtitleOverlay.outlineColor},
         ${outlineWidth}px  ${outlineWidth}px 0 ${subtitleOverlay.outlineColor},
         0px ${outlineWidth}px 0 ${subtitleOverlay.outlineColor},
         0px -${outlineWidth}px 0 ${subtitleOverlay.outlineColor},
         ${outlineWidth}px 0px 0 ${subtitleOverlay.outlineColor},
        -${outlineWidth}px 0px 0 ${subtitleOverlay.outlineColor}
      ` : "none",
    };

    return style;
  }, [previewLayout, canvasSize, subtitleOverlay]);

  return (
    <div
      ref={containerRef}
      className="video-canvas-container"
      style={{
        width: "100%",
        height: "100%",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        position: "relative",
      }}
    >
      {videoSrc ? (
        // layout===null means orientation invoke hasn't resolved yet.
        // Render nothing so the layout never snaps through a wrong aspect ratio.
        previewLayout !== null && resolvedCoverGeometry && (
          <div
            ref={canvasBoxRef}
            className="video-canvas-box"
            style={{
              width: canvasSize.width,
              height: canvasSize.height,
              position: "relative",
              overflow: "hidden",
              backgroundColor: showWhiteBackground ? "#fff" : "#000",
              borderRadius: "10px",
              // Only animate width/height after the video is ready to avoid
              // the layout-shift frame being visible during ratio transitions.
              transition: "width 0.3s ease, height 0.3s ease",
              // Fade the entire box in once the video can play. This keeps the
              // preview blank while metadata is loading without any layout jump.
              opacity: videoReady ? 1 : 0,
              transitionProperty: "width, height, opacity",
              transitionDuration: "0.3s, 0.3s, 0.25s",
              transitionTimingFunction: "ease, ease, ease-in",
            }}
          >
            {/* Main Video Layer */}
            {showBackgroundEffect ? (
              <>
                {showBlur && (
                  <video
                    key={`blur-bg-${videoSrc}`}
                    src={convertFileSrc(videoSrc)}
                    className="canvas-video-blur"
                    ref={setBackgroundVideoRef}
                    style={{
                      position: "absolute",
                      left: "50%",
                      top: "50%",
                      width:
                        resolvedCoverGeometry.sourceWidth *
                        resolvedCoverGeometry.scale,
                      height:
                        resolvedCoverGeometry.sourceHeight *
                        resolvedCoverGeometry.scale,
                      objectFit: "fill",
                      filter: `blur(${previewLayout.blurSigma}px)`,
                      ...transformStyle,
                    }}
                    autoPlay
                    muted
                    loop
                    playsInline
                    onLoadedMetadata={(e) => {
                      forceBlurBackgroundMuted(e.currentTarget);
                      syncBlurBackgroundToForeground();
                    }}
                    onCanPlay={(e) => {
                      forceBlurBackgroundMuted(e.currentTarget);
                      syncBlurBackgroundToForeground();
                    }}
                    onPlay={(e) => forceBlurBackgroundMuted(e.currentTarget)}
                    onVolumeChange={(e) =>
                      forceBlurBackgroundMuted(e.currentTarget)
                    }
                  />
                )}
                <video
                  key={`${showBlur ? "blur" : "white"}-fg-${videoSrc}`}
                  src={convertFileSrc(videoSrc)}
                  className="canvas-video-fg"
                  ref={foregroundVideoRef}
                  style={{
                    position: "absolute",
                    left: "50%",
                    top: "50%",
                    width:
                      (resolvedForegroundGeometry ?? resolvedCoverGeometry)
                        .sourceWidth *
                      (resolvedForegroundGeometry ?? resolvedCoverGeometry).scale,
                    height:
                      (resolvedForegroundGeometry ?? resolvedCoverGeometry)
                        .sourceHeight *
                      (resolvedForegroundGeometry ?? resolvedCoverGeometry).scale,
                    objectFit: "fill",
                    zIndex: 2,
                    ...transformStyle,
                  }}
                  autoPlay
                  loop
                  playsInline
                  onPlay={syncBlurBackgroundToForeground}
                  onPause={syncBlurBackgroundToForeground}
                  onSeeking={syncBlurBackgroundToForeground}
                  onSeeked={syncBlurBackgroundToForeground}
                  onRateChange={syncBlurBackgroundToForeground}
                  onTimeUpdate={syncBlurBackgroundToForeground}
                  onCanPlay={(e) => {
                    const normalized = Math.max(0, Math.min(100, previewVolume)) / 100;
                    const isMuted = normalized <= 0;
                    e.currentTarget.volume = normalized;
                    e.currentTarget.muted = isMuted;
                    setVideoReady(true);
                    syncBlurBackgroundToForeground();
                  }}
                />
              </>
            ) : (
              <video
                src={convertFileSrc(videoSrc)}
                className="canvas-video-main"
                ref={mainVideoRef}
                style={{
                  position: "absolute",
                  left: "50%",
                  top: "50%",
                  width:
                    resolvedCoverGeometry.sourceWidth *
                    resolvedCoverGeometry.scale,
                  height:
                    resolvedCoverGeometry.sourceHeight *
                    resolvedCoverGeometry.scale,
                  objectFit: "fill",
                  ...transformStyle,
                }}
                autoPlay
                loop
                playsInline
                onCanPlay={(e) => {
                  const normalized = Math.max(0, Math.min(100, previewVolume)) / 100;
                  const isMuted = normalized <= 0;
                  e.currentTarget.volume = normalized;
                  e.currentTarget.muted = isMuted;
                  setVideoReady(true);
                }}
              />
            )}

            {/* Logo Layer */}
            {effects.logo?.enabled && effects.logo.path && logoStyle && (
              <img
                src={convertFileSrc(effects.logo.path)}
                style={logoStyle}
                alt="Logo"
                draggable={false}
                onPointerDown={handleLogoPointerDown}
                onPointerMove={handleLogoPointerMove}
                onPointerUp={handleLogoPointerEnd}
                onPointerCancel={handleLogoPointerEnd}
              />
            )}

            {/* Editable Text Layers */}
            {textOverlay.layers
              .filter((layer) => layer.enabled)
              .map((layer) => {
                const isEditing = editingLayerId === layer.id;
                const isSelected = textOverlay.selectedLayerIds.includes(
                  layer.id,
                );
                const isEmpty = !layer.text.trim();
                return (
                  <div
                    key={layer.id}
                    ref={(element) => setTextOverlayElement(layer.id, element)}
                    className={`canvas-text-overlay${isEditing ? " is-editing" : ""}${isSelected ? " is-selected" : ""}${isEmpty ? " is-empty" : ""}`}
                    data-placeholder={DEFAULT_TEXT_LAYER.text}
                    style={getTextLayerStyle(layer)}
                    contentEditable={isEditing}
                    suppressContentEditableWarning
                    role="textbox"
                    aria-label="Video text overlay"
                    aria-multiline="true"
                    aria-selected={isSelected}
                    tabIndex={0}
                    onPointerDown={(event) =>
                      handleTextPointerDown(event, layer)
                    }
                    onPointerMove={handleTextPointerMove}
                    onPointerUp={handleTextPointerEnd}
                    onPointerCancel={handleTextPointerEnd}
                    onInput={(event) => {
                      const nextText = Array.from(
                        event.currentTarget.textContent ?? "",
                      )
                        .slice(0, 500)
                        .join("");
                      if (
                        Array.from(event.currentTarget.textContent ?? "")
                          .length > 500
                      ) {
                        event.currentTarget.textContent = nextText;
                      }
                      updateTextLayer(layer.id, { text: nextText });
                    }}
                    onKeyDown={(event) => {
                      if (
                        !isEditing &&
                        (event.key === "Enter" || event.key === " ")
                      ) {
                        event.preventDefault();
                        beginTextEditing(layer.id);
                      } else if (event.key === "Enter" && !event.shiftKey) {
                        event.preventDefault();
                        commitTextEditing();
                      } else if (isEditing && event.key === "Escape") {
                        event.preventDefault();
                        cancelTextEditing();
                      }
                    }}
                    onBlur={() => {
                      if (isEditing) commitTextEditing();
                    }}
                    onClick={(event) => event.stopPropagation()}
                  />
                );
              })}

            {/* Subtitles Layer */}
            {(effects.exportSubtitles || effects.burnSubtitles) && (
              <div
                className="canvas-subtitles"
                style={subtitleStyle!}
                onPointerDown={handleSubtitlePointerDown}
                onPointerMove={handleSubtitlePointerMove}
                onPointerUp={handleSubtitlePointerEnd}
                onPointerCancel={handleSubtitlePointerEnd}
              >
                [ Subtitles Preview ]
              </div>
            )}

            {/* Composition Guides */}
            {showGuides && (
              <div
                className="canvas-guides"
                style={{
                  position: "absolute",
                  inset: 0,
                  pointerEvents: "none",
                  zIndex: 30,
                  opacity: 0.3,
                }}
              >
                <div
                  style={{
                    position: "absolute",
                    left: "33.33%",
                    top: 0,
                    bottom: 0,
                    width: "1px",
                    borderLeft: "1px dashed white",
                  }}
                />
                <div
                  style={{
                    position: "absolute",
                    left: "66.66%",
                    top: 0,
                    bottom: 0,
                    width: "1px",
                    borderLeft: "1px dashed white",
                  }}
                />
                <div
                  style={{
                    position: "absolute",
                    top: "33.33%",
                    left: 0,
                    right: 0,
                    height: "1px",
                    borderTop: "1px dashed white",
                  }}
                />
                <div
                  style={{
                    position: "absolute",
                    top: "66.66%",
                    left: 0,
                    right: 0,
                    height: "1px",
                    borderTop: "1px dashed white",
                  }}
                />
              </div>
            )}

            {/* Safe Frame Guides */}
            {showSafeFrames && (
              <div
                className="canvas-safe-frames"
                style={{
                  position: "absolute",
                  inset: 0,
                  pointerEvents: "none",
                  zIndex: 31,
                  opacity: 0.2,
                }}
              >
                {/* 90% Safe Area */}
                <div
                  style={{
                    position: "absolute",
                    inset: "5%",
                    border: "1px solid white",
                    borderRadius: "2px",
                  }}
                />
                {/* 80% Safe Area */}
                <div
                  style={{
                    position: "absolute",
                    inset: "10%",
                    border: "1px solid rgba(255,255,255,0.5)",
                    borderRadius: "2px",
                  }}
                />
              </div>
            )}

            {/* Label Overlay */}
            <div
              style={{
                position: "absolute",
                top: 10,
                left: 10,
                padding: "2px 6px",
                background: "rgba(0,0,0,0.6)",
                color: "white",
                fontSize: 10,
                borderRadius: 3,
                fontFamily: "var(--font-mono)",
                zIndex: 40,
              }}
            >
              {RATIO_LABELS[
                (previewLayout.targetWidth / previewLayout.targetHeight).toString()
              ] ||
                (previewLayout.targetWidth / previewLayout.targetHeight).toFixed(2)}
            </div>
          </div>
        )
      ) : (
        <div className="preview-empty">
          <svg
            xmlns="http://www.w3.org/2000/svg"
            width="48"
            height="48"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.5"
            strokeLinecap="round"
            strokeLinejoin="round"
            className="lucide lucide-clapperboard-icon lucide-clapperboard"
            style={{ color: 'var(--accent)', opacity: 0.5, marginBottom: '10px' }}
          >
            <path d="m12.296 3.464 3.02 3.956" />
            <path d="M20.2 6 3 11l-.9-2.4c-.3-1.1.3-2.2 1.3-2.5l13.5-4c1.1-.3 2.2.3 2.5 1.3z" />
            <path d="M3 11h18v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
            <path d="m6.18 5.276 3.1 3.899" />
          </svg>
          <div className="preview-empty-text">No video selected</div>
        </div>
      )}
    </div>
  );
};
