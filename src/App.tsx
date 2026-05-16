import { useState, useEffect, useMemo, useRef, useCallback } from "react";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import type {
  AppConfig,
  AspectRatio,
  AspectRatioTarget,
  BatchProgress,
  CustomPreset,
  EncodingProfile,
  FileProgress,
  FileReadiness,
  LogoOptions,
  LogoPosition,
  OrientationInfo,
  OutputFormat,
  OutputJob,
  PlatformPreset,
  SelectionMetadata,
  TargetType,
  VideoEffectsSettings,
  VideoProgress,
  VideoTransform,
} from "./types/backend";
import "./App.css";
import { VideoCanvas } from "./components/VideoCanvas";
import { PresetsPanel, type DisplayPreset } from "./components/PresetsPanel";
import {
  getSourceDisplaySize,
  resolveVideoGeometry,
} from "./utils/resolvedVideoGeometry";

// ── Types ─────────────────────────────────────────────────────

/**
 * The tagged-union shape that `get_all_presets` actually returns from Rust.
 * Matches VideoPresetDTO in Rust: #[serde(tag = "kind", rename_all = "camelCase")]
 */
type VideoPresetDTO =
  | ({ kind: "platform" } & PlatformPreset)
  | ({ kind: "custom" } & CustomPreset);

type SelectionItem =
  | { type: "aspectRatio"; id: AspectRatio }
  | { type: "preset"; id: string };

interface LogEntry {
  time: string;
  msg: string;
  type: "info" | "success" | "error" | "warn" | "accent";
}

interface BackendError {
  code?: string;
  message?: string;
}

// ── Constants ────────────────────────────────────────────────
const DEFAULT_ENCODING: EncodingProfile = {
  crf: 18,
  qualityPreset: "standard",
  speedPreset: "medium",
  audioBitrate: "128k",
};

const DEFAULT_EFFECTS: VideoEffectsSettings = {
  blur: false,
  overlays: null,
  subtitles: null,
  colorFilter: null,
  blurSigma: 20.0,
  removeAudio: false,
  exportSubtitles: false,
  burnSubtitles: false,
  skipExisting: true,
  outputFormat: "mp4",
  logo: null,
  transform: { rotate: 0, flip_h: false, flip_v: false },
};

const ASPECT_RATIOS: { label: string; value: AspectRatio }[] = [
  { label: "9:16", value: "ratio9x16" },
  { label: "1:1", value: "ratio1x1" },
  { label: "4:5", value: "ratio4x5" },
  { label: "2:3", value: "ratio2x3" },
  { label: "16:9", value: "ratio16x9" },
];

export const RATIO_DISPLAY: Record<AspectRatio, string> = {
  ratio9x16: "9:16",
  ratio1x1: "1:1",
  ratio4x5: "4:5",
  ratio2x3: "2:3",
  ratio16x9: "16:9",
};

const RATIO_VALUE: Record<AspectRatio, number> = {
  ratio9x16: 9 / 16,
  ratio1x1: 1,
  ratio4x5: 4 / 5,
  ratio2x3: 2 / 3,
  ratio16x9: 16 / 9,
};

const SPEED_PRESETS = [
  "ultrafast",
  "superfast",
  "veryfast",
  "faster",
  "fast",
  "medium",
  "slow",
  "slower",
  "veryslow",
] as const;

const AUDIO_BITRATE_CANDIDATES = [
  "64k",
  "96k",
  "128k",
  "160k",
  "192k",
  "256k",
  "320k",
  "384k",
] as const;
const DEFAULT_PREVIEW_VOLUME = 20;

// ── Helpers ──────────────────────────────────────────────────

function formatTime(ts: Date) {
  return ts.toTimeString().slice(0, 8);
}

function formatETA(secs: number): string {
  if (!isFinite(secs) || secs <= 0) return "--:--";
  const m = Math.floor(secs / 60);
  const s = Math.floor(secs % 60);
  return m > 0 ? `${m}m ${s}s` : `${s}s`;
}

function formatDuration(secs: number): string {
  if (!isFinite(secs) || secs <= 0) return "--:--";
  const total = Math.floor(secs);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  if (h > 0) return `${h}h ${m}m ${s}s`;
  if (m > 0) return `${m}m ${s}s`;
  return `${s}s`;
}

function basename(path: string): string {
  return path.split(/[/\\]/).pop() || path;
}

function errorMessage(e: unknown): string {
  if (typeof e === "string") return e;
  if (e && typeof e === "object") {
    const maybe = e as BackendError;
    if (typeof maybe.message === "string") return maybe.message;
  }
  return String(e);
}

function isVideoPath(path: string): boolean {
  return /\.(mp4|mov|mkv|avi|webm)$/i.test(path);
}

function normalizeTransform(
  transform?: VideoTransform | null,
): VideoTransform | null {
  if (!transform) return null;
  return {
    rotate: transform.rotate ?? 0,
    flip_h: !!transform.flip_h,
    flip_v: !!transform.flip_v,
  };
}

function isOddQuarterTurn(rotate: number): boolean {
  const normalized = ((rotate % 360) + 360) % 360;
  return normalized === 90 || normalized === 270;
}

function transformToUiFlips(transform?: VideoTransform | null): {
  flipH: boolean;
  flipV: boolean;
} {
  const rotate = transform?.rotate ?? 0;
  const flipH = !!transform?.flip_h;
  const flipV = !!transform?.flip_v;
  if (!isOddQuarterTurn(rotate)) {
    return { flipH, flipV };
  }
  // At 90/270deg, backend flip axes are swapped relative to displayed axes.
  return { flipH: flipV, flipV: flipH };
}

function uiFlipsToTransform(
  transform: VideoTransform | null | undefined,
  uiFlipH: boolean,
  uiFlipV: boolean,
): VideoTransform {
  const rotate = transform?.rotate ?? 0;
  const base: VideoTransform = {
    rotate,
    flip_h: false,
    flip_v: false,
  };
  if (!isOddQuarterTurn(rotate)) {
    base.flip_h = uiFlipH;
    base.flip_v = uiFlipV;
    return base;
  }
  base.flip_h = uiFlipV;
  base.flip_v = uiFlipH;
  return base;
}

function normalizeLogo(logo: LogoOptions | null): LogoOptions | null {
  if (!logo || !logo.enabled || !logo.path) return null;
  return {
    enabled: true,
    position: logo.position,
    opacity: Number(logo.opacity),
    gap: Number(logo.gap),
    scale: Number(logo.scale),
    path: logo.path,
  };
}

function deepClone<T>(value: T): T {
  return structuredClone(value);
}

function generateId(): string {
  return crypto.randomUUID();
}

function normalizeEffects(effects: VideoEffectsSettings): VideoEffectsSettings {
  return {
    ...effects,
    logo: normalizeLogo(effects.logo ?? null),
    transform: normalizeTransform(effects.transform),
  };
}

function parseBitrateKbps(value: string): number | null {
  const normalized = value.trim().toLowerCase();
  if (!normalized.endsWith("k")) return null;
  const n = Number.parseInt(normalized.slice(0, -1), 10);
  return Number.isFinite(n) ? n : null;
}

function audioBitrateLabel(value: string): string {
  const kbps = parseBitrateKbps(value);
  if (kbps === null) return value;
  if (kbps <= 96) return `${kbps}k (Low)`;
  if (kbps <= 192) return `${kbps}k (Standard)`;
  if (kbps <= 320) return `${kbps}k (High)`;
  return `${kbps}k (Max)`;
}

/**
 * Resolves JobStatus (which can be a string or {error: string}) to a stable
 * string key safe for CSS class names and display logic.
 */
function resolveJobStatusKey(
  status: FileProgress["status"],
): "queued" | "pending" | "processing" | "completed" | "failed" | "cancelled" {
  if (typeof status === "string") {
    if (status === "queued") return "queued";
    if (status === "pending") return "pending";
    if (status === "processing") return "processing";
    if (status === "completed") return "completed";
    if (status === "cancelled") return "cancelled";
  }
  if (typeof status === "object" && status !== null && "error" in status) {
    return "failed";
  }
  return "queued";
}

function getJobStatusError(status: FileProgress["status"]): string | null {
  if (typeof status === "object" && status !== null && "error" in status) {
    return status.error;
  }
  return null;
}

/**
 * Normalizes the VideoPresetDTO tagged union returned by get_all_presets.
 * Produces a flat list of DisplayPreset items usable by the UI without
 * knowledge of the internal union discriminant.
 */
