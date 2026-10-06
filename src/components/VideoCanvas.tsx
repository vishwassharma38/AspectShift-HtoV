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
  ImageOverlaySettings,
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
  normalizeImageOverlaySettings,
  resolveImageOverlaySettings,
  type ResolvedImageOverlay,
  type ResolvedImageOverlaySettings,
} from "../utils/imageOverlay";
import {
  normalizeSubtitleOverlay,
  resolveSubtitleOverlay,
  type ResolvedSubtitleOverlaySettings,
} from "../utils/subtitleOverlay";
import {
  pointerAngleDeg,
  previewDeltaToCanonical,
  resizeHandleDeltaPx,
  rotationDeltaDeg,
  toPreviewFontSize,
  toPreviewPercent,
  type OverlayResizeHandle,
} from "../utils/overlayGeometry";

interface VideoCanvasProps {
  videoSrc: string;
  previewLayout: PreviewRenderLayout | null;
  effects: VideoEffectsSettings;
  onTextOverlayChange?: (textOverlay: TextOverlaySettings) => void;
  onSubtitleOverlayChange?: (subtitleOverlay: SubtitleOverlaySettings) => void;
  onImageOverlayChange?: (imageOverlay: ImageOverlaySettings) => void;
  orientation: OrientationInfo | null;
  previewVolume: number;
  showGuides?: boolean;
  showSafeFrames?: boolean;
  /**
   * Controlled preview playback state (single source of truth owned by the
   * preview controller). `playing` defaults to true: the preview plays by
   * default when opened. The same state is used across embedded / pop-out /
   * fullscreen display modes so mode switches preserve playing/paused,
   * volume, rate, and position.
   */
  playing?: boolean;
  playbackRate?: number;
  /** Position to restore when (re)mounting the shared renderer in a host. */
  initialTime?: number;
  onTimeUpdate?: (currentTime: number) => void;
}

