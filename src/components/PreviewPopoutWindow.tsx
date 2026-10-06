import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { PreviewHost } from "./PreviewHost";
import {
  POPOUT_STATE_EVENT_KEY,
  PREVIEW_PLAYBACK_COMMAND_EVENT,
  PREVIEW_POPOUT_APPLY_EVENT,
  PREVIEW_POPOUT_CANCEL_EVENT,
  PREVIEW_POPOUT_UPDATE_EVENT,
  clearPopoutSession,
  closeCurrentWindow,
  emitPopoutEvent,
  isTauriRuntime,
  readPopoutSession,
  rememberPopoutBounds,
  setCurrentWindowFullscreen,
  writePlaybackCommandEvent,
  writePopoutUpdateEvent,
} from "../services/previewController";
import {
  isEditableShortcutTarget,
  isExitPreviewFullscreenShortcut,
  isPreviewPlayPauseShortcut,
  normalizeShortcutKey,
} from "../utils/appShortcuts";
import type {
  ImageOverlaySettings,
  OrientationInfo,
  PreviewRenderLayout,
  SubtitleOverlaySettings,
  TextOverlaySettings,
  VideoEffectsSettings,
} from "../types/backend";
import {
  DEFAULT_TEXT_LAYER,
  normalizeTextOverlay,
  resolveTextOverlay,
} from "../utils/textOverlay";
import { normalizeSubtitleOverlay } from "../utils/subtitleOverlay";
import {
  normalizeImageOverlaySettings,
  resolveImageOverlaySettings,
} from "../utils/imageOverlay";

// Parity with the main window (App.tsx): arrow-key fine-positioning increment
// in canonical overlay coordinates (fraction of the video frame per axis).
const IMAGE_ARROW_NUDGE = 0.005;
const DEFAULT_POPOUT_VOLUME = 20;

function generateId(): string {
  return crypto.randomUUID();
}

/**
 * PreviewPopoutWindow — the pop-out native-window host root.
 *
 * This is ONLY a host: it reuses the same `VideoCanvas` implementation
 * through `PreviewHost` (no second renderer, no second overlay system).
 * There is no second application shell here — only the preview surface plus
 * its Apply/Cancel transaction footer.
 *
 * PLAYBACK OWNERSHIP: this host never owns the media clock. The main window
 * owns authoritative `playing` state; this host renders it (via the `playing`
 * prop) and routes user playback actions back as commands on
 * `PREVIEW_PLAYBACK_COMMAND_EVENT`. It never calls `video.play()` /
 * `video.pause()` / `video.currentTime =` / `video.playbackRate =` as part
 * of its own loop — `VideoCanvas` only follows the authoritative props.
 * `currentTime` is tracked locally solely for the one-time handoff snapshot
 * on Apply/Cancel/close, never chased continuously. Editing/state sync
 * (`effects`, `previewVolume`, `previewLayout`, draft Apply/Cancel) is
 * separate and preserved untouched.
 *
 * Editing model: the main window snapshots committed effects/volume at
 * Pop Out time. All edits here apply to that draft. Apply commits (main
 * keeps the draft); Cancel / native X discards (main restores the snapshot).
 * `Apply is the only explicit commit action.`
 */

interface PopoutStateEvent {
  kind: "update" | "apply" | "cancel" | "close";
  source: "main" | "popout";
  effects?: VideoEffectsSettings;
  previewVolume?: number;
  playing?: boolean;
  currentTime?: number;
  previewLayout?: PreviewRenderLayout | null;
  showGuides?: boolean;
  showSafeFrames?: boolean;
  at: number;
}