function normalizeDTOsToDisplayPresets(dtos: VideoPresetDTO[]): {
  platformPresets: PlatformPreset[];
  customPresets: CustomPreset[];
  displayPresets: DisplayPreset[];
} {
  const platformPresets: PlatformPreset[] = [];
  const customPresets: CustomPreset[] = [];

  for (const dto of dtos) {
    if (dto.kind === "platform") {
      // eslint-disable-next-line @typescript-eslint/no-unused-vars
      const { kind: _kind, ...preset } = dto;
      platformPresets.push(preset as PlatformPreset);
    } else if (dto.kind === "custom") {
      // eslint-disable-next-line @typescript-eslint/no-unused-vars
      const { kind: _kind, ...preset } = dto;
      customPresets.push(preset as CustomPreset);
    }
  }

  const displayPresets: DisplayPreset[] = [
    ...platformPresets.map(
      (p): DisplayPreset => ({
        id: p.id,
        name: p.name,
        ratioLabel: RATIO_DISPLAY[p.ratio] ?? p.ratio,
        resolution: p.platformConfig
          ? `${p.platformConfig.targetWidth}x${p.platformConfig.targetHeight}`
          : undefined,
        isBuiltin: p.isBuiltin,
        isCustom: false,
        source: p,
      }),
    ),
    ...customPresets.map(
      (p): DisplayPreset => ({
        id: p.id,
        name: p.name,
        ratioLabel: RATIO_DISPLAY[p.ratio] ?? p.ratio,
        isBuiltin: false,
        isCustom: true,
        source: p,
      }),
    ),
  ];

  return { platformPresets, customPresets, displayPresets };
}

// ── Toggle Component ─────────────────────────────────────────
function Toggle({
  checked,
  onChange,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <label className="toggle">
      <input
        type="checkbox"
        checked={checked}
        onChange={(e) => onChange(e.target.checked)}
      />
      <span className="toggle-track" />
    </label>
  );
}

