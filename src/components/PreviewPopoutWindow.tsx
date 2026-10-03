import React, { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { PreviewHost } from "./PreviewHost";
import {
  POPOUT_STATE_EVENT_KEY,
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
  writePopoutUpdateEvent,
} from "../services/previewController";
import {
  isEditableShortcutTarget,
  isExitPreviewFullscreenShortcut,
  isPreviewPlayPauseShortcut,
} from "../utils/appShortcuts";
import type {
  ImageOverlaySettings,
  OrientationInfo,
  PreviewRenderLayout,
  SubtitleOverlaySettings,
  TextOverlaySettings,
  VideoEffectsSettings,
} from "../types/backend";
import { resolveTextOverlay } from "../utils/textOverlay";
import { normalizeSubtitleOverlay } from "../utils/subtitleOverlay";
import {
  normalizeImageOverlaySettings,
  resolveImageOverlaySettings,
} from "../utils/imageOverlay";

/**
 * PreviewPopoutWindow — the pop-out native-window host root.
 *
 * This is ONLY a host: it reuses the same `VideoCanvas` implementation
 * through `PreviewHost` (no second renderer, no second overlay system).
 * There is no second application shell here — only the preview surface plus
 * its Apply/Cancel transaction footer.
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
  const playingRef = useRef(playing);

  useEffect(() => {
    effectsRef.current = effects;
  }, [effects]);
  useEffect(() => {
    volumeRef.current = previewVolume;
  }, [previewVolume]);
  useEffect(() => {
    playingRef.current = playing;
  }, [playing]);

  const orientation: OrientationInfo | null =
    session?.orientation && typeof session.orientation === "object"
      ? (session.orientation as OrientationInfo)
      : null;
  const videoSrc = typeof session?.videoSrc === "string" ? session.videoSrc : "";
  const showGuides = session?.showGuides ?? true;
  const showSafeFrames = session?.showSafeFrames ?? false;
  const playbackRate =
    typeof session?.playbackRate === "number" ? session.playbackRate : 1;

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
      writePopoutUpdateEvent("popout", { effects: next });
      await emitPopoutEvent(PREVIEW_POPOUT_UPDATE_EVENT, {
        source: "popout",
        effects: next,
        previewVolume: volumeRef.current,
        playing: playingRef.current,
        currentTime: currentTimeRef.current,
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
    writePopoutUpdateEvent("popout", { previewVolume: clamped });
    void emitPopoutEvent(PREVIEW_POPOUT_UPDATE_EVENT, {
      source: "popout",
      effects: effectsRef.current,
      previewVolume: clamped,
      playing: playingRef.current,
      currentTime: currentTimeRef.current,
    });
  }, []);

  const togglePlayback = useCallback(() => {
    setPlaying((was) => {
      const next = !was;
      writePopoutUpdateEvent("popout", { playing: next });
      void emitPopoutEvent(PREVIEW_POPOUT_UPDATE_EVENT, {
        source: "popout",
        effects: effectsRef.current,
        previewVolume: volumeRef.current,
        playing: next,
        currentTime: currentTimeRef.current,
      });
      return next;
    });
  }, []);

  const handleApply = useCallback(async () => {
    if (settledRef.current !== "open") return;
    settledRef.current = "applied";
    const draft = effectsRef.current;
    writeStateEvent({
      kind: "apply",
      source: "popout",
      effects: draft ?? undefined,
      previewVolume: volumeRef.current,
      playing: playingRef.current,
      currentTime: currentTimeRef.current,
    });
    await emitPopoutEvent(PREVIEW_POPOUT_APPLY_EVENT, {
      source: "popout",
      effects: draft,
      previewVolume: volumeRef.current,
      playing: playingRef.current,
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
    writeStateEvent({ kind: "cancel", source: "popout" });
    await emitPopoutEvent(PREVIEW_POPOUT_CANCEL_EVENT, { source: "popout" });
    clearPopoutSession();
    await closeCurrentWindow();
  }, []);

  // Native X must exit pop-out without committing. `beforeunload` is the
  // reliable cross-runtime hook: if the session was not explicitly applied,
  // synchronously record a discard so the main window restores its snapshot.
  useEffect(() => {
    const onBeforeUnload = () => {
      if (settledRef.current !== "open") return;
      settledRef.current = "cancelled";
      writeStateEvent({ kind: "cancel", source: "popout" });
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

  // Single global Space implementation for this host + Esc fullscreen exit.
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
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [togglePlayback, isFullscreen, exitFullscreenRestorePopout]);

  const handleTimeUpdate = useCallback((currentTime: number) => {
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