function readStateEvent(): PopoutStateEvent | null {
  try {
    const raw = window.localStorage.getItem(POPOUT_STATE_EVENT_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as PopoutStateEvent;
    if (!parsed || (parsed.kind !== "update" && parsed.kind !== "apply" && parsed.kind !== "cancel" && parsed.kind !== "close")) {
      return null;
    }
    return parsed;
  } catch {
    return null;
  }
}

function writeStateEvent(event: Omit<PopoutStateEvent, "at">): void {
  try {
    window.localStorage.setItem(
      POPOUT_STATE_EVENT_KEY,
      JSON.stringify({ ...event, at: Date.now() }),
    );
  } catch {
    // Best-effort only.
  }
}

export const PreviewPopoutWindow: React.FC = () => {
  const [session] = useState(readPopoutSession);
  const [effects, setEffects] = useState<VideoEffectsSettings | null>(() => {
    const raw = session?.effects;
    if (!raw || typeof raw !== "object") return null;
    return raw as VideoEffectsSettings;
  });
  const [previewLayout, setPreviewLayout] =
    useState<PreviewRenderLayout | null>(() => {
      const raw = session?.previewLayout;
      if (!raw || typeof raw !== "object") return null;
      return raw as PreviewRenderLayout;
    });
  const [previewVolume, setPreviewVolume] = useState<number>(
    () => session?.previewVolume ?? 20,
  );
  const [playing, setPlaying] = useState<boolean>(
    () => session?.playing ?? true,
  );
  const [isFullscreen, setIsFullscreen] = useState(false);
  const prevMaximizedRef = useRef(false);
  const currentTimeRef = useRef<number>(session?.currentTime ?? 0);
  const settledRef = useRef<"open" | "applied" | "cancelled">("open");
  const effectsRef = useRef(effects);
  const volumeRef = useRef(previewVolume);

  useEffect(() => {
    effectsRef.current = effects;
  }, [effects]);
  useEffect(() => {
    volumeRef.current = previewVolume;
  }, [previewVolume]);

  const orientation: OrientationInfo | null =
    session?.orientation && typeof session.orientation === "object"
      ? (session.orientation as OrientationInfo)
      : null;
  const videoSrc = typeof session?.videoSrc === "string" ? session.videoSrc : "";
  const [showGuides, setShowGuides] = useState<boolean>(
    () => session?.showGuides ?? true,
  );
  const [showSafeFrames, setShowSafeFrames] = useState<boolean>(
    () => session?.showSafeFrames ?? false,
  );
  const playbackRate =
    typeof session?.playbackRate === "number" ? session.playbackRate : 1;

  // Resolved overlay selection (mirrors the main window's derived selection).
  // Used only for keyboard parity (Delete / Ctrl+D / arrows) operating on the
  // same draft `effects` through `applyDraft` — no second overlay system.
  const resolvedImageOverlay = useMemo(
    () => resolveImageOverlaySettings(effects?.imageOverlay ?? null),
    [effects],
  );
  const resolvedTextOverlay = useMemo(
    () => resolveTextOverlay(effects?.textOverlay ?? null),
    [effects],
  );
  const hasSelectedImage = resolvedImageOverlay.selectedOverlayId !== null;
  const hasSelectedTextLayer =
    resolvedTextOverlay.selectedLayerIds.length > 0;

  // Register asset-protocol scope inside this window so the same file paths
  // resolve here exactly as they do in the main window.
  useEffect(() => {
    if (!isTauriRuntime()) return;
    const paths = new Set<string>();
    if (videoSrc) paths.add(videoSrc);
    try {
      const overlays = resolveImageOverlaySettings(
        effects?.imageOverlay ?? null,
      ).overlays;
      for (const overlay of overlays) {
        if (overlay.path.trim()) paths.add(overlay.path);
      }
    } catch {
      // Normalization is best-effort here.
    }
    for (const path of paths) {
      invoke("allow_path_scope", { path }).catch(() => {});
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const applyDraft = useCallback(
    async (next: VideoEffectsSettings) => {
      setEffects(next);
      // Editing sync only: mirror the draft overlays. Playback fields are
      // intentionally omitted here (no competing clock, no currentTime
      // chasing); position travels only as a handoff snapshot on
      // Apply/Cancel, and playing is owned by the main window.
      writePopoutUpdateEvent("popout", { effects: next });
      await emitPopoutEvent(PREVIEW_POPOUT_UPDATE_EVENT, {
        source: "popout",
        effects: next,
      });
    },
    [],
  );

  const handleTextOverlayChange = useCallback(
    (next: TextOverlaySettings) => {
      const current = effectsRef.current;
      if (!current) return;
      void applyDraft({
        ...current,
        textOverlay: resolveTextOverlay(next),
      });
    },
    [applyDraft],
  );

  const handleSubtitleOverlayChange = useCallback(
    (next: SubtitleOverlaySettings) => {
      const current = effectsRef.current;
      if (!current) return;
      void applyDraft({
        ...current,
        subtitleOverlay: normalizeSubtitleOverlay(next),
      });
    },
    [applyDraft],
  );

  const handleImageOverlayChange = useCallback(
    (next: ImageOverlaySettings) => {
      const current = effectsRef.current;
      if (!current) return;
      void applyDraft({
        ...current,
        imageOverlay: normalizeImageOverlaySettings(next),
      });
    },
    [applyDraft],
  );

  const handleVolumeChange = useCallback((value: number) => {
    const clamped = Math.max(0, Math.min(100, Math.round(value)));
    setPreviewVolume(clamped);
    // Editing/state sync (volume is preserved per scope). No playback
    // fields: the pop-out never mirrors playing/currentTime continuously.
    writePopoutUpdateEvent("popout", { previewVolume: clamped });
    void emitPopoutEvent(PREVIEW_POPOUT_UPDATE_EVENT, {
      source: "popout",
      previewVolume: clamped,
    });
  }, []);

  const handleMuteToggle = useCallback(() => {
    // Parity with the primary preview mute button: toggle 0 <-> default.
    // Routes through the same volume sync so the owner converges.
    handleVolumeChange(previewVolume > 0 ? 0 : DEFAULT_POPOUT_VOLUME);
  }, [handleVolumeChange, previewVolume]);

  const handleGuidesToggle = useCallback(() => {
    const next = !showGuides;
    setShowGuides(next);
    writePopoutUpdateEvent("popout", { showGuides: next });
    void emitPopoutEvent(PREVIEW_POPOUT_UPDATE_EVENT, {
      source: "popout",
      showGuides: next,
    });
  }, [showGuides]);

  const handleSafeFramesToggle = useCallback(() => {
    const next = !showSafeFrames;
    setShowSafeFrames(next);
    writePopoutUpdateEvent("popout", { showSafeFrames: next });
    void emitPopoutEvent(PREVIEW_POPOUT_UPDATE_EVENT, {
      source: "popout",
      showSafeFrames: next,
    });
  }, [showSafeFrames]);

  // ── Overlay keyboard parity (mirrors App.tsx main-window handlers) ──
  // All operations route through the same draft `effects` via `applyDraft` —
  // no second overlay system, no geometry changes. Pointer/drag/resize/rotate
  // already share `VideoCanvas`; these handlers cover the keyboard-initiated
  // operations that previously existed only in the main window.
  const handleRemoveSelectedOverlays = useCallback(() => {
    const current = effectsRef.current;
    if (!current) return;
    const currentImages = resolveImageOverlaySettings(current.imageOverlay);
    const currentText = resolveTextOverlay(current.textOverlay);
    const hasImage = currentImages.selectedOverlayId !== null;
    const hasText = currentText.selectedLayerIds.length > 0;
    if (!hasImage && !hasText) return;
    let next: VideoEffectsSettings = current;
    if (hasImage) {
      const selectedId = currentImages.selectedOverlayId;
      next = {
        ...next,
        imageOverlay: normalizeImageOverlaySettings({
          ...currentImages,
          overlays: currentImages.overlays.filter(
            (overlay) => overlay.id !== selectedId,
          ),
          selectedOverlayId: null,
        }),
      };
    }
    if (hasText) {
      const selectedIds = new Set(
        resolveTextOverlay(next.textOverlay).selectedLayerIds,
      );
      const resolved = resolveTextOverlay(next.textOverlay);
      next = {
        ...next,
        textOverlay: normalizeTextOverlay({
          ...resolved,
          layers: resolved.layers.filter((layer) => !selectedIds.has(layer.id)),
          selectedLayerIds: [],
        }),
      };
    }
    void applyDraft(next);
  }, [applyDraft]);

  const handleDuplicateSelectedOverlays = useCallback(() => {
    const current = effectsRef.current;
    if (!current) return;
    const currentImages = resolveImageOverlaySettings(current.imageOverlay);
    const currentText = resolveTextOverlay(current.textOverlay);
    const hasImage = currentImages.selectedOverlayId !== null;
    const hasText = currentText.selectedLayerIds.length > 0;
    if (!hasImage && !hasText) return;
    let next: VideoEffectsSettings = current;
    if (hasImage) {
      const source = currentImages.overlays.find(
        (overlay) => overlay.id === currentImages.selectedOverlayId,
      );
      if (source) {
        const id = generateId();
        next = {
          ...next,
          imageOverlay: normalizeImageOverlaySettings({
            ...currentImages,
            panelOpen: true,
            overlays: [
              ...currentImages.overlays,
              {
                ...source,
                id,
                x: source.x + 0.05,
                y: source.y + 0.05,
                crop: { ...source.crop },
              },
            ],
            selectedOverlayId: id,
          }),
          textOverlay: normalizeTextOverlay({
            ...resolveTextOverlay(next.textOverlay),
            selectedLayerIds: [],
          }),
        };
      }
    }
    if (hasText) {
      const resolved = resolveTextOverlay(next.textOverlay);
      const offset = Math.min(0.2, resolved.layers.length * 0.035);
      const id = generateId();
      const sourceLayer =
        resolved.selectedLayerIds
          .map((selectedId) =>
            resolved.layers.find((layer) => layer.id === selectedId),
          )
          .find((layer) => !!layer) ?? DEFAULT_TEXT_LAYER;
      const imagesAfter = resolveImageOverlaySettings(next.imageOverlay);
      next = {
        ...next,
        textOverlay: normalizeTextOverlay({
          ...resolved,
          panelOpen: true,
          layers: [
            ...resolved.layers,
            {
              ...sourceLayer,
              id,
              x: sourceLayer.x + offset,
              y: sourceLayer.y + offset,
            },
          ],
          selectedLayerIds: [id],
        }),
        imageOverlay: normalizeImageOverlaySettings({
          ...imagesAfter,
          selectedOverlayId: null,
        }),
      };
    }
    void applyDraft(next);
    requestAnimationFrame(() => {
      if (document.activeElement instanceof HTMLElement) {
        document.activeElement.blur();
      }
    });
  }, [applyDraft]);

  const handleNudgeSelectedOverlays = useCallback(
    (dx: number, dy: number) => {
      const current = effectsRef.current;
      if (!current) return;
      const currentImages = resolveImageOverlaySettings(current.imageOverlay);
      const currentText = resolveTextOverlay(current.textOverlay);
      if (
        currentImages.selectedOverlayId === null &&
        currentText.selectedLayerIds.length === 0
      ) {
        return;
      }
      let next: VideoEffectsSettings = current;
      if (currentImages.selectedOverlayId !== null) {
        const selectedId = currentImages.selectedOverlayId;
        next = {
          ...next,
          imageOverlay: normalizeImageOverlaySettings({
            ...currentImages,
            overlays: currentImages.overlays.map((overlay) =>
              overlay.id === selectedId
                ? { ...overlay, x: overlay.x + dx, y: overlay.y + dy }
                : overlay,
            ),
          }),
        };
      }
      if (currentText.selectedLayerIds.length > 0) {
        const resolved = resolveTextOverlay(next.textOverlay);
        const selectedIds = new Set(resolved.selectedLayerIds);
        next = {
          ...next,
          textOverlay: normalizeTextOverlay({
            ...resolved,
            layers: resolved.layers.map((layer) =>
              selectedIds.has(layer.id)
                ? { ...layer, x: layer.x + dx, y: layer.y + dy }
                : layer,
            ),
          }),
        };
      }
      void applyDraft(next);
    },
    [applyDraft],
  );

  const handleAddTextLayer = useCallback(() => {
    const current = effectsRef.current;
    if (!current) return;
    const currentOverlay = resolveTextOverlay(current.textOverlay);
    const offset = Math.min(0.2, currentOverlay.layers.length * 0.035);
    const id = generateId();
    const sourceLayer =
      currentOverlay.selectedLayerIds
        .map((selectedId) =>
          currentOverlay.layers.find((layer) => layer.id === selectedId),
        )
        .find((layer) => !!layer) ?? DEFAULT_TEXT_LAYER;
    const currentImages = resolveImageOverlaySettings(current.imageOverlay);
    void applyDraft({
      ...current,
      textOverlay: normalizeTextOverlay({
        ...currentOverlay,
        panelOpen: true,
        layers: [
          ...currentOverlay.layers,
          {
            ...sourceLayer,
            id,
            x: sourceLayer.x + offset,
            y: sourceLayer.y + offset,
          },
        ],
        selectedLayerIds: [id],
      }),
      imageOverlay: normalizeImageOverlaySettings({
        ...currentImages,
        selectedOverlayId: null,
      }),
    });
    requestAnimationFrame(() => {
      if (document.activeElement instanceof HTMLElement) {
        document.activeElement.blur();
      }
    });
  }, [applyDraft]);

  const handlePickImage = useCallback(async () => {
    const current = effectsRef.current;
    if (!current) return;
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const sel = await open({
        multiple: false,
        filters: [
          {
            name: "Image",
            extensions: ["png", "jpg", "jpeg", "svg", "webp", "gif"],
          },
        ],
      });
      if (!sel || typeof sel !== "string") return;
      const live = effectsRef.current;
      if (!live) return;
      const id = generateId();
      const currentOverlay = resolveImageOverlaySettings(live.imageOverlay);
      const currentText = resolveTextOverlay(live.textOverlay);
      await invoke("allow_path_scope", { path: sel }).catch(() => {});
      void applyDraft({
        ...live,
        imageOverlay: normalizeImageOverlaySettings({
          ...currentOverlay,
          panelOpen: true,
          overlays: [
            ...currentOverlay.overlays,
            {
              id,
              path: sel,
              x: 0.5,
              y: 0.5,
              scale: 0.25,
              rotation: 0,
              opacity: 1,
              flipHorizontal: false,
              flipVertical: false,
              crop: { x: 0, y: 0, width: 1, height: 1 },
            },
          ],
          selectedOverlayId: id,
        }),
        textOverlay: normalizeTextOverlay({
          ...currentText,
          selectedLayerIds: [],
        }),
      });
      requestAnimationFrame(() => {
        if (document.activeElement instanceof HTMLElement) {
          document.activeElement.blur();
        }
      });
    } catch {
      // File picker is best-effort; dismissal is not an error.
    }
  }, [applyDraft]);

  const togglePlayback = useCallback(() => {
    // Command routing only: never toggle a local clock. The main window is
    // the single owner; it applies this intent to `previewPlaying` and
    // pushes the authoritative state back, which this host renders. The
    // local `playing` state below updates only from those authoritative
    // pushes (plus the opening session snapshot), so both windows represent
    // the same timeline with no drift-correction loop.
    writePlaybackCommandEvent("toggle");
    void emitPopoutEvent(PREVIEW_PLAYBACK_COMMAND_EVENT, {
      source: "popout",
      command: "toggle",
      at: Date.now(),
    });
  }, []);

  const handleApply = useCallback(async () => {
    if (settledRef.current !== "open") return;
    settledRef.current = "applied";
    const draft = effectsRef.current;
    // Apply commits the draft; the one-time `currentTime` handoff lets the
    // authoritative owner resume exactly. No `playing` here: the owner keeps
    // its authoritative playing state.
    writeStateEvent({
      kind: "apply",
      source: "popout",
      effects: draft ?? undefined,
      previewVolume: volumeRef.current,
      currentTime: currentTimeRef.current,
    });
    await emitPopoutEvent(PREVIEW_POPOUT_APPLY_EVENT, {
      source: "popout",
      effects: draft,
      previewVolume: volumeRef.current,
      currentTime: currentTimeRef.current,
    });
    clearPopoutSession();
    await closeCurrentWindow();
  }, []);

  const handleCancel = useCallback(async () => {
    if (settledRef.current !== "open") return;
    settledRef.current = "cancelled";
    // X = leave pop-out without committing: discard the draft. The main
    // window restores its pre-pop-out snapshot; Apply is the only commit.
    // The handoff position is still reported so the owner resumes exactly.
    writeStateEvent({
      kind: "cancel",
      source: "popout",
      currentTime: currentTimeRef.current,
    });
    await emitPopoutEvent(PREVIEW_POPOUT_CANCEL_EVENT, {
      source: "popout",
      currentTime: currentTimeRef.current,
    });
    clearPopoutSession();
    await closeCurrentWindow();
  }, []);

  // Native X must exit pop-out without committing. `beforeunload` is the
  // reliable cross-runtime hook: if the session was not explicitly applied,
  // synchronously record a discard so the main window restores its snapshot.
  // The final position is included synchronously so a main-initiated close
  // during playback still resumes exactly (one-time handoff, no loop).
  useEffect(() => {
    const onBeforeUnload = () => {
      if (settledRef.current !== "open") return;
      settledRef.current = "cancelled";
      writeStateEvent({
        kind: "cancel",
        source: "popout",
        currentTime: currentTimeRef.current,
      });
    };
    window.addEventListener("beforeunload", onBeforeUnload);
    return () => window.removeEventListener("beforeunload", onBeforeUnload);
  }, []);

  // Updates pushed by the main window (e.g. recomputed preview layout while
  // the draft is mirrored there, or Apply/Cancel initiated from the main
  // placeholder). Self-originated events are ignored.
  useEffect(() => {
    let disposed = false;
    const seenAtRef = { current: 0 };

    const applyRemote = (event: PopoutStateEvent | null) => {
      if (!event || disposed) return;
      if (event.source !== "main") return;
      if (event.at <= seenAtRef.current) return;
      seenAtRef.current = event.at;
      if (event.kind === "update") {
        if (event.previewLayout !== undefined) {
          setPreviewLayout(event.previewLayout);
        }
        if (typeof event.previewVolume === "number") {
          setPreviewVolume(
            Math.max(0, Math.min(100, Math.round(event.previewVolume))),
          );
        }
        if (typeof event.playing === "boolean") {
          setPlaying(event.playing);
        }
        if (typeof event.showGuides === "boolean") {
          setShowGuides(event.showGuides);
        }
        if (typeof event.showSafeFrames === "boolean") {
          setShowSafeFrames(event.showSafeFrames);
        }
      } else if (event.kind === "apply" || event.kind === "cancel" || event.kind === "close") {
        if (settledRef.current === "open") {
          settledRef.current =
            event.kind === "apply" ? "applied" : "cancelled";
        }
        clearPopoutSession();
        void closeCurrentWindow();
      }
    };

    const onStorage = (e: StorageEvent) => {
      if (e.key !== POPOUT_STATE_EVENT_KEY) return;
      applyRemote(readStateEvent());
    };
    window.addEventListener("storage", onStorage);

    let unsubs: Array<() => void> = [];
    if (isTauriRuntime()) {
      (async () => {
        try {
          const { listen } = await import("@tauri-apps/api/event");
          const events = [
            PREVIEW_POPOUT_UPDATE_EVENT,
            PREVIEW_POPOUT_APPLY_EVENT,
            PREVIEW_POPOUT_CANCEL_EVENT,
          ];
          for (const name of events) {
            const unlisten = await listen(name, (tauriEvent) => {
              const payload = tauriEvent.payload as Partial<PopoutStateEvent> | null;
              if (!payload || payload.source !== "main") return;
              if (name === PREVIEW_POPOUT_UPDATE_EVENT) {
                applyRemote({ ...(payload as PopoutStateEvent), kind: "update", at: Date.now() });
              } else if (name === PREVIEW_POPOUT_APPLY_EVENT) {
                applyRemote({ ...(payload as PopoutStateEvent), kind: "apply", at: Date.now() });
              } else {
                applyRemote({ ...(payload as PopoutStateEvent), kind: "cancel", at: Date.now() });
              }
            });
            if (disposed) unlisten();
            else unsubs.push(unlisten);
          }
        } catch {
          // Storage-event fallback covers browser use.
        }
      })();
    }
    return () => {
      disposed = true;
      window.removeEventListener("storage", onStorage);
      unsubs.forEach((unsub) => unsub());
    };
  }, []);

  const enterFullscreen = useCallback(async () => {
    if (isFullscreen) return;
    const bounds = await rememberPopoutBounds();
    prevMaximizedRef.current = bounds.maximized;
    setIsFullscreen(true);
    await setCurrentWindowFullscreen(true);
  }, [isFullscreen]);

  const exitFullscreenRestorePopout = useCallback(async () => {
    if (!isFullscreen) return;
    setIsFullscreen(false);
    await setCurrentWindowFullscreen(false);
    // Restore the exact pop-out window state from before fullscreen and
    // remain in pop-out mode (never drop to embedded on Esc).
    if (isTauriRuntime() && prevMaximizedRef.current === false) {
      try {
        const { getCurrentWindow } = await import("@tauri-apps/api/window");
        const win = getCurrentWindow();
        if (await win.isMaximized().catch(() => false)) {
          await win.unmaximize().catch(() => {});
        }
      } catch {
        // Best-effort restore only.
      }
    }
  }, [isFullscreen]);

  // Preview keyboard parity for this host: Space (play/pause), Esc (fullscreen
  // exit), Ctrl/Cmd+D (duplicate), Delete (delete), arrows (nudge). Mirrors
  // the main-window preview handler in App.tsx through the same draft
  // architecture. Editable targets (inputs, buttons, sliders, text editing)
  // are never hijacked.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (isPreviewPlayPauseShortcut(event)) {
        event.preventDefault();
        event.stopPropagation();
        togglePlayback();
        return;
      }
      if (isExitPreviewFullscreenShortcut(event) && isFullscreen) {
        // Esc exits custom fullscreen and restores the previous pop-out
        // state; it never closes the pop-out or cancels the session.
        if (!isEditableShortcutTarget(event.target)) {
          event.preventDefault();
          event.stopPropagation();
          void exitFullscreenRestorePopout();
        }
        return;
      }
      if (event.defaultPrevented) {
        return;
      }

      // Ctrl/Cmd+D duplicates the currently selected overlay. Mirrors App.tsx:
      // images duplicate via the image path; text layers duplicate via the Add
      // Text path. No new shortcuts are introduced.
      if (
        (event.ctrlKey || event.metaKey) &&
        !event.altKey &&
        !event.shiftKey &&
        normalizeShortcutKey(event) === "d" &&
        !isEditableShortcutTarget(event.target)
      ) {
        if (hasSelectedImage || hasSelectedTextLayer) {
          event.preventDefault();
          event.stopPropagation();
          handleDuplicateSelectedOverlays();
          return;
        }
      }

      if (
        event.key === "Delete" &&
        (hasSelectedTextLayer || hasSelectedImage) &&
        !isEditableShortcutTarget(event.target)
      ) {
        event.preventDefault();
        event.stopPropagation();
        handleRemoveSelectedOverlays();
        return;
      }

      // Arrow keys fine-position the selected overlays by one deterministic
      // canonical increment per press. Plain arrows only (no modifiers), and
      // never while an editable control has focus — same guard as App.tsx.
      if (
        !event.ctrlKey &&
        !event.metaKey &&
        !event.altKey &&
        !event.shiftKey &&
        (event.key === "ArrowUp" ||
          event.key === "ArrowDown" ||
          event.key === "ArrowLeft" ||
          event.key === "ArrowRight") &&
        (hasSelectedImage || hasSelectedTextLayer) &&
        !isEditableShortcutTarget(event.target)
      ) {
        event.preventDefault();
        event.stopPropagation();
        let dx = 0;
        let dy = 0;
        switch (event.key) {
          case "ArrowLeft":
            dx = -IMAGE_ARROW_NUDGE;
            break;
          case "ArrowRight":
            dx = IMAGE_ARROW_NUDGE;
            break;
          case "ArrowUp":
            dy = -IMAGE_ARROW_NUDGE;
            break;
          case "ArrowDown":
            dy = IMAGE_ARROW_NUDGE;
            break;
        }
        handleNudgeSelectedOverlays(dx, dy);
        return;
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [togglePlayback, isFullscreen, exitFullscreenRestorePopout, hasSelectedImage, hasSelectedTextLayer, handleDuplicateSelectedOverlays, handleRemoveSelectedOverlays, handleNudgeSelectedOverlays]);

  const handleTimeUpdate = useCallback((currentTime: number) => {
    // Local handoff bookkeeping only: records the renderer's position for
    // the one-time Apply/Cancel/close snapshot. Never broadcast continuously
    // and never used to chase another clock.
    currentTimeRef.current = currentTime;
  }, []);

  if (!session || !effects) {
    return (
      <div className="preview-popout-root preview-popout-empty">
        <div className="preview-popout-empty-card">
          <div className="preview-popout-empty-title">No preview session</div>
          <p className="preview-popout-empty-text">
            Open a video in the main window and choose Pop Out to edit it here.
          </p>
          <button
            type="button"
            className="btn btn-ghost"
            onClick={() => void handleCancel()}
          >
            Close
          </button>
        </div>
      </div>
    );
  }

  return (
    <div
      className={`preview-popout-root${isFullscreen ? " is-fullscreen" : ""}`}
      data-preview-mode={isFullscreen ? "fullscreen" : "popout"}
    >
      <div className="preview-popout-stage">
        <PreviewHost
          mode={isFullscreen ? "fullscreen" : "popout"}
          videoSrc={videoSrc}
          previewLayout={previewLayout}
          effects={effects}
          onTextOverlayChange={handleTextOverlayChange}
          onSubtitleOverlayChange={handleSubtitleOverlayChange}
          onImageOverlayChange={handleImageOverlayChange}
          orientation={orientation}
          previewVolume={previewVolume}
          showGuides={showGuides}
          showSafeFrames={showSafeFrames}
          playing={playing}
          playbackRate={playbackRate}
          initialTime={session.currentTime}
          onTimeUpdate={handleTimeUpdate}
        />
      </div>
      <div className="preview-popout-footer">
        <div className="preview-popout-playback">
          <button
            type="button"
            className="btn btn-xs"
            onClick={togglePlayback}
            aria-label={playing ? "Pause preview" : "Play preview"}
            title="Play/Pause (Space)"
          >
            {playing ? "❚❚ Pause" : "▶ Play"}
          </button>
          <label className="preview-popout-volume" aria-label="Preview volume">
            <span className="preview-popout-volume-label">Volume</span>
            <input
              type="range"
              min={0}
              max={100}
              step={1}
              value={previewVolume}
              onChange={(e) => handleVolumeChange(Number(e.target.value))}
              aria-label="Preview volume slider"
            />
          </label>
          <button
            type="button"
            className="btn btn-xs"
            onClick={handleMuteToggle}
            aria-label={previewVolume === 0 ? "Unmute preview" : "Mute preview"}
            title={`Preview volume: ${previewVolume}%`}
          >
            {previewVolume === 0 ? "Unmute" : "Mute"}
          </button>
          <button
            type="button"
            className={`btn btn-xs${showGuides ? " active" : ""}`}
            onClick={handleGuidesToggle}
            aria-pressed={showGuides}
            aria-label="Toggle guides"
            title="Toggle guides"
          >
            Guides
          </button>
          <button
            type="button"
            className={`btn btn-xs${showSafeFrames ? " active" : ""}`}
            onClick={handleSafeFramesToggle}
            aria-pressed={showSafeFrames}
            aria-label="Toggle safe areas"
            title="Toggle safe areas"
          >
            Safe Areas
          </button>
          <button
            type="button"
            className="btn btn-xs"
            onClick={handleAddTextLayer}
            aria-label="Add text overlay"
            title="Add text overlay"
          >
            + Text
          </button>
          <button
            type="button"
            className="btn btn-xs"
            onClick={() => void handlePickImage()}
            aria-label="Add image overlay"
            title="Add image overlay"
          >
            + Image
          </button>
          <button
            type="button"
            className="btn btn-xs"
            onClick={() =>
              void (isFullscreen
                ? exitFullscreenRestorePopout()
                : enterFullscreen())
            }
            aria-label={isFullscreen ? "Exit fullscreen" : "Enter fullscreen"}
            title={
              isFullscreen ? "Exit fullscreen (Esc)" : "Enter fullscreen"
            }
          >
            {isFullscreen ? "Exit Fullscreen" : "Fullscreen"}
          </button>
        </div>
        <div className="preview-popout-actions">
          <button
            type="button"
            className="btn btn-ghost"
            onClick={() => void handleCancel()}
          >
            Cancel
          </button>
          <button
            type="button"
            className="btn btn-primary"
            onClick={() => void handleApply()}
          >
            Apply
          </button>
        </div>
      </div>
    </div>
  );
};