// ── Main App ─────────────────────────────────────────────────
export default function App() {
  // Theme
  const [theme, setTheme] = useState<"day" | "night">(() => {
    return (
      (document.documentElement.getAttribute("data-theme") as
        | "day"
        | "night") || "night"
    );
  });

  // Files
  const [inputFile, setInputFile] = useState("");
  const [previewFile, setPreviewFile] = useState("");
  const [batchFiles, setBatchFiles] = useState<string[]>([]);
  const [folderPreviewFiles, setFolderPreviewFiles] = useState<string[]>([]);
  const [outputDir, setOutputDir] = useState("");
  const [enableSubfolders, setEnableSubfolders] = useState(false);
  const [isDragOver, setIsDragOver] = useState(false);

  // Selection
  const [selectionHistory, setSelectionHistory] = useState<SelectionItem[]>([]);
  const [selectedRatios, setSelectedRatios] = useState<AspectRatio[]>([]);
  const [selectedPresetIds, setSelectedPresetIds] = useState<string[]>([]);
  const [aspectRatioTargets, setAspectRatioTargets] = useState<
    AspectRatioTarget[]
  >([]);

  // Resolved preset stores — kept in sync with get_all_presets response
  const [platformPresets, setPlatformPresets] = useState<PlatformPreset[]>([]);
  const [customPresets, setCustomPresets] = useState<CustomPreset[]>([]);
  const [displayPresets, setDisplayPresets] = useState<DisplayPreset[]>([]);

  // Settings State
  const [encodingState, setEncodingState] =
    useState<EncodingProfile>(DEFAULT_ENCODING);
  const [effectsState, setEffectsState] =
    useState<VideoEffectsSettings>(DEFAULT_EFFECTS);

  // Custom preset builder
  const [newPresetName, setNewPresetName] = useState("");

  // Video / batch state
  const [orientation, setOrientation] = useState<OrientationInfo | null>(null);
  const [fileReadiness, setFileReadiness] = useState<FileReadiness | null>(
    null,
  );
  const [batchProgress, setBatchProgress] = useState<BatchProgress | null>(
    null,
  );
  const [videoProgresses, setVideoProgresses] = useState<
    Record<string, number>
  >({});
  const [activeSessionId, setActiveSessionId] = useState<string | null>(null);
  const activeSessionIdRef = useRef<string | null>(null);
  const startRequestedRef = useRef(false);

  // UI tabs
  const [rightTab, setRightTab] = useState<"queue" | "log">("queue");
  const [settingsTab, setSettingsTab] = useState<
    "effects" | "encode" | "presets"
  >("effects");

  // Log
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const logRef = useRef<HTMLDivElement>(null);

  // Guides overlay
  const [showGuides, setShowGuides] = useState(true);
  const [showSafeFrames, setShowSafeFrames] = useState(false);
  const [previewVolume, setPreviewVolume] = useState<number>(
    DEFAULT_PREVIEW_VOLUME,
  );
  const [volumeSliderActive, setVolumeSliderActive] = useState(false);
  const [volumeSliderInteracting, setVolumeSliderInteracting] = useState(false);
  const [volumeSliderHovering, setVolumeSliderHovering] = useState(false);
  const [volumeSliderFocusWithin, setVolumeSliderFocusWithin] = useState(false);
  const volumeCollapseTimer = useRef<ReturnType<typeof setTimeout> | null>(
    null,
  );
  const previewVolumeRef = useRef<HTMLDivElement | null>(null);

  const cancelVolumeCollapse = useCallback(() => {
    if (volumeCollapseTimer.current) {
      clearTimeout(volumeCollapseTimer.current);
      volumeCollapseTimer.current = null;
    }
  }, []);

  const scheduleVolumeCollapse = useCallback(() => {
    cancelVolumeCollapse();
    volumeCollapseTimer.current = setTimeout(() => {
      setVolumeSliderActive(false);
    }, 1200);
  }, [cancelVolumeCollapse]);

  const handleVolumeChange = useCallback((val: number) => {
    setPreviewVolume(val);
    setVolumeSliderActive(true);
  }, []);

  useEffect(() => {
    if (!volumeSliderActive) return;
    if (
      volumeSliderInteracting ||
      volumeSliderHovering ||
      volumeSliderFocusWithin
    ) {
      cancelVolumeCollapse();
      return;
    }
    scheduleVolumeCollapse();
  }, [
    volumeSliderActive,
    volumeSliderInteracting,
    volumeSliderHovering,
    volumeSliderFocusWithin,
    cancelVolumeCollapse,
    scheduleVolumeCollapse,
  ]);

  useEffect(() => {
    const onPointerDown = (e: PointerEvent) => {
      if (!previewVolumeRef.current) return;
      if (previewVolumeRef.current.contains(e.target as Node)) return;
      cancelVolumeCollapse();
      setVolumeSliderInteracting(false);
      setVolumeSliderActive(false);
    };
    const onPointerUp = () => {
      setVolumeSliderInteracting(false);
    };

    window.addEventListener("pointerdown", onPointerDown);
    window.addEventListener("pointerup", onPointerUp);
    return () => {
      window.removeEventListener("pointerdown", onPointerDown);
      window.removeEventListener("pointerup", onPointerUp);
    };
  }, [cancelVolumeCollapse]);

  useEffect(() => {
    return () => {
      cancelVolumeCollapse();
    };
  }, [cancelVolumeCollapse]);

  // Derived: last active selection drives preview ratio.
  // Fallback to existing checked items so restored config stays in sync.
  const activeSelection = useMemo<SelectionItem | null>(() => {
    const fromHistory = selectionHistory[selectionHistory.length - 1];
    if (fromHistory) return fromHistory;
    const lastPresetId = selectedPresetIds[selectedPresetIds.length - 1];
    if (lastPresetId) return { type: "preset", id: lastPresetId };
    const lastRatio = selectedRatios[selectedRatios.length - 1];
    if (lastRatio) return { type: "aspectRatio", id: lastRatio };
    return null;
  }, [selectionHistory, selectedPresetIds, selectedRatios]);
  const uiFlips = useMemo(
    () => transformToUiFlips(effectsState.transform),
    [effectsState.transform],
  );

  // Whether orientation data has been fetched for the current inputFile.
  // null = not yet resolved (async invoke in flight or no file).
  // Used to suppress the preview box until we know the real geometry.
  const effectivePreviewRatio = useMemo((): number | null => {
    if (!activeSelection) {
      // No ratio selected: use source geometry resolved through the same
      // transform-aware pipeline used by preview rendering.
      const src = getSourceDisplaySize(orientation);
      if (!src) return null;
      const resolved = resolveVideoGeometry({
        orientation,
        transform: effectsState.transform,
        targetAspectRatio: src.width / src.height,
        frameWidth: src.width,
        frameHeight: src.height,
        fitMode: "cover",
      });
      if (resolved) return resolved.displayAspectRatio;
      return null; // pending — VideoCanvas will render nothing
    }
    if (activeSelection.type === "aspectRatio") {
      return RATIO_VALUE[activeSelection.id];
    }
    const p = platformPresets.find((x) => x.id === activeSelection.id);
    if (p) return RATIO_VALUE[p.ratio];
    const cp = customPresets.find((x) => x.id === activeSelection.id);
    if (cp) return RATIO_VALUE[cp.ratio];
    // Preset selected but not resolved yet — same guard
    return null;
  }, [
    activeSelection,
    orientation,
    platformPresets,
    customPresets,
    effectsState.transform,
  ]);

  // Sync encoding when a platform preset becomes the active selection
  useEffect(() => {
    if (!activeSelection || activeSelection.type !== "preset") return;
    const p = platformPresets.find((x) => x.id === activeSelection.id);
    if (p) {
      setEncodingState(deepClone(p.encoding));
      return;
    }
    const cp = customPresets.find((x) => x.id === activeSelection.id);
    if (cp) {
      setEncodingState(deepClone(cp.encoding));
    }
  }, [activeSelection, platformPresets, customPresets]);

  // ── Theme ──────────────────────────────────────────────────
  useEffect(() => {
    const html = document.documentElement;
    // Add the transitioning class just before the attribute swap so every
    // element inherits color/background transitions for the duration of the
    // switch. Remove it after transitions complete (matches the 0.22s CSS
    // duration with a small buffer) so normal interactions stay snappy.
    html.classList.add("theme-transitioning");
    html.setAttribute("data-theme", theme);
    localStorage.setItem("asp-theme", theme);
    const timer = window.setTimeout(() => {
      html.classList.remove("theme-transitioning");
    }, 260);
    return () => window.clearTimeout(timer);
  }, [theme]);

  // ── Init ──────────────────────────────────────────────────
  useEffect(() => {
    loadConfig();
    loadPresets();
    loadAspectRatioTargets();
    invoke<BatchProgress>("get_batch_status")
      .then((status) => {
        if (!status.sessionId) return;
        setBatchProgress(status);
        setActiveSessionId(status.sessionId);
        activeSessionIdRef.current = status.sessionId;
        startRequestedRef.current = false;
      })
      .catch(() => {});
  }, []);

  // ── Persistence ───────────────────────────────────────────
  const isInitialLoad = useRef(true);

  const loadConfig = async () => {
    try {
      const config = await invoke<AppConfig>("get_config");
      if (config.lastOutputDir) setOutputDir(config.lastOutputDir);
      if (config.enableSubfolders !== null)
        setEnableSubfolders(config.enableSubfolders ?? false);
      setPreviewVolume(
        Math.max(
          0,
          Math.min(100, config.previewVolume ?? DEFAULT_PREVIEW_VOLUME),
        ),
      );

      if (config.logoPath || config.logoOpacity !== null) {
        setEffectsState((prev) => ({
          ...prev,
          logo: config.logoPath
            ? {
                enabled: true,
                path: config.logoPath,
                opacity: config.logoOpacity ?? 1.0,
                position:
                  config.logoPosition ?? prev.logo?.position ?? "bottom_right",
                gap: prev.logo?.gap ?? 20,
                scale: prev.logo?.scale ?? 0.15,
              }
            : null,
          blur: config.blur ?? prev.blur,
          blurSigma: config.blurSigma ?? prev.blurSigma,
        }));
      }

      // Migrate old single selection
      let ratiosToRestore = [...config.selectedRatioIds];
      let presetsToRestore = [...config.selectedPresetIds];

      if (
        config.lastPresetId &&
        presetsToRestore.length === 0 &&
        ratiosToRestore.length === 0
      ) {
        restorePresetRef.current = config.lastPresetId;
      } else {
        setSelectedRatios(ratiosToRestore);
        setSelectedPresetIds(presetsToRestore);
      }

      isInitialLoad.current = false;
    } catch (e) {
      addLog(`Failed to load config: ${errorMessage(e)}`, "error");
      isInitialLoad.current = false;
    }
  };

  const restorePresetRef = useRef<string | null>(null);

  useEffect(() => {
    if (
      restorePresetRef.current &&
      (platformPresets.length > 0 || customPresets.length > 0)
    ) {
      const pid = restorePresetRef.current;
      handleTogglePreset(pid);
      restorePresetRef.current = null;
    }
  }, [platformPresets, customPresets, aspectRatioTargets]);

  const [lastInputDir, setLastInputDir] = useState<string | null>(null);

  useEffect(() => {
    if (isInitialLoad.current) return;

    const timer = setTimeout(async () => {
      const config: AppConfig = {
        lastInputDir: lastInputDir || null,
        lastOutputDir: outputDir || null,
        lastPresetId: null,
        selectedRatioIds: selectedRatios,
        selectedPresetIds: selectedPresetIds,
        logoPath: effectsState.logo?.path || null,
        logoOpacity: effectsState.logo?.opacity ?? null,
        logoPosition: effectsState.logo?.position ?? null,
        blur: effectsState.blur ?? null,
        blurSigma: effectsState.blurSigma ?? null,
        enableSubfolders: enableSubfolders,
        previewVolume: previewVolume,
      };
      try {
        await invoke("update_config", { config });
      } catch (e) {
        console.error("Failed to save config", e);
      }
    }, 500);

    return () => clearTimeout(timer);
  }, [
    outputDir,
    selectedRatios,
    selectedPresetIds,
    effectsState.logo,
    effectsState.blur,
    effectsState.blurSigma,
    enableSubfolders,
    lastInputDir,
    previewVolume,
  ]);

  const handleResetToDefaults = async () => {
    try {
      await invoke("reset_config");
      setOutputDir("");
      setLastInputDir(null);
      setEffectsState(DEFAULT_EFFECTS);
      setEncodingState(DEFAULT_ENCODING);
      setSelectionHistory([]);
      setSelectedRatios([]);
      setSelectedPresetIds([]);
      setPreviewVolume(DEFAULT_PREVIEW_VOLUME);
      addLog("Settings reset to defaults", "info");
    } catch (e) {
      addLog(`Reset failed: ${errorMessage(e)}`, "error");
    }
  };

  // ── Tauri Event Listeners ─────────────────────────────────
  useEffect(() => {
    activeSessionIdRef.current = activeSessionId;
  }, [activeSessionId]);

  useEffect(() => {
    const unsubscribers: Array<() => void> = [];
    let disposed = false;

    const setupListeners = async () => {
      const u1 = await listen<BatchProgress>("batch://progress", (e) => {
        const sessionId = e.payload.sessionId;
        if (!sessionId) return;

        if (!activeSessionIdRef.current) {
          if (startRequestedRef.current && e.payload.status === "processing") {
            activeSessionIdRef.current = sessionId;
            setActiveSessionId(sessionId);
            startRequestedRef.current = false;
          } else {
            return;
          }
        }

        if (activeSessionIdRef.current !== sessionId) return;
        setBatchProgress(e.payload);

        if (e.payload.status === "completed") {
          addLog("Batch complete ✓", "success");
          startRequestedRef.current = false;
        } else if (e.payload.status === "cancelled") {
          addLog("Batch cancelled by user", "warn");
          startRequestedRef.current = false;
        } else if (e.payload.status === "failed") {
          addLog("Batch finished with errors", "error");
          setRightTab("log");
          startRequestedRef.current = false;
        }
      });
      if (disposed) u1();
      else unsubscribers.push(u1);

      const u2 = await listen<FileProgress>("batch://file-status", (e) => {
        if (!activeSessionIdRef.current) return;
        if (e.payload.sessionId !== activeSessionIdRef.current) return;
        const name = basename(e.payload.filePath);
        const ratio = RATIO_DISPLAY[e.payload.ratio] ?? e.payload.ratio;
        const statusKey = resolveJobStatusKey(e.payload.status);
        const err = getJobStatusError(e.payload.status);

        if (statusKey === "processing")
          addLog(`Processing: ${name} → ${ratio}`, "accent");
        else if (statusKey === "completed")
          addLog(`Done: ${name} → ${ratio}`, "success");
        else if (statusKey === "cancelled")
          addLog(`Cancelled: ${name} → ${ratio}`, "warn");
        else if (statusKey === "failed")
          addLog(`Failed: ${name} — ${err ?? "unknown error"}`, "error");
      });
      if (disposed) u2();
      else unsubscribers.push(u2);

      const u3 = await listen<VideoProgress>("video://progress", (e) => {
        if (!activeSessionIdRef.current) return;
        if (e.payload.sessionId !== activeSessionIdRef.current) return;
        setVideoProgresses((prev) => ({
          ...prev,
          [e.payload.jobId]: e.payload.percent,
        }));
      });
      if (disposed) u3();
      else unsubscribers.push(u3);
    };

    setupListeners();
    return () => {
      disposed = true;
      unsubscribers.forEach((u) => u());
    };
  }, []);

  // ── Orientation detect on file change ─────────────────────
  useEffect(() => {
    if (!previewFile || !isVideoPath(previewFile)) {
      setOrientation(null);
      setFileReadiness(null);
      return;
    }

    // Reset orientation immediately so effectivePreviewRatio returns null
    // and VideoCanvas stays hidden until the real geometry arrives.
    setOrientation(null);

    invoke<OrientationInfo>("detect_orientation", { filePath: previewFile })
      .then(setOrientation)
      .catch(() => setOrientation(null));

    invoke<FileReadiness>("check_file_ready", { path: previewFile })
      .then(setFileReadiness)
      .catch((e) => {
        setFileReadiness(null);
        addLog(`File readiness check failed: ${errorMessage(e)}`, "warn");
      });
  }, [previewFile]);

  // ── Auto-scroll log ────────────────────────────────────────
  useEffect(() => {
    if (logRef.current) {
      logRef.current.scrollTop = logRef.current.scrollHeight;
    }
  }, [logs]);

  // ── Helpers ────────────────────────────────────────────────
  const addLog = useCallback((msg: string, type: LogEntry["type"] = "info") => {
    setLogs((prev) => {
      const next = [...prev, { time: formatTime(new Date()), msg, type }];
      return next.length > 200 ? next.slice(-200) : next;
    });
  }, []);

  const loadPresets = async () => {
    try {
      const dtos = await invoke<VideoPresetDTO[]>("get_all_presets");
      const {
        platformPresets: pp,
        customPresets: cp,
        displayPresets: dp,
      } = normalizeDTOsToDisplayPresets(dtos);
      setPlatformPresets(pp);
      setCustomPresets(cp);
      setDisplayPresets(dp);
    } catch (e) {
      addLog(`Failed to load presets: ${errorMessage(e)}`, "error");
    }
  };

  const loadAspectRatioTargets = async () => {
    try {
      const all = await invoke<AspectRatioTarget[]>(
        "get_all_aspect_ratio_targets",
      );
      setAspectRatioTargets(all);
    } catch (e) {
      addLog(
        `Failed to load aspect ratio targets: ${errorMessage(e)}`,
        "error",
      );
    }
  };

  // ── Handlers ───────────────────────────────────────────────

  const handlePickFile = async () => {
    try {
      const sel = await open({
        multiple: true,
        defaultPath: lastInputDir || undefined,
        filters: [
          { name: "Video", extensions: ["mp4", "mov", "mkv", "avi", "webm"] },
        ],
      });
      if (!sel) return;
      const files = Array.isArray(sel) ? sel : [sel];
      const path = files[0];
      const dir = path.substring(
        0,
        Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\")),
      );
      if (dir) setLastInputDir(dir);

      if (files.length === 1) {
        setInputFile(files[0]);
        setPreviewFile(files[0]);
        setBatchFiles([]);
        setFolderPreviewFiles([]);
        addLog(`File selected: ${basename(files[0])}`, "info");
      } else {
        setInputFile(files[0]);
        setPreviewFile(files[0]);
        setBatchFiles(files);
        setFolderPreviewFiles([]);
        addLog(`${files.length} files selected for batch`, "info");
      }
    } catch (e) {
      addLog(`File picker error: ${errorMessage(e)}`, "error");
    }
  };

  const handlePickFolder = async () => {
    try {
      const sel = await open({
        multiple: false,
        directory: true,
        defaultPath: lastInputDir || undefined,
      });
      if (sel && typeof sel === "string") {
        setLastInputDir(sel);
        addLog("Input folder selected, scanning...", "info");
        setBatchFiles([sel]);

        try {
          let videos: string[] = [];
          try {
            videos = await invoke<string[]>("get_videos_in_folder", {
              folderPath: sel,
            });
          } catch {
            const firstVideo = await invoke<string | null>(
              "get_first_video_in_folder",
              { folderPath: sel },
            );
            videos = firstVideo ? [firstVideo] : [];
          }

          setFolderPreviewFiles(videos);
          const firstVideo = videos[0] ?? null;
          if (firstVideo) {
            setPreviewFile(firstVideo);
            setInputFile("");
          } else {
            setPreviewFile("");
            setInputFile("");
            addLog("No valid video files found in selected folder", "warn");
          }
        } catch (scanErr) {
          console.error("Failed to scan folder:", scanErr);
          setPreviewFile("");
          setInputFile("");
          setFolderPreviewFiles([]);
        }
      }
    } catch (e) {
      addLog(`Folder picker error: ${errorMessage(e)}`, "error");
    }
  };

  const handlePickOutputDir = async () => {
    try {
      const sel = await open({
        multiple: false,
        directory: true,
        defaultPath: outputDir || lastInputDir || undefined,
      });
      if (sel && typeof sel === "string") {
        setOutputDir(sel);
        addLog(`Output directory: ${sel}`, "info");
      }
    } catch (e) {
      addLog(`Output dir picker error: ${errorMessage(e)}`, "error");
    }
  };

  const handleOpenOutput = async () => {
    if (!outputDir) return;
    try {
      await invoke("open_output_folder", { path: outputDir });
    } catch (e) {
      addLog(`Could not open folder: ${errorMessage(e)}`, "error");
    }
  };

  const handleDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    setIsDragOver(true);
  }, []);

  const handleDragLeave = useCallback(() => setIsDragOver(false), []);

  const handleDrop = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      setIsDragOver(false);
      const files = Array.from(e.dataTransfer.files)
        .filter((f) => /\.(mp4|mov|mkv|avi|webm)$/i.test(f.name))
        .map((f) => (f as File & { path: string }).path);
      if (files.length === 0) return;
      if (files.length === 1) {
        setInputFile(files[0]);
        setPreviewFile(files[0]);
        setBatchFiles([]);
        setFolderPreviewFiles([]);
      } else {
        setInputFile(files[0]);
        setPreviewFile(files[0]);
        setBatchFiles(files);
        setFolderPreviewFiles([]);
      }
      addLog(`Dropped ${files.length} file(s)`, "info");
    },
    [addLog],
  );

  const handlePickLogo = async () => {
    try {
      const sel = await open({
        multiple: false,
        filters: [
          { name: "Image", extensions: ["png", "jpg", "jpeg", "svg", "webp"] },
        ],
      });
      if (sel && typeof sel === "string") {
        setEffectsState((prev) => ({
          ...prev,
          logo: {
            enabled: true,
            position: prev.logo?.position ?? "bottom_right",
            opacity: prev.logo?.opacity ?? 1,
            gap: prev.logo?.gap ?? 20,
            scale: prev.logo?.scale ?? 0.15,
            path: sel,
          },
        }));
        addLog(`Logo loaded: ${basename(sel)}`, "info");
      }
    } catch (e) {
      addLog(`Logo picker error: ${errorMessage(e)}`, "error");
    }
  };

  const handleToggleRatio = (r: AspectRatio) => {
    const isSelected = selectedRatios.includes(r);
    if (isSelected) {
      setSelectedRatios((prev) => prev.filter((x) => x !== r));
      setSelectionHistory((h) =>
        h.filter((x) => !(x.type === "aspectRatio" && x.id === r)),
      );
    } else {
      setSelectedRatios((prev) => [...prev, r]);
      setSelectionHistory((h) => [...h, { type: "aspectRatio", id: r }]);
    }
  };

  const handleTogglePreset = (id: string) => {
    const isSelected = selectedPresetIds.includes(id);
    if (isSelected) {
      setSelectedPresetIds((prev) => prev.filter((x) => x !== id));
      setSelectionHistory((h) =>
        h.filter((x) => !(x.type === "preset" && x.id === id)),
      );
    } else {
      if (selectedPresetIds.length >= 5) {
        addLog("Max 5 presets per batch", "warn");
        return;
      }
      setSelectedPresetIds((prev) => [...prev, id]);
      setSelectionHistory((h) => [...h, { type: "preset", id }]);
    }
  };

  const handleStartBatch = async () => {
    if (!outputDir) {
      addLog("Please select an output directory", "warn");
      return;
    }
    const files =
      batchFiles.length > 0 ? batchFiles : inputFile ? [inputFile] : [];
    if (files.length === 0) {
      addLog("Please select at least one file", "warn");
      return;
    }
    if (selectedRatios.length === 0 && selectedPresetIds.length === 0) {
      addLog("Select at least one preset or ratio", "warn");
      return;
    }

    const normalizedEffects = deepClone(normalizeEffects(effectsState));

    // Build targets from platform presets
    const platformTargets: OutputJob[] = selectedPresetIds.flatMap(
      (id): OutputJob[] => {
        const p = platformPresets.find((x) => x.id === id);
        const cp = customPresets.find((x) => x.id === id);

        if (p) {
          return [
            {
              id: generateId(),
              ratio: p.ratio,
              encoding: deepClone(p.encoding),
              effects: normalizedEffects,
              platformConfig: p.platformConfig ?? null,
              selection: {
                sourceType: "platform" as TargetType,
                sourceId: p.id,
                label: p.name,
              } as SelectionMetadata,
            },
          ];
        }
        if (cp) {
          return [
            {
              id: generateId(),
              ratio: cp.ratio,
              encoding: deepClone(cp.encoding),
              effects: normalizedEffects,
              platformConfig: null,
              selection: {
                sourceType: "custom" as TargetType,
                sourceId: cp.id,
                label: cp.name,
              } as SelectionMetadata,
            },
          ];
        }
        return [];
      },
    );

    // Build targets from aspect ratio selections
    const ratioTargets: OutputJob[] = selectedRatios.map((ratio) => {
      const target = aspectRatioTargets.find((t) => t.ratio === ratio);
      const label = RATIO_DISPLAY[ratio] ?? ratio;
      return {
        id: generateId(),
        ratio,
        encoding: deepClone(target?.encoding ?? DEFAULT_ENCODING),
        effects: normalizedEffects,
        platformConfig: null,
        selection: {
          sourceType: "aspectRatio" as TargetType,
          sourceId: ratio,
          label: label,
        } as SelectionMetadata,
      } satisfies OutputJob;
    });

    const targets = [...platformTargets, ...ratioTargets];

    try {
      startRequestedRef.current = true;
      setActiveSessionId(null);
      activeSessionIdRef.current = null;
      setBatchProgress(null);
      setVideoProgresses({});
      await invoke("start_batch", {
        files,
        settings: { targets, outputDir, enableSubfolders },
      });
      addLog(
        `Batch started: ${files.length} file(s) × ${targets.length / (files.length || 1)} target(s)`,
        "accent",
      );
    } catch (e) {
      startRequestedRef.current = false;
      addLog(`Batch start failed: ${errorMessage(e)}`, "error");
    }
  };

  const handleCancelBatch = async () => {
    try {
      startRequestedRef.current = false;
      await invoke("cancel_batch");
      addLog("Cancellation requested...", "warn");
    } catch (e) {
      addLog(`Cancel failed: ${errorMessage(e)}`, "error");
    }
  };

  const handleClearBatch = async () => {
    try {
      startRequestedRef.current = false;
      await invoke("clear_batch");
      setBatchProgress(null);
      setVideoProgresses({});
      setActiveSessionId(null);
      activeSessionIdRef.current = null;
      addLog("Queue cleared", "info");
    } catch (e) {
      addLog(`Clear failed: ${errorMessage(e)}`, "error");
    }
  };

  const handleSavePreset = async () => {
    if (!newPresetName.trim()) return;

    // Determine ratio from active selection or first selected ratio
    const ratio: AspectRatio =
      activeSelection?.type === "aspectRatio"
        ? activeSelection.id
        : activeSelection?.type === "preset"
          ? (platformPresets.find((p) => p.id === activeSelection.id)?.ratio ??
            selectedRatios[0] ??
            "ratio9x16")
          : (selectedRatios[0] ?? "ratio9x16");

    const p: CustomPreset = {
      id: Date.now().toString(),
      name: newPresetName.trim(),
      ratio,
      encoding: deepClone(encodingState),
    };
    try {
      await invoke("save_preset", { preset: p });
      setNewPresetName("");
      await loadPresets();
      addLog(`Preset saved: ${p.name}`, "success");
    } catch (e) {
      addLog(`Save preset failed: ${errorMessage(e)}`, "error");
    }
  };

  const handleDeletePreset = async (id: string) => {
    try {
      await invoke("delete_preset", { id });
      await loadPresets();
      setSelectedPresetIds((prev) => prev.filter((x) => x !== id));
      setSelectionHistory((h) =>
        h.filter((x) => !(x.type === "preset" && x.id === id)),
      );
      addLog("Preset deleted", "info");
    } catch (e) {
      addLog(`Delete failed: ${errorMessage(e)}`, "error");
    }
  };

  // ── Derived state ──────────────────────────────────────────

  const isRunning = batchProgress?.status === "processing";
  const isFileLocked = !!fileReadiness?.isLocked;

  const jobCount =
    (batchFiles.length || (inputFile ? 1 : 0)) *
    (selectedPresetIds.length + selectedRatios.length);

  // Queue sorted: processing first, queued next, then rest
  const queueItems = useMemo(() => {
    if (!batchProgress?.queue) return [];
    return [...batchProgress.queue].sort((a, b) => {
      const score = (s: FileProgress["status"]) => {
        const k = resolveJobStatusKey(s);
        if (k === "processing") return 0;
        if (k === "queued" || k === "pending") return 1;
        if (k === "completed") return 2;
        return 3;
      };
      return score(a.status) - score(b.status);
    });
  }, [batchProgress?.queue]);

  // ETA from backend (more accurate than client-side calc)
  const etaSeconds = batchProgress?.etaSeconds ?? null;
  const stageMessage =
    batchProgress?.currentStageMessage ??
    (isRunning ? "Preparing pipeline..." : "Idle");
  const importSelectionLabel =
    batchFiles.length > 1
      ? `${batchFiles.length} files selected`
      : batchFiles.length === 1 && !inputFile
        ? `Folder Selected: ${basename(batchFiles[0])}`
        : inputFile
          ? basename(inputFile)
          : "Drop videos here";
  const importSelectionTooltip =
    batchFiles.length > 1
      ? undefined
      : batchFiles.length === 1 && !inputFile
        ? batchFiles[0]
        : inputFile || undefined;
  const outputDirLabel = outputDir || "No output selected";
  const audioBitrateOptions = useMemo(() => {
    const values = new Set<string>(AUDIO_BITRATE_CANDIDATES);
    values.add(encodingState.audioBitrate);
    for (const p of platformPresets) values.add(p.encoding.audioBitrate);
    for (const p of customPresets) values.add(p.encoding.audioBitrate);
    return [...values].sort((a, b) => {
      const ak = parseBitrateKbps(a) ?? Number.MAX_SAFE_INTEGER;
      const bk = parseBitrateKbps(b) ?? Number.MAX_SAFE_INTEGER;
      if (ak === bk) return a.localeCompare(b);
      return ak - bk;
    });
  }, [encodingState.audioBitrate, platformPresets, customPresets]);

  const previewCandidates = useMemo(() => {
    if (folderPreviewFiles.length > 0) return folderPreviewFiles;
    if (batchFiles.length > 1) return batchFiles;
    if (previewFile) return [previewFile];
    return [];
  }, [folderPreviewFiles, batchFiles, previewFile]);

  const activePreviewIndex = useMemo(() => {
    if (!previewFile || previewCandidates.length === 0) return -1;
    return previewCandidates.indexOf(previewFile);
  }, [previewCandidates, previewFile]);

  const canNavigatePreview = previewCandidates.length > 1;

  useEffect(() => {
    if (previewCandidates.length === 0) return;
    if (!previewFile || !previewCandidates.includes(previewFile)) {
      setPreviewFile(previewCandidates[0]);
    }
  }, [previewCandidates, previewFile]);

  const navigatePreview = useCallback(
    (direction: -1 | 1) => {
      if (!canNavigatePreview) return;
      const currentIndex = activePreviewIndex >= 0 ? activePreviewIndex : 0;
      const nextIndex =
        (currentIndex + direction + previewCandidates.length) %
        previewCandidates.length;
      setPreviewFile(previewCandidates[nextIndex]);
    },
    [canNavigatePreview, activePreviewIndex, previewCandidates],
  );

  return (
    <div className="app-shell">
      <header className="topbar">
        <div className="topbar-logo">
          <img src="logo.png" alt="AspectShift" className="topbar-logo-img" />
          <div>
            <div className="topbar-title">AspectShift</div>
            <div className="topbar-subtitle">HTOV Converter</div>
          </div>
        </div>
        <div className="topbar-right">
          {batchProgress &&
            batchProgress.percentage >= 100 &&
            batchProgress.status === "completed" && (
              <span className="badge badge-success">✓ Complete</span>
            )}
          {batchProgress && batchProgress.status === "failed" && (
            <span className="badge badge-error">⚠ Errors</span>
          )}
          <button
            className="theme-toggle"
            onClick={() => setTheme((t) => (t === "day" ? "night" : "day"))}
          >
            {theme === "night" ? "☀" : "☾"}
          </button>
        </div>
      </header>

      <div className="main-content">
        {/* ── Left Sidebar ───────────────────────────────────── */}
        <aside className="sidebar">
          <div className="sidebar-section">
            <div className="sidebar-section-title">Import Files</div>
            <div
              className={`drop-zone${isDragOver ? " drag-over" : ""}`}
              onDragOver={handleDragOver}
              onDragLeave={handleDragLeave}
              onDrop={handleDrop}
              onClick={handlePickFile}
            >
              <div className="drop-zone-icon">
                <svg
                  xmlns="http://www.w3.org/2000/svg"
                  width="24"
                  height="24"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="2"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  className="lucide lucide-import-icon lucide-import"
                  style={{ color: "var(--accent)" }}
                >
                  <path d="M12 3v12" />
                  <path d="m8 11 4 4 4-4" />
                  <path d="M8 5H4a2 2 0 0 0-2 2v10a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V7a2 2 0 0 0-2-2h-4" />
                </svg>
              </div>
              <div className="drop-zone-text">
                <strong title={importSelectionTooltip}>
                  {importSelectionLabel}
                </strong>
              </div>
              <div
                className="drop-zone-actions"
                onClick={(e) => e.stopPropagation()}
              >
                <button className="btn btn-sm" onClick={handlePickFile}>
                  Files
                </button>
                <button className="btn btn-sm" onClick={handlePickFolder}>
                  Folder
                </button>
              </div>
            </div>
          </div>

          <div className="sidebar-section">
            <div className="sidebar-section-title">Output Directory</div>
            <div className="path-row">
              <div
                className={`path-display${outputDir ? "" : " empty"}`}
                title={outputDir || undefined}
              >
                {outputDirLabel}
              </div>
            </div>
            <div className="flex gap-6 mt-2">
              <button
                className="btn btn-sm flex-1"
                onClick={handlePickOutputDir}
              >
                Browse
              </button>
              {outputDir && (
                <button className="btn btn-sm" onClick={handleOpenOutput}>
                  ↗
                </button>
              )}
            </div>
            <div className="toggle-row mt-4">
              <span className="toggle-label">Subfolders per target</span>
              <Toggle
                checked={enableSubfolders}
                onChange={setEnableSubfolders}
              />
            </div>
          </div>

          <div className="tabs">
            <button
              className={`tab${settingsTab === "effects" ? " active" : ""}`}
              onClick={() => setSettingsTab("effects")}
            >
              Effects
            </button>
            <button
              className={`tab${settingsTab === "encode" ? " active" : ""}`}
              onClick={() => setSettingsTab("encode")}
            >
              Encode
            </button>
            <button
              className={`tab${settingsTab === "presets" ? " active" : ""}`}
              onClick={() => setSettingsTab("presets")}
            >
              Presets
            </button>
          </div>

          <div className="settings-scroll">
            {/* ── Effects Tab ─────────────────────────────── */}
            {settingsTab === "effects" && (
              <>
                <div className="settings-group">
                  <div className="settings-group-title">Effects</div>
                  <div className="toggle-row">
                    <span className="toggle-label">Blur Background</span>
                    <Toggle
                      checked={!!effectsState.blur}
                      onChange={(v) =>
                        setEffectsState({ ...effectsState, blur: v })
                      }
                    />
                  </div>
                  {effectsState.blur && (
                    <div className="slider-row mt-2">
                      <span className="text-sm text-muted">Sigma</span>
                      <input
                        className="slider"
                        type="range"
                        min="5"
                        max="60"
                        value={effectsState.blurSigma ?? 20}
                        onChange={(e) =>
                          setEffectsState({
                            ...effectsState,
                            blurSigma: parseFloat(e.target.value),
                          })
                        }
                      />
                      <span className="slider-value">
                        {effectsState.blurSigma}
                      </span>
                    </div>
                  )}
                </div>

                <div className="settings-group">
                  <div className="settings-group-title">Audio</div>
                  <div className="toggle-row">
                    <span className="toggle-label">Remove Audio</span>
                    <Toggle
                      checked={!!effectsState.removeAudio}
                      onChange={(v) =>
                        setEffectsState({ ...effectsState, removeAudio: v })
                      }
                    />
                  </div>
                </div>

                <div className="settings-group">
                  <div className="settings-group-title">Subtitles</div>
                  <div className="toggle-row">
                    <span className="toggle-label">Export Subtitles</span>
                    <Toggle
                      checked={!!effectsState.exportSubtitles}
                      onChange={(v) =>
                        setEffectsState({
                          ...effectsState,
                          exportSubtitles: v,
                        })
                      }
                    />
                  </div>
                  <div className="toggle-row">
                    <span className="toggle-label">Burn Subtitles</span>
                    <Toggle
                      checked={!!effectsState.burnSubtitles}
                      onChange={(v) =>
                        setEffectsState({
                          ...effectsState,
                          burnSubtitles: v,
                        })
                      }
                    />
                  </div>
                </div>

                <div className="settings-group">
                  <div className="settings-group-title">Skip Existing</div>
                  <div className="toggle-row">
                    <span className="toggle-label">
                      Skip if output exists
                      <span className="label-desc">Saves time on re-runs</span>
                    </span>
                    <Toggle
                      checked={!!effectsState.skipExisting}
                      onChange={(v) =>
                        setEffectsState({ ...effectsState, skipExisting: v })
                      }
                    />
                  </div>
                </div>

                <div className="settings-group">
                  <div className="settings-group-title">Transform</div>
                  <div className="transform-grid">
                    <button
                      className="btn btn-sm"
                      onClick={() =>
                        setEffectsState({
                          ...effectsState,
                          transform: {
                            rotate:
                              ((effectsState.transform?.rotate ?? 0) + 90) %
                              360,
                            flip_h: !!effectsState.transform?.flip_h,
                            flip_v: !!effectsState.transform?.flip_v,
                          },
                        })
                      }
                    >
                      ↻ Rotate {effectsState.transform?.rotate || 0}°
                    </button>
                    <button
                      className="btn btn-sm"
                      onClick={() =>
                        setEffectsState({
                          ...effectsState,
                          transform: {
                            rotate: 0,
                            flip_h: false,
                            flip_v: false,
                          },
                        })
                      }
                    >
                      ⊕ Reset
                    </button>
                    <div className="checkbox-row">
                      <input
                        type="checkbox"
                        id="fh"
                        checked={uiFlips.flipH}
                        onChange={(e) =>
                          setEffectsState({
                            ...effectsState,
                            transform: uiFlipsToTransform(
                              effectsState.transform,
                              e.target.checked,
                              uiFlips.flipV,
                            ),
                          })
                        }
                      />
                      <label className="checkbox-label" htmlFor="fh">
                        Flip H
                      </label>
                    </div>
                    <div className="checkbox-row">
                      <input
                        type="checkbox"
                        id="fv"
                        checked={uiFlips.flipV}
                        onChange={(e) =>
                          setEffectsState({
                            ...effectsState,
                            transform: uiFlipsToTransform(
                              effectsState.transform,
                              uiFlips.flipH,
                              e.target.checked,
                            ),
                          })
                        }
                      />
                      <label className="checkbox-label" htmlFor="fv">
                        Flip V
                      </label>
                    </div>
                  </div>
                </div>

                <div className="settings-group">
                  <div className="settings-group-title">Logo</div>
                  <div className="toggle-row mb-2">
                    <span className="toggle-label">Enable Logo</span>
                    <Toggle
                      checked={!!effectsState.logo?.enabled}
                      onChange={(v) =>
                        setEffectsState({
                          ...effectsState,
                          logo: v
                            ? {
                                enabled: true,
                                position:
                                  effectsState.logo?.position ?? "bottom_right",
                                opacity: effectsState.logo?.opacity ?? 1,
                                gap: effectsState.logo?.gap ?? 20,
                                scale: effectsState.logo?.scale ?? 0.15,
                                path: effectsState.logo?.path ?? null,
                              }
                            : null,
                        })
                      }
                    />
                  </div>
                  {effectsState.logo?.enabled && (
                    <>
                      <div
                        className="logo-upload-zone"
                        onClick={handlePickLogo}
                      >
                        {effectsState.logo.path ? (
                          <img
                            className="logo-preview-thumb"
                            src={convertFileSrc(effectsState.logo.path)}
                            alt="logo"
                          />
                        ) : (
                          <span style={{ fontSize: 25 }}>🖼</span>
                        )}{" "}
                        <div className="logo-upload-text">
                          <strong>
                            {effectsState.logo.path
                              ? basename(effectsState.logo.path)
                              : "No logo"}
                          </strong>{" "}
                          Click to change
                        </div>
                      </div>
                      <select
                        className="input select mt-2"
                        value={effectsState.logo.position}
                        onChange={(e) =>
                          setEffectsState({
                            ...effectsState,
                            logo: {
                              ...effectsState.logo!,
                              position: e.target.value as LogoPosition,
                            },
                          })
                        }
                      >
                        <option value="top_left">Top Left</option>
                        <option value="top_right">Top Right</option>
                        <option value="bottom_left">Bottom Left</option>
                        <option value="bottom_right">Bottom Right</option>
                      </select>
                      <div className="slider-row mt-2">
                        <span className="text-xs">Opacity</span>
                        <input
                          className="slider"
                          type="range"
                          min="0"
                          max="1"
                          step="0.05"
                          value={effectsState.logo.opacity}
                          onChange={(e) =>
                            setEffectsState({
                              ...effectsState,
                              logo: {
                                ...effectsState.logo!,
                                opacity: parseFloat(e.target.value),
                              },
                            })
                          }
                        />
                        <span className="slider-value">
                          {Math.round(effectsState.logo.opacity * 100)}%
                        </span>
                      </div>
                      <div className="slider-row mt-2">
                        <span className="text-xs">Scale</span>
                        <input
                          className="slider"
                          type="range"
                          min="0.05"
                          max="0.5"
                          step="0.01"
                          value={effectsState.logo.scale}
                          onChange={(e) =>
                            setEffectsState({
                              ...effectsState,
                              logo: {
                                ...effectsState.logo!,
                                scale: parseFloat(e.target.value),
                              },
                            })
                          }
                        />
                        <span className="slider-value">
                          {Math.round(effectsState.logo.scale * 100)}%
                        </span>
                      </div>
                    </>
                  )}
                </div>

                <div className="settings-group">
                  <button
                    className="btn btn-ghost btn-sm btn-full mt-4"
                    onClick={handleResetToDefaults}
                    style={{ opacity: 0.6, fontSize: 11 }}
                  >
                    Reset all settings to defaults
                  </button>
                </div>
              </>
            )}

            {/* ── Encode Tab ──────────────────────────────── */}
            {settingsTab === "encode" && (
              <div className="settings-group">
                <div className="settings-group-title">Quality</div>
                <label className="input-label">Quality Preset</label>
                <select
                  className="input select"
                  value={encodingState.qualityPreset}
                  onChange={(e) =>
                    setEncodingState({
                      ...encodingState,
                      qualityPreset: e.target.value,
                    })
                  }
                >
                  <option value="draft">Draft</option>
                  <option value="standard">Standard</option>
                  <option value="high">High</option>
                </select>

                <div className="slider-row mt-4">
                  <span className="text-xs">CRF</span>
                  <input
                    className="slider"
                    type="range"
                    min="0"
                    max="51"
                    value={encodingState.crf}
                    onChange={(e) =>
                      setEncodingState({
                        ...encodingState,
                        crf: parseInt(e.target.value),
                      })
                    }
                  />
                  <span className="slider-value">{encodingState.crf}</span>
                </div>

                <label className="input-label mt-4">Speed Preset</label>
                <select
                  className="input select"
                  value={encodingState.speedPreset}
                  onChange={(e) =>
                    setEncodingState({
                      ...encodingState,
                      speedPreset: e.target.value,
                    })
                  }
                >
                  {SPEED_PRESETS.map((s) => (
                    <option key={s} value={s}>
                      {s.charAt(0).toUpperCase() + s.slice(1)}
                    </option>
                  ))}
                </select>

                <label className="input-label mt-4">Audio Bitrate</label>
                <select
                  className="input select"
                  value={encodingState.audioBitrate}
                  onChange={(e) =>
                    setEncodingState({
                      ...encodingState,
                      audioBitrate: e.target.value,
                    })
                  }
                >
                  {audioBitrateOptions.map((bitrate) => (
                    <option key={bitrate} value={bitrate}>
                      {audioBitrateLabel(bitrate)}
                    </option>
                  ))}
                </select>

                <div className="settings-group">
                  <div className="settings-group-title">Output Format</div>
                  <select
                    className="input select"
                    value={effectsState.outputFormat ?? "mp4"}
                    onChange={(e) =>
                      setEffectsState({
                        ...effectsState,
                        outputFormat: e.target.value as OutputFormat,
                      })
                    }
                  >
                    <option value="mp4">MP4 (H.264)</option>
                    <option value="mov">MOV (H.264)</option>
                    <option value="webm">WebM (VP9)</option>
                  </select>
                </div>
              </div>
            )}

            {/* ── Presets Tab ─────────────────────────────── */}
            {settingsTab === "presets" && (
              <div className="settings-group">
                <div className="settings-group-title">
                  Save Current Encoding as Preset
                </div>
                <input
                  className="input"
                  placeholder="Preset name"
                  value={newPresetName}
                  onChange={(e) => setNewPresetName(e.target.value)}
                />
                <button
                  className="btn btn-primary btn-sm btn-full mt-2"
                  onClick={handleSavePreset}
                  disabled={!newPresetName.trim()}
                >
                  Save Preset
                </button>

                {customPresets.length > 0 && (
                  <div className="mt-8">
                    <div className="settings-group-title">My Presets</div>
                    {customPresets.map((p) => (
                      <div
                        key={p.id}
                        className="flex items-center justify-between"
                        style={{
                          padding: "6px 0",
                          borderBottom: "1px solid var(--border)",
                        }}
                      >
                        <div>
                          <div style={{ fontSize: 12, fontWeight: 600 }}>
                            {p.name}
                          </div>
                          <div className="text-xs text-muted">
                            {RATIO_DISPLAY[p.ratio]} · CRF {p.encoding.crf}
                          </div>
                        </div>
                        <button
                          className="btn btn-danger btn-sm"
                          onClick={() => handleDeletePreset(p.id)}
                        >
                          ✕
                        </button>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            )}
          </div>
        </aside>

        {/* ── Center Panel ──────────────────────────────────── */}
        <main className="center-panel">
          <div
            className="sidebar-section"
            style={{ background: "var(--bg-card)" }}
          >
            <div className="flex items-center justify-between mb-4">
              <span className="section-title">Aspect Ratio Targets</span>
            </div>
            <div className="ratio-pills">
              {ASPECT_RATIOS.map((r) => (
                <label
                  key={r.value}
                  className={`ratio-pill${selectedRatios.includes(r.value) ? " active" : ""}`}
                >
                  <input
                    type="checkbox"
                    onChange={() => handleToggleRatio(r.value)}
                  />
                  {r.label}
                </label>
              ))}
            </div>
          </div>

          <div className="preview-wrapper">
            <button
              className="preview-nav-btn preview-nav-btn-left"
              onClick={() => navigatePreview(-1)}
              disabled={!canNavigatePreview}
              aria-label="Previous preview video"
              title="Previous video"
            >
              <svg
                xmlns="http://www.w3.org/2000/svg"
                width="16"
                height="16"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
                strokeLinecap="round"
                strokeLinejoin="round"
                className="lucide lucide-chevron-left"
              >
                <path d="m15 18-6-6 6-6" />
              </svg>
            </button>
            <button
              className="preview-nav-btn preview-nav-btn-right"
              onClick={() => navigatePreview(1)}
              disabled={!canNavigatePreview}
              aria-label="Next preview video"
              title="Next video"
            >
              <svg
                xmlns="http://www.w3.org/2000/svg"
                width="16"
                height="16"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
                strokeLinecap="round"
                strokeLinejoin="round"
                className="lucide lucide-chevron-right"
              >
                <path d="m9 18 6-6-6-6" />
              </svg>
            </button>
            <div className="preview-controls-overlay">
              <button
                className={`btn btn-xs ${showGuides ? "active" : ""}`}
                onClick={() => setShowGuides(!showGuides)}
              >
                Guides
              </button>
              <button
                className={`btn btn-xs ${showSafeFrames ? "active" : ""}`}
                onClick={() => setShowSafeFrames(!showSafeFrames)}
              >
                Safe Areas
              </button>
            </div>
            <VideoCanvas
              videoSrc={previewFile}
              ratio={effectivePreviewRatio}
              effects={effectsState}
              orientation={orientation}
              previewVolume={previewVolume}
              showGuides={showGuides}
              showSafeFrames={showSafeFrames}
            />
            <div
              className="preview-volume"
              ref={previewVolumeRef}
              aria-label="Preview volume"
              data-slider-active={volumeSliderActive ? "true" : undefined}
              onMouseEnter={() => {
                setVolumeSliderHovering(true);
                setVolumeSliderActive(true);
              }}
              onMouseLeave={() => setVolumeSliderHovering(false)}
              onFocusCapture={() => {
                setVolumeSliderFocusWithin(true);
                setVolumeSliderActive(true);
              }}
              onBlurCapture={(e) => {
                const nextTarget = e.relatedTarget as Node | null;
                if (
                  nextTarget &&
                  previewVolumeRef.current?.contains(nextTarget)
                ) {
                  return;
                }
                setVolumeSliderFocusWithin(false);
              }}
            >
              <div className="preview-volume-slider-wrap">
                <input
                  className="preview-volume-slider"
                  type="range"
                  min={0}
                  max={100}
                  step={1}
                  value={previewVolume}
                  onChange={(e) => handleVolumeChange(Number(e.target.value))}
                  onPointerDown={() => {
                    cancelVolumeCollapse();
                    setVolumeSliderInteracting(true);
                    setVolumeSliderActive(true);
                  }}
                  onPointerUp={() => {
                    setVolumeSliderInteracting(false);
                  }}
                  onKeyDown={() => {
                    cancelVolumeCollapse();
                    setVolumeSliderActive(true);
                  }}
                  onKeyUp={() => {
                    setVolumeSliderInteracting(false);
                  }}
                  aria-label="Preview volume slider"
                  style={
                    { "--vol": `${previewVolume}%` } as React.CSSProperties
                  }
                />
              </div>
              <button
                className="preview-volume-btn"
                onClick={() => {
                  cancelVolumeCollapse();
                  setVolumeSliderActive(true);
                  setPreviewVolume((v) => (v > 0 ? 0 : DEFAULT_PREVIEW_VOLUME));
                }}
                aria-label={
                  previewVolume === 0 ? "Unmute preview" : "Mute preview"
                }
                title={`Preview volume: ${previewVolume}%`}
              >
                {previewVolume === 0 ? (
                  <svg
                    xmlns="http://www.w3.org/2000/svg"
                    width="16"
                    height="16"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    strokeWidth="2"
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    className="lucide lucide-volume-x"
                  >
                    <polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5" />
                    <line x1="22" x2="16" y1="9" y2="15" />
                    <line x1="16" x2="22" y1="9" y2="15" />
                  </svg>
                ) : (
                  <svg
                    xmlns="http://www.w3.org/2000/svg"
                    width="16"
                    height="16"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    strokeWidth="2"
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    className="lucide lucide-volume-2"
                  >
                    <polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5" />
                    <path d="M15.54 8.46a5 5 0 0 1 0 7.07" />
                    <path d="M19.07 4.93a10 10 0 0 1 0 14.14" />
                  </svg>
                )}
              </button>
            </div>
            {orientation && (
              <div className="preview-meta">
                <span className="preview-meta-item">
                  ⬛ {orientation.displayWidth}×{orientation.displayHeight}
                </span>
                <span className="preview-meta-item">
                  ◱ {orientation.isVertical ? "Vertical" : "Horizontal"}
                </span>
                <span className="preview-meta-item">
                  ⏱{" "}
                  {fileReadiness
                    ? formatDuration(fileReadiness.estimatedDurationSecs)
                    : "--:--"}
                </span>
              </div>
            )}
          </div>

          <div
            className="sidebar-section"
            style={{
              background: "var(--bg-card)",
              borderTop: "1px solid var(--border)",
              borderBottom: "none",
            }}
          >
            <PresetsPanel
              presets={displayPresets}
              selectedPresetIds={selectedPresetIds}
              onToggle={handleTogglePreset}
            />
          </div>

          <div className="controls-bar">
            <button
              className="btn btn-primary btn-lg flex-1"
              onClick={handleStartBatch}
              disabled={
                isRunning ||
                isFileLocked ||
                (selectedRatios.length === 0 && selectedPresetIds.length === 0)
              }
            >
              {isRunning ? (
                <>
                  <span className="spinner" /> Processing…
                </>
              ) : (
                <>
                  ▶ {jobCount > 1 ? `Start Batch (${jobCount})` : "Convert Now"}
                </>
              )}
            </button>
            {batchProgress && (
              <>
                <button
                  className="btn btn-danger"
                  onClick={handleCancelBatch}
                  disabled={!isRunning}
                >
                  ✕ Cancel
                </button>
                <button
                  className="btn btn-ghost"
                  onClick={handleClearBatch}
                  disabled={isRunning}
                >
                  Clear
                </button>
              </>
            )}
          </div>
        </main>

        {/* ── Right Panel ───────────────────────────────────── */}
        <aside className="right-panel">
          {batchProgress && (
            <div className="batch-stats">
              <div className="batch-stats-row">
                <span className="batch-stats-label">
                  {isRunning ? (
                    <>
                      <span className="spinner" /> Processing
                    </>
                  ) : (
                    "Batch Status"
                  )}
                </span>
                <span
                  className="text-accent font-mono font-bold"
                  style={{ fontSize: 13 }}
                >
                  {batchProgress.percentage.toFixed(1)}%
                </span>
              </div>
              <div className="progress-bar">
                <div
                  className={`progress-bar-fill${isRunning ? " animated" : ""}`}
                  style={{
                    width: `${Math.min(100, batchProgress.percentage)}%`,
                  }}
                />
              </div>
              <div className="batch-stats-nums">
                <span className="stat-pill stat-completed">
                  ✓ {batchProgress.completedJobs}
                </span>
                {batchProgress.failedJobs > 0 && (
                  <span className="stat-pill stat-failed">
                    ✕ {batchProgress.failedJobs}
                  </span>
                )}
                <span className="stat-pill stat-pending">
                  {batchProgress.totalJobs} TOTAL
                </span>
                {isRunning && etaSeconds !== null && (
                  <span
                    className="stat-pill"
                    style={{
                      background: "var(--accent-subtle)",
                      color: "var(--accent)",
                      border: "1px solid var(--accent-glow)",
                    }}
                  >
                    ⏱ {formatETA(etaSeconds)}
                  </span>
                )}
              </div>
              {batchProgress && (
                <div
                  className="text-xs text-muted font-mono"
                  style={{ marginTop: 2.5, fontSize: 10 }}
                >
                  {stageMessage}
                </div>
              )}
            </div>
          )}

          <div
            className="tabs"
            style={{
              marginTop: batchProgress ? "var(--space-md)" : "var(--space-lg)",
            }}
          >
            <button
              className={`tab${rightTab === "queue" ? " active" : ""}`}
              onClick={() => setRightTab("queue")}
            >
              Queue
              {batchProgress && batchProgress.totalJobs > 0 && (
                <span className="badge badge-muted" style={{ marginLeft: 6 }}>
                  {batchProgress.totalJobs}
                </span>
              )}
            </button>
            <button
              className={`tab${rightTab === "log" ? " active" : ""}`}
              onClick={() => setRightTab("log")}
            >
              Log
            </button>
          </div>

          {rightTab === "queue" ? (
            <div className="queue-list">
              {queueItems.length === 0 && (
                <div className="queue-empty">
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
                    className="lucide lucide-folder-open-icon lucide-folder-open"
                    style={{
                      color: "var(--accent)",
                      opacity: 0.5,
                      marginBottom: "10px",
                    }}
                  >
                    <path d="m6 14 1.5-2.9A2 2 0 0 1 9.24 10H20a2 2 0 0 1 1.94 2.5l-1.54 6a2 2 0 0 1-1.95 1.5H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h3.9a2 2 0 0 1 1.69.9l.81 1.2a2 2 0 0 0 1.67.9H18a2 2 0 0 1 2 2v2" />
                  </svg>
                  <div>Queue is empty</div>
                </div>
              )}
              {queueItems.map((job) => {
                const statusKey = resolveJobStatusKey(job.status);
                const errorMsg = getJobStatusError(job.status);
                const isProc = statusKey === "processing";
                const isDone = statusKey === "completed";
                const isFail = statusKey === "failed";
                const isCancelled = statusKey === "cancelled";

                const progress = isProc
                  ? Math.max(videoProgresses[job.jobId] ?? 0, job.progress)
                  : job.progress;

                return (
                  <div
                    key={job.jobId}
                    className={`queue-item${isProc ? " is-processing" : ""}`}
                  >
                    {job.thumbnailPath && (
                      <div className="queue-item-thumb">
                        <img
                          src={convertFileSrc(job.thumbnailPath)}
                          alt="thumb"
                          loading="lazy"
                        />
                      </div>
                    )}
                    <div className="queue-item-body">
                      <div className="queue-item-name">
                        {basename(job.filePath)}
                      </div>
                      <div className="queue-item-meta">
                        <span className="queue-item-ratio">
                          {job.selection.label}
                        </span>
                        <span
                          className={`queue-item-status-text${isFail ? " text-error" : ""}`}
                          title={errorMsg ?? undefined}
                        >
                          {isProc
                            ? `${progress.toFixed(0)}%`
                            : isDone
                              ? "Done"
                              : isFail
                                ? "Failed"
                                : isCancelled
                                  ? "Cancelled"
                                  : "Queued"}
                        </span>
                      </div>
                      {isProc && (
                        <div className="queue-item-progress">
                          <div
                            className="queue-item-progress-fill"
                            style={{ width: `${progress}%` }}
                          />
                        </div>
                      )}
                      {isFail && errorMsg && (
                        <div
                          className="text-xs text-error truncate"
                          title={errorMsg}
                          style={{ marginTop: 2 }}
                        >
                          {errorMsg}
                        </div>
                      )}
                    </div>
                  </div>
                );
              })}
            </div>
          ) : (
            <div className="log-panel">
              <div className="log-body" ref={logRef}>
                {logs.length === 0 && (
                  <div className="queue-empty">
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
                      className="lucide lucide-logs-icon lucide-logs"
                      style={{
                        color: "var(--accent)",
                        opacity: 0.5,
                        marginBottom: "10px",
                      }}
                    >
                      <path d="M3 5h1" />
                      <path d="M3 12h1" />
                      <path d="M3 19h1" />
                      <path d="M8 5h1" />
                      <path d="M8 12h1" />
                      <path d="M8 19h1" />
                      <path d="M13 5h8" />
                      <path d="M13 12h8" />
                      <path d="M13 19h8" />
                    </svg>
                    <div style={{ fontSize: 13 }}>No log entries yet</div>
                  </div>
                )}
                {logs.map((entry, i) => (
                  <div key={i} className="log-entry">
                    <span className="log-time">{entry.time}</span>
                    <span className={`log-msg log-${entry.type}`}>
                      {entry.msg}
                    </span>
                  </div>
                ))}
              </div>
              {logs.length > 0 && (
                <div
                  style={{
                    padding: "6px 8px",
                    borderTop: "1px solid var(--border)",
                    flexShrink: 0,
                  }}
                >
                  <button
                    className="btn btn-ghost btn-sm"
                    onClick={() => setLogs([])}
                  >
                    Clear Log
                  </button>
                </div>
              )}
            </div>
          )}
        </aside>
      </div>
    </div>
  );
}
