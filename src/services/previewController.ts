/**
 * PreviewController — preview hosting / display-mode / window-lifecycle layer.
 *
 * HARD ARCHITECTURAL INVARIANT (must be preserved):
 *
 * > There must only ever be one active preview renderer for the current
 * > project. The pop-out window is a different host for the same preview
 * > system, not a second preview implementation.
 *
 * That means:
 *
 * - One preview state (video, playback, currentTime, overlays, selection,
 *   preview geometry) owned by the application.
 * - One renderer (`VideoCanvas`) and one overlay interaction system used in
 *   every display mode.
 * - Multiple display/window modes (`embedded` / `popout` / `fullscreen`) that
 *   only change *where* the same preview is hosted.
 * - Only one host is active at a time (embedded preview is hidden/inactive
 *   while the pop-out hosts the preview).
 *
 * A Tauri WebView cannot literally share one DOM node between two native OS
 * windows, so the pop-out uses a second WebView/window *container*. That
 * container must reuse the same `VideoCanvas` implementation (via
 * `PreviewHost`) and synchronize from the single preview state — it must
 * never become a forked `PopOutVideoCanvas` renderer or a second overlay
 * system.
 *
 * Responsibility boundaries (do not mix):
 *
 * - `VideoCanvas`: video/overlay rendering, interaction, geometry, resize,
 *   play/pause behavior.
 * - PreviewController (this module + hosting state in `App`): preview mode,
 *   embedded/pop-out switching, pop-out lifecycle, draft Apply/Cancel
 *   transaction, Close semantics, fullscreen state, keyboard ownership,
 *   preserving preview state.
 * - Tauri / native window layer: native creation, show/hide, resize,
 *   maximize/unmaximize, minimize/unminimize, close, focus, blur, lifecycle.
 *
 * Focus/visibility invariant (mandatory):
 *
 * - The pop-out window's visibility and minimized state are controlled by the
 *   native Windows window manager, not by application focus changes.
 * - There is intentionally NO blur-to-minimize / click-outside-to-hide logic
 *   anywhere in this feature. `blur` must have no minimize side effect.
 * - Native minimize only happens via the standard Windows Minimize button;
 *   restore happens via the Windows taskbar.
 */

export type PreviewMode = "embedded" | "popout" | "fullscreen";

export const POPOUT_WINDOW_LABEL = "aspectshift-preview";
export const POPOUT_WINDOW_TITLE = "AspectShift - Preview";
export const POPOUT_WINDOW_WIDTH = 800;
export const POPOUT_WINDOW_HEIGHT = 800;
export const POPOUT_ROUTE_HASH = "#/preview-popout";

/** Tauri event channels for the pop-out editing session. */
export const PREVIEW_POPOUT_UPDATE_EVENT =
  "aspectshift:preview-popout:update";
export const PREVIEW_POPOUT_APPLY_EVENT = "aspectshift:preview-popout:apply";
export const PREVIEW_POPOUT_CANCEL_EVENT =
  "aspectshift:preview-popout:cancel";
export const PREVIEW_POPOUT_OPENED_EVENT =
  "aspectshift:preview-popout:opened";

/**
 * Dedicated Tauri event channel for playback *commands* (pop-out -> main).
 *
 * PLAYBACK OWNERSHIP INVARIANT (must be preserved):
 *
 * > There is exactly ONE authoritative preview playback owner. The main
 * > window owns the authoritative playback state (`playing`, position,
 * > rate). The pop-out never owns the media clock: it renders the
 * > authoritative state and routes user playback actions back as commands.
 *
 * This channel carries intent (`toggle` / `play` / `pause`), never mirrored
 * `currentTime` chasing. State pushes in the opposite direction
 * (main -> pop-out `playing`) travel on `PREVIEW_POPOUT_UPDATE_EVENT`.
 * Editing/state sync (`effects`, `previewVolume`, `previewLayout`, draft
 * Apply/Cancel) also travels on the pop-out update/apply/cancel channels
 * and is intentionally untouched by the playback refactor.
 *
 * A DOM `<video>` node cannot belong to two Tauri WebViews at once, so true
 * cross-WebView frame sharing is impractical here. The closest correct
 * architecture is therefore single-active-renderer with explicit handoff:
 * only one host (`embedded` in the main window OR `popout` in the native
 * window) mounts `VideoCanvas` at a time, so only one `<video>` decoder
 * ever advances. There is no polling, no `setInterval`, no per-frame React
 * state, and no `currentTime` drift-correction loop.
 */