type ImageResizeHandle = OverlayResizeHandle;

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
  onImageOverlayChange,
  orientation,
  previewVolume,
  showGuides = true,
  showSafeFrames = true,
  playing = true,
  playbackRate = 1,
  initialTime,
  onTimeUpdate,
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
    moved: boolean;
  } | null>(null);
  const textResizeRef = useRef<{
    layerId: string;
    handle: OverlayResizeHandle;
    pointerId: number;
    startClientX: number;
    startClientY: number;
    startFontSize: number;
    startWidthPx: number;
  } | null>(null);
  const textRotateRef = useRef<{
    layerId: string;
    pointerId: number;
    startAngleDeg: number;
    startRotation: number;
    centerX: number;
    centerY: number;
  } | null>(null);
  const imageDragRef = useRef<{
    overlayId: string;
    pointerId: number;
    startClientX: number;
    startClientY: number;
    startX: number;
    startY: number;
    frameWidth: number;
    frameHeight: number;
    moved: boolean;
  } | null>(null);
  const imageResizeRef = useRef<{
    overlayId: string;
    handle: ImageResizeHandle;
    pointerId: number;
    startClientX: number;
    startClientY: number;
    startScale: number;
    frameWidth: number;
  } | null>(null);
  const imageRotateRef = useRef<{
    overlayId: string;
    pointerId: number;
    startAngleDeg: number;
    startRotation: number;
    centerX: number;
    centerY: number;
  } | null>(null);
  const subtitleDragRef = useRef<{
    pointerId: number;
    startClientX: number;
    startClientY: number;
    startX: number;
    startY: number;
    frameWidth: number;
    frameHeight: number;
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
  const imageOverlay = useMemo<ResolvedImageOverlaySettings>(
    () => resolveImageOverlaySettings(effects.imageOverlay),
    [effects.imageOverlay],
  );
  const subtitleOverlay = useMemo<ResolvedSubtitleOverlaySettings>(
    () => resolveSubtitleOverlay(effects.subtitleOverlay),
    [effects.subtitleOverlay],
  );
  const textOverlayStateRef = useRef(textOverlay);
  const imageOverlayStateRef = useRef(imageOverlay);
  const subtitleOverlayRef = useRef(subtitleOverlay);
  useLayoutEffect(() => {
    textOverlayStateRef.current = textOverlay;
  }, [textOverlay]);
  useLayoutEffect(() => {
    imageOverlayStateRef.current = imageOverlay;
  }, [imageOverlay]);
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

  // Controlled preview playback: one playback state owned by the preview
  // controller. Multiple <video> elements exist only because of the
  // blur/background rendering arrangement; they all synchronize from the
  // same `playing` / `playbackRate` / `previewVolume` state so embedded and
  // pop-out hosts never drift.
  //
  // PLAYBACK OWNERSHIP: this component is a pure renderer, never the clock
  // owner. `playing` / `playbackRate` / `initialTime` arrive as props from
  // the single authoritative owner (main-window preview state, transferred
  // with the active host). There is no internal playing state, no polling,
  // no `setInterval`, no `requestVideoFrameCallback` loop, and no
  // `currentTime` drift-correction between windows. `onTimeUpdate` only
  // reports the renderer's position for one-time handoff snapshots (open /
  // Apply / Cancel); it never drives a second clock. Only one host mounts
  // this renderer at a time, so only one `<video>` decoder ever advances.
  // `autoPlay` follows the authoritative `playing` prop so a paused owner
  // never flashes playing on mount.
  const onTimeUpdateRef = useRef(onTimeUpdate);
  useLayoutEffect(() => {
    onTimeUpdateRef.current = onTimeUpdate;
  }, [onTimeUpdate]);
  const reportTime = useCallback((currentTime: number) => {
    if (Number.isFinite(currentTime)) onTimeUpdateRef.current?.(currentTime);
  }, []);
  const restoreTimeRef = useRef<number | null>(null);
  useEffect(() => {
    restoreTimeRef.current =
      typeof initialTime === "number" && Number.isFinite(initialTime)
        ? initialTime
        : null;
  }, [videoSrc, initialTime]);

  // One-time handoff correction: if the authoritative `initialTime` changes
  // after the video is already ready (e.g. the pop-out's final position
  // arrives just after the embedded host remounts on Apply/Cancel), seek
  // once to the authoritative position. This is NOT a sync loop: it fires
  // only on discrete `initialTime` prop changes, performs a single bounded
  // seek when the gap is material (>0.35s), and never polls or re-renders
  // per frame.
  const lastHandoffTimeRef = useRef<number | null>(null);
  useEffect(() => {
    if (!videoReady) return;
    if (typeof initialTime !== "number" || !Number.isFinite(initialTime)) return;
    if (lastHandoffTimeRef.current === initialTime) return;
    lastHandoffTimeRef.current = initialTime;
    const targets = [mainVideoRef.current, foregroundVideoRef.current].filter(
      (el): el is HTMLVideoElement => !!el,
    );
    for (const el of targets) {
      try {
        if (!Number.isFinite(el.currentTime)) continue;
        if (Math.abs(el.currentTime - initialTime) > 0.35) {
          el.currentTime = Math.max(0, initialTime);
        }
      } catch {
        // Seeking an unready element is best-effort; canPlay restore covers it.
      }
    }
  }, [initialTime, videoReady]);

  const applyPlaybackState = useCallback(
    (el: HTMLVideoElement | null) => {
      if (!el) return;
      try {
        el.playbackRate = playbackRate;
      } catch {
        // playbackRate is best-effort on some media states.
      }
      if (playing) {
        if (el.paused) void el.play().catch(() => {});
      } else if (!el.paused) {
        el.pause();
      }
    },
    [playing, playbackRate],
  );

  useEffect(() => {
    applyPlaybackState(mainVideoRef.current);
    applyPlaybackState(foregroundVideoRef.current);
    // The blur background strictly follows the foreground element.
    syncBlurBackgroundToForeground();
    if (!playing) {
      const bg = backgroundVideoRef.current;
      if (bg && !bg.paused) bg.pause();
    }
  }, [playing, playbackRate, videoSrc, applyPlaybackState, syncBlurBackgroundToForeground]);

  const handlePlaybackCanPlay = useCallback(
    (event: React.SyntheticEvent<HTMLVideoElement>) => {
      const el = event.currentTarget;
      const pending = restoreTimeRef.current;
      if (pending !== null && Number.isFinite(pending)) {
        try {
          const duration = el.duration;
          if (
            !Number.isFinite(duration) ||
            duration <= 0 ||
            pending <= duration + 0.25
          ) {
            el.currentTime = Math.max(0, pending);
          }
        } catch {
          // Seeking before metadata is fully ready is best-effort.
        }
        restoreTimeRef.current = null;
      }
      const normalized = Math.max(0, Math.min(100, previewVolume)) / 100;
      const isMuted = normalized <= 0;
      el.volume = normalized;
      el.muted = isMuted;
      applyPlaybackState(el);
      setVideoReady(true);
      syncBlurBackgroundToForeground();
    },
    [
      applyPlaybackState,
      previewVolume,
      syncBlurBackgroundToForeground,
    ],
  );

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

  const applyImageOverlay = useCallback(
    (next: ResolvedImageOverlaySettings) => {
      onImageOverlayChange?.(normalizeImageOverlaySettings(next));
    },
    [onImageOverlayChange],
  );

  const applyTextOverlay = useCallback(
    (next: ResolvedTextOverlaySettings) => {
      onTextOverlayChange?.(next);
    },
    [onTextOverlayChange],
  );

  const updateImageOverlay = useCallback(
    (overlayId: string, patch: Partial<ResolvedImageOverlay>) => {
      const current = imageOverlayStateRef.current;
      applyImageOverlay({
        ...current,
        overlays: current.overlays.map((overlay) =>
          overlay.id === overlayId ? { ...overlay, ...patch } : overlay,
        ),
      });
    },
    [applyImageOverlay],
  );

  const deselectTextLayers = useCallback(() => {
    const current = textOverlayStateRef.current;
    if (current.selectedLayerIds.length === 0) return;
    applyTextOverlay({ ...current, selectedLayerIds: [] });
  }, [applyTextOverlay]);

  const selectImageOverlay = useCallback(
    (overlayId: string) => {
      // Cross-type exclusivity: selecting an image clears any text
      // selection so keyboard shortcuts and the bounding box always follow
      // the single currently selected overlay.
      const currentText = textOverlayStateRef.current;
      if (currentText.selectedLayerIds.length > 0) {
        applyTextOverlay({ ...currentText, selectedLayerIds: [] });
      }
      const current = imageOverlayStateRef.current;
      if (current.selectedOverlayId === overlayId) return;
      applyImageOverlay({ ...current, selectedOverlayId: overlayId });
    },
    [applyImageOverlay, applyTextOverlay],
  );

  const deselectImageOverlay = useCallback(() => {
    const current = imageOverlayStateRef.current;
    if (current.selectedOverlayId === null) return;
    applyImageOverlay({ ...current, selectedOverlayId: null });
  }, [applyImageOverlay]);

  const handleCanvasBoxPointerDown = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      // Fires only for empty canvas space: image, handle, rotation, text,
      // and subtitle pointer handlers all stop propagation. Clears both
      // image and text selection (same deselect model); overlay transform
      // state is untouched, so overlays stay visible in place with their
      // handles hidden.
      if (event.button !== 0) return;
      deselectImageOverlay();
      deselectTextLayers();
    },
    [deselectImageOverlay, deselectTextLayers],
  );

  useEffect(() => {
    // App-wide dismissal: a click anywhere outside the selected overlays'
    // interaction context clears the selection so bounding boxes/handles
    // hide. Interaction context = (A) the selected overlay element itself
    // (image, handles, rotation, text content, wrapper, handles, rotation)
    // or (B) the selected object's own settings panel
    // (`.image-settings-panel` for images, `.text-settings-panel` for
    // text). This is a native bubble-phase window listener with no
    // preventDefault/stopPropagation, so it runs after React synthetic
    // onClick handlers and never blocks panel controls: each control
    // performs its action first, then selection is (or isn't) dismissed.
    // Overlay transform state is untouched. Image and text selections clear
    // independently through their own state, so an image-panel click keeps
    // the image but still dismisses text, and vice versa.
    const onWindowClick = (event: MouseEvent) => {
      const target = event.target;
      const inOverlay =
        target instanceof Element &&
        target.closest(
          ".canvas-image-overlay, .canvas-text-overlay, .canvas-text-overlay-wrap",
        );
      if (inOverlay) return;
      const inImagePanel =
        target instanceof Element &&
        target.closest(".image-settings-panel");
      const inTextPanel =
        target instanceof Element && target.closest(".text-settings-panel");
      const currentImage = imageOverlayStateRef.current;
      if (currentImage.selectedOverlayId !== null && !inImagePanel) {
        applyImageOverlay({ ...currentImage, selectedOverlayId: null });
      }
      const currentText = textOverlayStateRef.current;
      if (currentText.selectedLayerIds.length > 0 && !inTextPanel) {
        applyTextOverlay({ ...currentText, selectedLayerIds: [] });
      }
    };
    window.addEventListener("click", onWindowClick);
    return () => window.removeEventListener("click", onWindowClick);
  }, [applyImageOverlay, applyTextOverlay]);

  const getImageWrapperStyle = useCallback(
    (overlay: ResolvedImageOverlay): React.CSSProperties => {
      // Canonical video-space position → preview percent. Center anchor.
      // Unbounded: x/y may be outside 0..1; the frame clips visibility.
      const preview = toPreviewPercent({ x: overlay.x, y: overlay.y });
      return {
        position: "absolute",
        left: `${preview.xPercent}%`,
        top: `${preview.yPercent}%`,
        width: `${overlay.scale * 100}%`,
        transform: `translate(-50%, -50%) rotate(${overlay.rotation}deg)`,
        transformOrigin: "center center",
        zIndex: 10,
        cursor: "grab",
        userSelect: "none",
        touchAction: "none",
      };
    },
    [],
  );

  const getImageElementStyle = useCallback(
    (overlay: ResolvedImageOverlay): React.CSSProperties => {
      const crop = overlay.crop;
      const insetTop = crop.y * 100;
      const insetLeft = crop.x * 100;
      const insetRight = Math.max(0, (1 - crop.x - crop.width) * 100);
      const insetBottom = Math.max(0, (1 - crop.y - crop.height) * 100);
      const transforms: string[] = [];
      if (overlay.flipHorizontal) transforms.push("scaleX(-1)");
      if (overlay.flipVertical) transforms.push("scaleY(-1)");
      return {
        display: "block",
        width: "100%",
        height: "auto",
        opacity: overlay.opacity,
        transform: transforms.length > 0 ? transforms.join(" ") : undefined,
        transformOrigin: "center center",
        clipPath:
          crop.x !== 0 ||
          crop.y !== 0 ||
          crop.width !== 1 ||
          crop.height !== 1
            ? `inset(${insetTop}% ${insetRight}% ${insetBottom}% ${insetLeft}%)`
            : undefined,
        pointerEvents: "none",
        userSelect: "none",
        draggable: false,
      } as React.CSSProperties;
    },
    [],
  );

  const handleImagePointerDown = useCallback(
    (event: React.PointerEvent<HTMLDivElement>, overlay: ResolvedImageOverlay) => {
      if (event.button !== 0) return;
      const frame = canvasBoxRef.current;
      if (!frame) return;
      event.preventDefault();
      event.stopPropagation();
      selectImageOverlay(overlay.id);

      const frameRect = frame.getBoundingClientRect();
      if (frameRect.width <= 0 || frameRect.height <= 0) return;
      // Unbounded drag: screen-px delta → canonical delta with no clamping.
      imageDragRef.current = {
        overlayId: overlay.id,
        pointerId: event.pointerId,
        startClientX: event.clientX,
        startClientY: event.clientY,
        startX: overlay.x,
        startY: overlay.y,
        frameWidth: frameRect.width,
        frameHeight: frameRect.height,
        moved: false,
      };
      event.currentTarget.setPointerCapture(event.pointerId);
    },
    [selectImageOverlay],
  );

  const handleImagePointerMove = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      const drag = imageDragRef.current;
      if (!drag || drag.pointerId !== event.pointerId) return;
      const deltaX = event.clientX - drag.startClientX;
      const deltaY = event.clientY - drag.startClientY;
      if (!drag.moved && Math.hypot(deltaX, deltaY) >= 3) {
        drag.moved = true;
      }
      if (!drag.moved) return;
      event.preventDefault();
      const x =
        drag.startX + previewDeltaToCanonical(deltaX, drag.frameWidth);
      const y =
        drag.startY + previewDeltaToCanonical(deltaY, drag.frameHeight);
      updateImageOverlay(drag.overlayId, { x, y });
    },
    [updateImageOverlay],
  );

  const handleImagePointerEnd = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      const drag = imageDragRef.current;
      if (!drag || drag.pointerId !== event.pointerId) return;
      imageDragRef.current = null;
      if (event.currentTarget.hasPointerCapture(event.pointerId)) {
        event.currentTarget.releasePointerCapture(event.pointerId);
      }
    },
    [],
  );

  const handleImageResizePointerDown = useCallback(
    (event: React.PointerEvent<HTMLDivElement>, overlay: ResolvedImageOverlay, handle: ImageResizeHandle) => {
      if (event.button !== 0) return;
      const frame = canvasBoxRef.current;
      if (!frame) return;
      event.preventDefault();
      event.stopPropagation();
      selectImageOverlay(overlay.id);
      const frameRect = frame.getBoundingClientRect();
      if (frameRect.width <= 0) return;
      imageResizeRef.current = {
        overlayId: overlay.id,
        handle,
        pointerId: event.pointerId,
        startClientX: event.clientX,
        startClientY: event.clientY,
        startScale: overlay.scale,
        frameWidth: frameRect.width,
      };
      event.currentTarget.setPointerCapture(event.pointerId);
    },
    [selectImageOverlay],
  );

  const handleImageResizePointerMove = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      const resize = imageResizeRef.current;
      if (!resize || resize.pointerId !== event.pointerId) return;
      const current = imageOverlayStateRef.current.overlays.find(
        (o) => o.id === resize.overlayId,
      );
      if (!current) return;
      const dx = event.clientX - resize.startClientX;
      const dy = event.clientY - resize.startClientY;
      // Shared handle geometry (same as text): edge/corner drag → box-size
      // delta. Only the canonical mapping stays domain-specific (image
      // `scale` vs text `fontSize`); aspect is preserved by width-only sizing.
      const deltaPx = resizeHandleDeltaPx(resize.handle, dx, dy);
      // Bounding-box manipulation is the source of scale changes.
      // Only scale changes; aspect is preserved by width-only sizing.
      const nextScale = Math.max(
        0.01,
        Math.min(10, resize.startScale + deltaPx / resize.frameWidth),
      );
      event.preventDefault();
      updateImageOverlay(resize.overlayId, { scale: nextScale });
    },
    [updateImageOverlay],
  );

  const handleImageResizePointerEnd = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      const resize = imageResizeRef.current;
      if (!resize || resize.pointerId !== event.pointerId) return;
      imageResizeRef.current = null;
      if (event.currentTarget.hasPointerCapture(event.pointerId)) {
        event.currentTarget.releasePointerCapture(event.pointerId);
      }
    },
    [],
  );

  const handleImageRotatePointerDown = useCallback(
    (event: React.PointerEvent<HTMLDivElement>, overlay: ResolvedImageOverlay) => {
      if (event.button !== 0) return;
      const frame = canvasBoxRef.current;
      if (!frame) return;
      event.preventDefault();
      event.stopPropagation();
      selectImageOverlay(overlay.id);
      const frameRect = frame.getBoundingClientRect();
      if (frameRect.width <= 0 || frameRect.height <= 0) return;
      const centerX = frameRect.left + overlay.x * frameRect.width;
      const centerY = frameRect.top + overlay.y * frameRect.height;
      // Shared rotation geometry (same as text): pointer angle around the
      // canonical center. Domain-specific part is only which object we write.
      const startAngleDeg = pointerAngleDeg(
        event.clientX,
        event.clientY,
        centerX,
        centerY,
      );
      imageRotateRef.current = {
        overlayId: overlay.id,
        pointerId: event.pointerId,
        startAngleDeg,
        startRotation: overlay.rotation,
        centerX,
        centerY,
      };
      event.currentTarget.setPointerCapture(event.pointerId);
    },
    [selectImageOverlay],
  );

  const handleImageRotatePointerMove = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      const rotate = imageRotateRef.current;
      if (!rotate || rotate.pointerId !== event.pointerId) return;
      const currentAngleDeg = pointerAngleDeg(
        event.clientX,
        event.clientY,
        rotate.centerX,
        rotate.centerY,
      );
      // Shared shortest-sweep delta, same as text rotation.
      const delta = rotationDeltaDeg(rotate.startAngleDeg, currentAngleDeg);
      let nextRotation = rotate.startRotation + delta;
      // Keep within validation bounds.
      if (nextRotation > 720) nextRotation = 720;
      if (nextRotation < -720) nextRotation = -720;
      event.preventDefault();
      updateImageOverlay(rotate.overlayId, { rotation: nextRotation });
    },
    [updateImageOverlay],
  );

  const handleImageRotatePointerEnd = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      const rotate = imageRotateRef.current;
      if (!rotate || rotate.pointerId !== event.pointerId) return;
      imageRotateRef.current = null;
      if (event.currentTarget.hasPointerCapture(event.pointerId)) {
        event.currentTarget.releasePointerCapture(event.pointerId);
      }
    },
    [],
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
      // Cross-type exclusivity (mirror of selectImageOverlay): any text
      // interaction clears the image selection so only one overlay type is
      // ever active. Intra-text ctrl/shift multi-select is preserved.
      const currentImage = imageOverlayStateRef.current;
      if (currentImage.selectedOverlayId !== null) {
        applyImageOverlay({ ...currentImage, selectedOverlayId: null });
      }
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
    [applyImageOverlay, applyTextOverlay],
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

  const handleTextPointerDown = useCallback(
    (event: React.PointerEvent<HTMLDivElement>, layer: ResolvedTextLayerSettings) => {
      if (editingLayerId || event.button !== 0) return;
      const frame = canvasBoxRef.current;
      if (!frame) return;
      event.preventDefault();
      event.stopPropagation();
      selectTextLayer(layer.id, event);

      const frameRect = frame.getBoundingClientRect();
      if (frameRect.width <= 0 || frameRect.height <= 0) return;
      // Unbounded overlay geometry: no min/max drag bounds. The overlay
      // element is positioned by its canonical center; the frame only
      // clips visibility. Overlay size is never used as a drag boundary.
      textDragRef.current = {
        layerId: layer.id,
        pointerId: event.pointerId,
        startClientX: event.clientX,
        startClientY: event.clientY,
        startX: layer.x,
        startY: layer.y,
        frameWidth: frameRect.width,
        frameHeight: frameRect.height,
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
      // Unbounded drag: screen-px delta → canonical delta with no frame
      // clamping. Any finite x/y (negative, >1, arbitrarily far outside)
      // is valid and never snaps back.
      const x =
        drag.startX + previewDeltaToCanonical(deltaX, drag.frameWidth);
      const y =
        drag.startY + previewDeltaToCanonical(deltaY, drag.frameHeight);
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

  const selectSingleTextLayer = useCallback(
    (layerId: string) => {
      const currentImage = imageOverlayStateRef.current;
      if (currentImage.selectedOverlayId !== null) {
        applyImageOverlay({ ...currentImage, selectedOverlayId: null });
      }
      const current = textOverlayStateRef.current;
      if (
        current.selectedLayerIds.length === 1 &&
        current.selectedLayerIds[0] === layerId &&
        currentImage.selectedOverlayId === null
      ) {
        return;
      }
      applyTextOverlay({ ...current, selectedLayerIds: [layerId] });
    },
    [applyImageOverlay, applyTextOverlay],
  );

  const handleTextResizePointerDown = useCallback(
    (
      event: React.PointerEvent<HTMLDivElement>,
      layer: ResolvedTextLayerSettings,
      handle: OverlayResizeHandle,
    ) => {
      if (event.button !== 0) return;
      event.preventDefault();
      event.stopPropagation();
      // Keep an existing multi-selection intact; otherwise single-select the
      // resized layer, mirroring canvas click selection. Any image selection
      // is cleared so only one overlay type stays active.
      const currentImageForResize = imageOverlayStateRef.current;
      if (currentImageForResize.selectedOverlayId !== null) {
        applyImageOverlay({
          ...currentImageForResize,
          selectedOverlayId: null,
        });
      }
      const current = textOverlayStateRef.current;
      if (!current.selectedLayerIds.includes(layer.id)) {
        selectSingleTextLayer(layer.id);
      }
      const element = textOverlayRefs.current[layer.id];
      const startWidthPx = element?.offsetWidth ?? 0;
      if (startWidthPx <= 0) return;
      textResizeRef.current = {
        layerId: layer.id,
        handle,
        pointerId: event.pointerId,
        startClientX: event.clientX,
        startClientY: event.clientY,
        startFontSize: layer.fontSize,
        startWidthPx,
      };
      event.currentTarget.setPointerCapture(event.pointerId);
    },
    [applyImageOverlay, selectSingleTextLayer],
  );

  const handleTextResizePointerMove = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      const resize = textResizeRef.current;
      if (!resize || resize.pointerId !== event.pointerId) return;
      const dx = event.clientX - resize.startClientX;
      const dy = event.clientY - resize.startClientY;
      // Approach 1 — font-size scaling: the box width maps proportionally
      // onto the canonical fontSize, so glyphs, outline, spacing, and
      // wrapping keep their normal typography instead of being stretched.
      // Bounds (12..240) are enforced by normalizeTextLayer on write.
      const nextWidthPx = Math.max(8, resize.startWidthPx + resizeHandleDeltaPx(resize.handle, dx, dy));
      const nextFontSize = Math.round(
        (resize.startFontSize * nextWidthPx) / resize.startWidthPx,
      );
      event.preventDefault();
      updateTextLayer(resize.layerId, { fontSize: nextFontSize });
    },
    [updateTextLayer],
  );

  const handleTextResizePointerEnd = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      const resize = textResizeRef.current;
      if (!resize || resize.pointerId !== event.pointerId) return;
      textResizeRef.current = null;
      if (event.currentTarget.hasPointerCapture(event.pointerId)) {
        event.currentTarget.releasePointerCapture(event.pointerId);
      }
    },
    [],
  );

  const handleTextRotatePointerDown = useCallback(
    (
      event: React.PointerEvent<HTMLDivElement>,
      layer: ResolvedTextLayerSettings,
    ) => {
      if (event.button !== 0) return;
      const frame = canvasBoxRef.current;
      if (!frame) return;
      event.preventDefault();
      event.stopPropagation();
      // Rotating a text layer clears any image selection (exclusive
      // selection); existing text multi-selection is otherwise preserved.
      const currentImageForRotate = imageOverlayStateRef.current;
      if (currentImageForRotate.selectedOverlayId !== null) {
        applyImageOverlay({
          ...currentImageForRotate,
          selectedOverlayId: null,
        });
      }
      const current = textOverlayStateRef.current;
      if (!current.selectedLayerIds.includes(layer.id)) {
        selectSingleTextLayer(layer.id);
      }
      const frameRect = frame.getBoundingClientRect();
      if (frameRect.width <= 0 || frameRect.height <= 0) return;
      const centerX = frameRect.left + layer.x * frameRect.width;
      const centerY = frameRect.top + layer.y * frameRect.height;
      textRotateRef.current = {
        layerId: layer.id,
        pointerId: event.pointerId,
        startAngleDeg: pointerAngleDeg(
          event.clientX,
          event.clientY,
          centerX,
          centerY,
        ),
        startRotation: layer.rotation,
        centerX,
        centerY,
      };
      event.currentTarget.setPointerCapture(event.pointerId);
    },
    [applyImageOverlay, selectSingleTextLayer],
  );

  const handleTextRotatePointerMove = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      const rotate = textRotateRef.current;
      if (!rotate || rotate.pointerId !== event.pointerId) return;
      const currentAngleDeg = pointerAngleDeg(
        event.clientX,
        event.clientY,
        rotate.centerX,
        rotate.centerY,
      );
      let nextRotation =
        rotate.startRotation + rotationDeltaDeg(rotate.startAngleDeg, currentAngleDeg);
      // Same bounds as image rotation; enforced again by normalization.
      if (nextRotation > 720) nextRotation = 720;
      if (nextRotation < -720) nextRotation = -720;
      event.preventDefault();
      updateTextLayer(rotate.layerId, { rotation: nextRotation });
    },
    [updateTextLayer],
  );

  const handleTextRotatePointerEnd = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      const rotate = textRotateRef.current;
      if (!rotate || rotate.pointerId !== event.pointerId) return;
      textRotateRef.current = null;
      if (event.currentTarget.hasPointerCapture(event.pointerId)) {
        event.currentTarget.releasePointerCapture(event.pointerId);
      }
    },
    [],
  );

  const getTextWrapperStyle = useCallback(
    (layer: ResolvedTextLayerSettings): React.CSSProperties => {
      // Phase 1: canonical video-space position → preview percent, center
      // anchor shared with every overlay type. Rotation applies on the same
      // center anchor, mirroring the image wrapper. Unbounded: x/y may be
      // outside 0..1; the frame clips visibility.
      const textPreview = toPreviewPercent({ x: layer.x, y: layer.y });
      return {
        position: "absolute",
        left: `${textPreview.xPercent}%`,
        top: `${textPreview.yPercent}%`,
        transform:
          layer.rotation !== 0
            ? `translate(-50%, -50%) rotate(${layer.rotation}deg)`
            : "translate(-50%, -50%)",
        transformOrigin: "center center",
        zIndex: 15,
        cursor: "grab",
        userSelect: "none",
        touchAction: "none",
      };
    },
    [],
  );

  const getTextLayerStyle = useCallback((layer: ResolvedTextLayerSettings): React.CSSProperties => {
    const isEditing = editingLayerId === layer.id;
    // Phase 5: pure mathematical scale of the canonical video-space font
    // size. No minimum floor: tiny canonical sizes stay tiny in preview.
    const fontSize = toPreviewFontSize(layer.fontSize, targetScale);
    const outlineWidth = layer.outlineEnabled
      ? Math.max(0, layer.outlineWidth * targetScale)
      : 0;
    // Phase 3: no viewport-imposed text box. The element sizes to its
    // natural content width and the canvas frame clips it; explicit
    // newlines are preserved via `pre` without automatic wrapping.
    // Positioning/rotation live on the wrapper; remaining typography (line
    // height, letter spacing, outline rendering) untouched — Phase 6 scope.
    return {
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
      whiteSpace: "pre",
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
      // Unbounded overlay geometry: manual subtitle position follows the
      // same free-positioning rules as text/image overlays. No min/max drag bounds;
      // the frame only clips visibility. Auto-subtitle margin layout is
      // untouched (separate product concept, used only when manualPosition
      // is false).
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
        // Unbounded drag: screen-px delta → canonical delta with no frame
        // clamping. Manual subtitles share the free-positioning model;
        // auto-subtitle margin layout is not applied here.
        x:
          drag.startX + previewDeltaToCanonical(deltaX, drag.frameWidth),
        y:
          drag.startY + previewDeltaToCanonical(deltaY, drag.frameHeight),
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
    // Phase 5: pure mathematical scale of the resolved video-space font
    // size (explicit override or backend layout metric). No minimum floor.
    // Subtitle layout (margins, auto positioning) untouched.
    const fontSize = toPreviewFontSize(
      subtitleOverlay.fontSize ?? previewLayout.subtitle.fontSize,
      subtitleScale,
    );
    const marginV = previewLayout.subtitle.marginV * subtitleScale;
    const marginH = previewLayout.subtitle.marginH * subtitleScale;
    const outlineWidth =
      (subtitleOverlay.outlineEnabled
        ? (subtitleOverlay.outlineWidth ?? previewLayout.subtitle.outline)
        : 0) * subtitleScale;

    const style: React.CSSProperties = {
      position: "absolute",
      ...(subtitleOverlay.manualPosition
        ? (() => {
            // Phase 1: canonical video-space position → preview percent.
            // Center anchor preserved; auto-subtitle margin layout untouched.
            const manualPreview = toPreviewPercent({
              x: subtitleOverlay.x,
              y: subtitleOverlay.y,
            });
            return {
              left: `${manualPreview.xPercent}%`,
              top: `${manualPreview.yPercent}%`,
              transform: "translate(-50%, -50%)",
              maxWidth: `calc(100% - ${marginH * 2}px)`,
            };
          })()
        : {
            bottom: marginV,
            left: marginH,
            right: marginH,
          }),
      textAlign: "center",
      color: subtitleOverlay.color,
      opacity: subtitleOverlay.opacity,
      fontSize,
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
            onPointerDown={handleCanvasBoxPointerDown}
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
                    autoPlay={playing}
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
                  autoPlay={playing}
                  loop
                  playsInline
                  onPlay={syncBlurBackgroundToForeground}
                  onPause={syncBlurBackgroundToForeground}
                  onSeeking={syncBlurBackgroundToForeground}
                  onSeeked={syncBlurBackgroundToForeground}
                  onRateChange={syncBlurBackgroundToForeground}
                  onTimeUpdate={(e) => {
                    syncBlurBackgroundToForeground();
                    reportTime(e.currentTarget.currentTime);
                  }}
                  onCanPlay={handlePlaybackCanPlay}
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
                autoPlay={playing}
                loop
                playsInline
                onTimeUpdate={(e) =>
                  reportTime(e.currentTarget.currentTime)
                }
                onCanPlay={handlePlaybackCanPlay}
              />
            )}

            {/* Image Overlays: independent objects directly on the canvas.
                Single selection via selectedOverlayId; the panel edits the
                selected image. No image list in the panel. */}
            {imageOverlay.overlays
              .filter((overlay) => overlay.path.trim())
              .map((overlay) => {
                const isSelected =
                  imageOverlay.selectedOverlayId === overlay.id;
                const handles: ImageResizeHandle[] = [
                  "nw",
                  "n",
                  "ne",
                  "w",
                  "e",
                  "sw",
                  "s",
                  "se",
                ];
                return (
                  <div
                    key={overlay.id}
                    className={`canvas-image-overlay${isSelected ? " is-selected" : ""}`}
                    style={getImageWrapperStyle(overlay)}
                    onPointerDown={(event) =>
                      handleImagePointerDown(event, overlay)
                    }
                    onPointerMove={handleImagePointerMove}
                    onPointerUp={handleImagePointerEnd}
                    onPointerCancel={handleImagePointerEnd}
                    aria-label="Video image overlay"
                    aria-selected={isSelected}
                  >
                    <img
                      src={convertFileSrc(overlay.path)}
                      style={getImageElementStyle(overlay)}
                      alt=""
                      draggable={false}
                    />
                    {isSelected && (
                      <>
                        <div className="canvas-image-selection-outline" />
                        {handles.map((handle) => (
                          <div
                            key={handle}
                            className={`canvas-image-handle canvas-image-handle-${handle}`}
                            data-handle={handle}
                            onPointerDown={(event) =>
                              handleImageResizePointerDown(
                                event,
                                overlay,
                                handle,
                              )
                            }
                            onPointerMove={handleImageResizePointerMove}
                            onPointerUp={handleImageResizePointerEnd}
                            onPointerCancel={handleImageResizePointerEnd}
                          />
                        ))}
                        <div
                          className="canvas-image-rotate-handle"
                          aria-label="Rotate image"
                          onPointerDown={(event) =>
                            handleImageRotatePointerDown(event, overlay)
                          }
                          onPointerMove={handleImageRotatePointerMove}
                          onPointerUp={handleImageRotatePointerEnd}
                          onPointerCancel={handleImageRotatePointerEnd}
                        >
                          ↻
                        </div>
                      </>
                    )}
                  </div>
                );
              })}

            {/* Editable Text Layers.
                Each layer is a positioned wrapper (shared center-anchor
                geometry with image overlays) containing the typographic
                content element plus, when actively selected and not editing,
                the bounding-box resize/rotation handles. The content element
                keeps every existing behavior: editing lifecycle, text sync,
                typography, and selection styling. */}
            {textOverlay.layers
              .filter((layer) => layer.enabled)
              .map((layer) => {
                const isEditing = editingLayerId === layer.id;
                const isSelected = textOverlay.selectedLayerIds.includes(
                  layer.id,
                );
                const isEmpty = !layer.text.trim();
                // Bounding box persists while editing the selected layer:
                // selection (not editing state) controls visibility, so
                // clicking/typing in the already-selected text keeps its
                // handles. Drag is separately disabled while any layer is
                // editing (see handleTextPointerDown), so typing can't move
                // the layer; resize/rotate handles stay interactive.
                const showTextHandles = isSelected;
                const textHandles: OverlayResizeHandle[] = [
                  "nw",
                  "n",
                  "ne",
                  "w",
                  "e",
                  "sw",
                  "s",
                  "se",
                ];
                return (
                  <div
                    key={layer.id}
                    className="canvas-text-overlay-wrap"
                    style={getTextWrapperStyle(layer)}
                    onPointerDown={(event) =>
                      handleTextPointerDown(event, layer)
                    }
                    onPointerMove={handleTextPointerMove}
                    onPointerUp={handleTextPointerEnd}
                    onPointerCancel={handleTextPointerEnd}
                  >
                    <div
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
                    {showTextHandles && (
                      <>
                        {textHandles.map((handle) => (
                          <div
                            key={handle}
                            className={`canvas-image-handle canvas-image-handle-${handle}`}
                            data-handle={handle}
                            onPointerDown={(event) =>
                              handleTextResizePointerDown(
                                event,
                                layer,
                                handle,
                              )
                            }
                            onPointerMove={handleTextResizePointerMove}
                            onPointerUp={handleTextResizePointerEnd}
                            onPointerCancel={handleTextResizePointerEnd}
                          />
                        ))}
                        <div
                          className="canvas-image-rotate-handle"
                          aria-label="Rotate text"
                          onPointerDown={(event) =>
                            handleTextRotatePointerDown(event, layer)
                          }
                          onPointerMove={handleTextRotatePointerMove}
                          onPointerUp={handleTextRotatePointerEnd}
                          onPointerCancel={handleTextRotatePointerEnd}
                        >
                          ↻
                        </div>
                      </>
                    )}
                  </div>
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