export const PREVIEW_PLAYBACK_COMMAND_EVENT =
  "aspectshift:preview-playback:command";

/** localStorage keys for the pop-out session (Tauri + browser fallback). */
export const POPOUT_SESSION_STORAGE_KEY = "aspectshift.preview-popout-session";
export const POPOUT_STATE_EVENT_KEY = "aspectshift.preview-popout-state";
/**
 * localStorage fallback key for playback commands (browser + Tauri backup).
 * The Tauri event above is primary; this storage key covers browser use.
 */
export const PLAYBACK_COMMAND_STORAGE_KEY =
  "aspectshift.preview-playback-command";

export interface PopoutPreviewSession {
  version: 1;
  /** Committed overlay/effects snapshot taken at Pop Out time (draft base). */
  effects: unknown;
  previewVolume: number;
  playing: boolean;
  playbackRate: number;
  currentTime: number;
  videoSrc: string;
  orientation: unknown;
  previewLayout: unknown;
  showGuides: boolean;
  showSafeFrames: boolean;
  createdAt: number;
}

export interface PopoutDraftUpdate {
  version: 1;
  effects?: unknown;
  previewVolume?: number;
  playing?: boolean;
  currentTime?: number;
}

export function isTauriRuntime(): boolean {
  try {
    return (
      typeof window !== "undefined" &&
      "__TAURI_INTERNALS__" in window &&
      !!(window as unknown as Record<string, unknown>).__TAURI_INTERNALS__
    );
  } catch {
    return false;
  }
}

/**
 * Synchronous pop-out route detection (safe during initial render).
 * The pop-out WebviewWindow is opened with `#/preview-popout` in its URL so
 * the same frontend bundle can render only the preview host (no app shell).
 */
export function isPopoutRouteSync(): boolean {
  try {
    const hash = window.location.hash ?? "";
    if (hash.includes("preview-popout")) return true;
    const search = window.location.search ?? "";
    if (search.includes("preview-popout")) return true;
    return false;
  } catch {
    return false;
  }
}

export function getPopoutUrl(): string {
  try {
    const url = new URL(window.location.href);
    // Both a query param and the hash: some hosts strip the hash when
    // opening a new window, so the pop-out route is detectable either way.
    url.searchParams.set("preview-popout", "1");
    url.hash = "/preview-popout";
    return url.toString();
  } catch {
    return `?preview-popout=1${POPOUT_ROUTE_HASH}`;
  }
}

export function readPopoutSession(): PopoutPreviewSession | null {
  try {
    const raw = window.localStorage.getItem(POPOUT_SESSION_STORAGE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as PopoutPreviewSession;
    if (!parsed || parsed.version !== 1) return null;
    return parsed;
  } catch {
    return null;
  }
}

export function writePopoutSession(session: PopoutPreviewSession): void {
  try {
    window.localStorage.setItem(
      POPOUT_SESSION_STORAGE_KEY,
      JSON.stringify(session),
    );
  } catch {
    // Session persistence is best-effort (private mode, quota, etc.).
  }
}

export function clearPopoutSession(): void {
  try {
    window.localStorage.removeItem(POPOUT_SESSION_STORAGE_KEY);
  } catch {
    // Best-effort cleanup only.
  }
}

export interface PopoutSyncFields {
  effects?: unknown;
  previewVolume?: number;
  playing?: boolean;
  currentTime?: number;
  previewLayout?: unknown;
}

/**
 * Playback command sent by the pop-out to the authoritative owner.
 * `playing`/`currentTime` must NOT be mirrored as continuous state in the
 * pop-out -> main direction; the pop-out sends intent here and the main
 * window applies it to its single authoritative `previewPlaying` state,
 * which then pushes back to the pop-out as a state update.
 */
export type PreviewPlaybackCommandKind = "play" | "pause" | "toggle";

export interface PreviewPlaybackCommand {
  source: "popout";
  command: PreviewPlaybackCommandKind;
  at: number;
}

/**
 * Broadcast a draft update to the other window (storage-event channel,
 * which is the primary channel in browsers and the backup in Tauri).
 * The envelope carries kind/source so both hosts' listeners accept it.
 *
 * Main -> pop-out `playing` pushes via this helper are authoritative state
 * pushes (owner to renderer). Pop-out -> main `playing`/`currentTime`
 * mirroring through this channel is obsolete: the pop-out must use
 * `writePlaybackCommandEvent` instead. `currentTime` travels here only as
 * a one-time handoff snapshot (pop-out open, Apply/Cancel close), never as
 * continuous chasing.
 */
export function writePopoutUpdateEvent(
  source: "main" | "popout",
  fields: PopoutSyncFields,
): void {
  try {
    window.localStorage.setItem(
      POPOUT_STATE_EVENT_KEY,
      JSON.stringify({ kind: "update", source, ...fields, at: Date.now() }),
    );
  } catch {
    // Best-effort only; Tauri events are the primary channel there.
  }
}

/** Broadcast a draft update to the other window (storage-event fallback). */
export function broadcastPopoutDraft(update: PopoutDraftUpdate): void {
  writePopoutUpdateEvent("main", {
    effects: update.effects,
    previewVolume: update.previewVolume,
    playing: update.playing,
    currentTime: update.currentTime,
  });
}

/**
 * Send a playback intent from the pop-out to the authoritative owner
 * (storage-event fallback; Tauri event is primary via `emitPopoutEvent`
 * with `PREVIEW_PLAYBACK_COMMAND_EVENT`). Synchronous and render-free.
 */
export function writePlaybackCommandEvent(
  command: PreviewPlaybackCommandKind,
): void {
  try {
    const payload: PreviewPlaybackCommand = {
      source: "popout",
      command,
      at: Date.now(),
    };
    window.localStorage.setItem(
      PLAYBACK_COMMAND_STORAGE_KEY,
      JSON.stringify(payload),
    );
  } catch {
    // Best-effort only; Tauri events are the primary channel there.
  }
}

export function readPlaybackCommandEvent(): PreviewPlaybackCommand | null {
  try {
    const raw = window.localStorage.getItem(PLAYBACK_COMMAND_STORAGE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as PreviewPlaybackCommand;
    if (
      !parsed ||
      parsed.source !== "popout" ||
      (parsed.command !== "play" &&
        parsed.command !== "pause" &&
        parsed.command !== "toggle")
    ) {
      return null;
    }
    return parsed;
  } catch {
    return null;
  }
}

export async function emitPopoutEvent(
  event: string,
  payload?: unknown,
): Promise<void> {
  if (!isTauriRuntime()) return;
  try {
    const { emit } = await import("@tauri-apps/api/event");
    await emit(event, payload);
  } catch {
    // Event delivery is best-effort; localStorage fallback covers browser use.
  }
}

/**
 * Create (or focus, if it already exists) the genuine native pop-out window.
 *
 * - Title: `AspectShift - Preview`
 * - Default size: 800 x 800
 * - Resizable, centered, normal native title bar (min/max/close).
 * - No blur-to-minimize / click-outside behavior is installed here — focus
 *   state and minimized state stay independent by design.
 */
export async function showPopoutWindow(): Promise<boolean> {
  if (!isTauriRuntime()) return false;
  try {
    const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
    const existing = await WebviewWindow.getByLabel(POPOUT_WINDOW_LABEL);
    if (existing) {
      try {
        await existing.show();
      } catch {
        // Already visible.
      }
      try {
        await existing.setFocus();
      } catch {
        // Focus is best-effort.
      }
      return true;
    }
    const popout = new WebviewWindow(POPOUT_WINDOW_LABEL, {
      url: getPopoutUrl(),
      title: POPOUT_WINDOW_TITLE,
      width: POPOUT_WINDOW_WIDTH,
      height: POPOUT_WINDOW_HEIGHT,
      resizable: true,
      center: true,
      decorations: true,
      visible: true,
    });
    // Wait for the native creation result so a failure is reported instead
    // of leaving the main window in pop-out mode with no visible host.
    // Timeout never assumes success: it falls back to ground truth (does
    // the window label actually exist yet?).
    const created = await new Promise<boolean>((resolve) => {
      let settled = false;
      const timer = window.setTimeout(() => {
        if (!settled) {
          settled = true;
          WebviewWindow.getByLabel(POPOUT_WINDOW_LABEL)
            .then((w) => resolve(w !== null))
            .catch(() => resolve(false));
        }
      }, 3000);
      popout
        .once("tauri://created", () => {
          if (!settled) {
            settled = true;
            window.clearTimeout(timer);
            resolve(true);
          }
        })
        .catch(() => {});
      popout
        .once("tauri://error", (event) => {
          if (!settled) {
            settled = true;
            window.clearTimeout(timer);
            console.error(
              "[preview-popout] failed to create native window:",
              event?.payload ?? event,
            );
            resolve(false);
          }
        })
        .catch(() => {});
    });
    if (!created) return false;
    try {
      await popout.show();
    } catch {
      // Already visible.
    }
    try {
      await popout.setFocus();
    } catch {
      // Focus is best-effort.
    }
    return true;
  } catch (err) {
    console.error("[preview-popout] failed to create native window:", err);
    return false;
  }
}

export async function focusPopoutWindow(): Promise<void> {
  if (!isTauriRuntime()) return;
  try {
    const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
    const existing = await WebviewWindow.getByLabel(POPOUT_WINDOW_LABEL);
    if (!existing) return;
    try {
      await existing.show();
    } catch {
      // Already visible.
    }
    try {
      await existing.setFocus();
    } catch {
      // Focus is best-effort.
    }
  } catch {
    // Best-effort only.
  }
}

export async function closePopoutWindow(): Promise<void> {
  if (!isTauriRuntime()) return;
  try {
    const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
    const existing = await WebviewWindow.getByLabel(POPOUT_WINDOW_LABEL);
    if (!existing) return;
    await existing.close();
  } catch {
    // Best-effort cleanup only.
  }
}

/** Close the *current* window (used by the pop-out for X/Cancel/Apply). */
export async function closeCurrentWindow(): Promise<void> {
  if (!isTauriRuntime()) {
    // Browser fallback popup: script-opened windows may close themselves.
    try {
      window.close();
    } catch {
      // Ignore.
    }
    return;
  }
  try {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow().close();
  } catch {
    // Best-effort only.
    try {
      window.close();
    } catch {
      // Ignore.
    }
  }
}

/**
 * Remember whether the current (pop-out) window is maximized/fullscreen so a
 * custom fullscreen state can restore the exact previous pop-out state on Esc
 * while remaining in pop-out mode.
 */
export async function rememberPopoutBounds(): Promise<{
  maximized: boolean;
  fullscreen: boolean;
}> {
  const fallback = { maximized: false, fullscreen: false };
  if (!isTauriRuntime()) return fallback;
  try {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    const win = getCurrentWindow();
    const [maximized, fullscreen] = await Promise.all([
      win.isMaximized().catch(() => false),
      win.isFullscreen().catch(() => false),
    ]);
    return { maximized, fullscreen };
  } catch {
    return fallback;
  }
}

export async function setCurrentWindowFullscreen(
  fullscreen: boolean,
): Promise<void> {
  if (!isTauriRuntime()) return;
  try {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow().setFullscreen(fullscreen);
  } catch {
    // CSS fullscreen fallback is handled by the pop-out root component.
  }
}
