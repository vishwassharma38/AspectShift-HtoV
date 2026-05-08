import { useState, useEffect, useMemo, useRef, useCallback } from "react";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import type {
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
  VideoEffectsSettings,
  VideoProgress,
  VideoTransform,
} from "./types/backend";
import "./App.css";
import { VideoCanvas } from "./components/VideoCanvas";
import { PresetsPanel, type DisplayPreset } from "./components/PresetsPanel";

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
  | { type: "platform"; id: string };

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
  generateSubtitles: false,
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

export const PLATFORM_ICONS: Record<string, string> = {
  youtube: "▶",
  shorts: "▲",
  instagram: "◈",
  reels: "◈",
  tiktok: "♪",
  twitter: "✕",
  reddit: "◉",
  x: "✕",
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
  const [batchFiles, setBatchFiles] = useState<string[]>([]);
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

  // Derived: last active selection drives preview ratio
  const activeSelection = selectionHistory[selectionHistory.length - 1] ?? null;

  const effectivePreviewRatio = useMemo(() => {
    if (!activeSelection) {
      if (orientation)
        return orientation.displayWidth / orientation.displayHeight;
      return 9 / 16;
    }
    if (activeSelection.type === "aspectRatio") {
      return RATIO_VALUE[activeSelection.id];
    }
    const p = platformPresets.find((x) => x.id === activeSelection.id);
    if (p) return RATIO_VALUE[p.ratio];
    const cp = customPresets.find((x) => x.id === activeSelection.id);
    if (cp) return RATIO_VALUE[cp.ratio];
    return 9 / 16;
  }, [activeSelection, orientation, platformPresets, customPresets]);

  // Sync encoding when a platform preset becomes the active selection
  useEffect(() => {
    if (!activeSelection || activeSelection.type !== "platform") return;
    const p = platformPresets.find((x) => x.id === activeSelection.id);
    if (p) {
      setEncodingState(deepClone(p.encoding));
    }
  }, [activeSelection, platformPresets]);

  // ── Theme ──────────────────────────────────────────────────
  useEffect(() => {
    document.documentElement.setAttribute("data-theme", theme);
    localStorage.setItem("asp-theme", theme);
  }, [theme]);

  // ── Init ──────────────────────────────────────────────────
  useEffect(() => {
    loadPresets();
    loadAspectRatioTargets();
  }, []);

  // ── Tauri Event Listeners ─────────────────────────────────
  useEffect(() => {
    const unsubscribers: Array<() => void> = [];

    const setupListeners = async () => {
      const u1 = await listen<BatchProgress>("batch://progress", (e) => {
        setBatchProgress(e.payload);

        if (e.payload.status === "completed") {
          addLog("Batch complete ✓", "success");
        } else if (e.payload.status === "cancelled") {
          addLog("Batch cancelled by user", "warn");
        } else if (e.payload.status === "failed") {
          addLog("Batch finished with errors", "error");
        }
      });
      unsubscribers.push(u1);

      const u2 = await listen<FileProgress>("batch://file-status", (e) => {
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
      unsubscribers.push(u2);

      const u3 = await listen<VideoProgress>("video://progress", (e) => {
        setVideoProgresses((prev) => ({
          ...prev,
          [e.payload.jobId]: e.payload.percent,
        }));
      });
      unsubscribers.push(u3);
    };

    setupListeners();
    return () => {
      unsubscribers.forEach((u) => u());
    };
  }, []);

  // ── Orientation detect on file change ─────────────────────
  useEffect(() => {
    if (!inputFile || !isVideoPath(inputFile)) {
      setOrientation(null);
      setFileReadiness(null);
      return;
    }

    invoke<OrientationInfo>("detect_orientation", { filePath: inputFile })
      .then(setOrientation)
      .catch(() => setOrientation(null));

    invoke<FileReadiness>("check_file_ready", { path: inputFile })
      .then(setFileReadiness)
      .catch((e) => {
        setFileReadiness(null);
        addLog(`File readiness check failed: ${errorMessage(e)}`, "warn");
      });
  }, [inputFile]);

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
        filters: [
          { name: "Video", extensions: ["mp4", "mov", "mkv", "avi", "webm"] },
        ],
      });
      if (!sel) return;
      const files = Array.isArray(sel) ? sel : [sel];
      if (files.length === 1) {
        setInputFile(files[0]);
        setBatchFiles([]);
        addLog(`File selected: ${basename(files[0])}`, "info");
      } else {
        setInputFile(files[0]);
        setBatchFiles(files);
        addLog(`${files.length} files selected for batch`, "info");
      }
    } catch (e) {
      addLog(`File picker error: ${errorMessage(e)}`, "error");
    }
  };

  const handlePickFolder = async () => {
    try {
      const sel = await open({ multiple: false, directory: true });
      if (sel && typeof sel === "string") {
        addLog("Input folder selected, scanning...", "info");
        setBatchFiles([sel]);
        setInputFile("");
      }
    } catch (e) {
      addLog(`Folder picker error: ${errorMessage(e)}`, "error");
    }
  };

  const handlePickOutputDir = async () => {
    try {
      const sel = await open({ multiple: false, directory: true });
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
        setBatchFiles([]);
      } else {
        setInputFile(files[0]);
        setBatchFiles(files);
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
        h.filter((x) => !(x.type === "platform" && x.id === id)),
      );
    } else {
      if (selectedPresetIds.length >= 5) {
        addLog("Max 5 presets per batch", "warn");
        return;
      }
      setSelectedPresetIds((prev) => [...prev, id]);
      setSelectionHistory((h) => [...h, { type: "platform", id }]);
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
    const platformTargets: OutputJob[] = selectedPresetIds.flatMap((id) => {
      const p = platformPresets.find((x) => x.id === id);
      const cp = customPresets.find((x) => x.id === id);

      if (p) {
        return [
          {
            id: generateId(),
            sourcePresetId: `platform:${p.id}`,
            ratio: p.ratio,
            encoding: deepClone(encodingState),
            effects: normalizedEffects,
            platformConfig: p.platformConfig ?? null,
            presetName: p.name,
          } satisfies OutputJob,
        ];
      }
      if (cp) {
        return [
          {
            id: generateId(),
            sourcePresetId: `custom:${cp.id}`,
            ratio: cp.ratio,
            encoding: deepClone(cp.encoding),
            effects: normalizedEffects,
            platformConfig: null,
            presetName: cp.name,
          } satisfies OutputJob,
        ];
      }
      return [];
    });

    // Build targets from aspect ratio selections
    const ratioTargets: OutputJob[] = selectedRatios.map((ratio) => {
      const target = aspectRatioTargets.find((t) => t.ratio === ratio);
      return {
        id: generateId(),
        sourcePresetId: `aspectRatio:${ratio}`,
        ratio,
        encoding: deepClone(target?.encoding ?? DEFAULT_ENCODING),
        effects: normalizedEffects,
        platformConfig: null,
        presetName: null,
      } satisfies OutputJob;
    });

    const targets = [...platformTargets, ...ratioTargets];

    try {
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
      addLog(`Batch start failed: ${errorMessage(e)}`, "error");
    }
  };

  const handleCancelBatch = async () => {
    try {
      await invoke("cancel_batch");
      addLog("Cancellation requested...", "warn");
    } catch (e) {
      addLog(`Cancel failed: ${errorMessage(e)}`, "error");
    }
  };

  const handleClearBatch = async () => {
    try {
      await invoke("clear_batch");
      setBatchProgress(null);
      setVideoProgresses({});
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
        : activeSelection?.type === "platform"
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
        h.filter((x) => !(x.type === "platform" && x.id === id)),
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
              <div className="drop-zone-icon">🎬</div>
              <div className="drop-zone-text">
                {batchFiles.length > 1 ? (
                  <strong>{batchFiles.length} files selected</strong>
                ) : inputFile ? (
                  <strong>{basename(inputFile)}</strong>
                ) : (
                  <strong>Drop videos here</strong>
                )}
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
              <div className={`path-display${outputDir ? "" : " empty"}`}>
                {outputDir || "No output selected"}
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
                    <span className="toggle-label">Generate Subtitles</span>
                    <Toggle
                      checked={!!effectsState.generateSubtitles}
                      onChange={(v) =>
                        setEffectsState({
                          ...effectsState,
                          generateSubtitles: v,
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
                          generateSubtitles:
                            v || !!effectsState.generateSubtitles,
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
                        checked={!!effectsState.transform?.flip_h}
                        onChange={(e) =>
                          setEffectsState({
                            ...effectsState,
                            transform: {
                              ...(effectsState.transform ?? { rotate: 0 }),
                              flip_h: e.target.checked,
                            },
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
                        checked={!!effectsState.transform?.flip_v}
                        onChange={(e) =>
                          setEffectsState({
                            ...effectsState,
                            transform: {
                              ...(effectsState.transform ?? { rotate: 0 }),
                              flip_v: e.target.checked,
                            },
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
                          <span>🖼</span>
                        )}
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
                  <option value="64k">64k (Low)</option>
                  <option value="96k">96k</option>
                  <option value="128k">128k (Standard)</option>
                  <option value="192k">192k</option>
                  <option value="256k">256k (High)</option>
                  <option value="320k">320k (Max)</option>
                </select>
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
            style={{
              padding: "14px 20px 12px",
              borderBottom: "1px solid var(--border)",
              background: "var(--bg-card)",
              flexShrink: 0,
            }}
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
              videoSrc={inputFile}
              ratio={effectivePreviewRatio}
              effects={effectsState}
              orientation={orientation}
              showGuides={showGuides}
              showSafeFrames={showSafeFrames}
            />
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
            style={{
              padding: "8px 20px 12px",
              borderTop: "1px solid var(--border)",
              background: "var(--bg-card)",
              flexShrink: 0,
            }}
          >
            <PresetsPanel
              presets={displayPresets}
              selectedPresetIds={selectedPresetIds}
              onToggle={handleTogglePreset}
              icons={PLATFORM_ICONS}
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
                <span className="text-accent font-mono font-bold">
                  {batchProgress.percentage.toFixed(1)}%
                </span>
              </div>
              <div className="progress-bar mb-1">
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
                  / {batchProgress.totalJobs}
                </span>
                {isRunning && etaSeconds !== null && (
                  <span
                    className="stat-pill"
                    style={{
                      background: "var(--accent-subtle)",
                      color: "var(--accent)",
                    }}
                  >
                    ⏱ {formatETA(etaSeconds)}
                  </span>
                )}
              </div>
              {isRunning && batchProgress.speed > 0 && (
                <div className="text-xs text-muted" style={{ marginTop: 4 }}>
                  {batchProgress.speed.toFixed(2)}× realtime ·{" "}
                  {formatDuration(batchProgress.totalDurationSecs)} total
                </div>
              )}
            </div>
          )}

          <div className="tabs" style={{ marginTop: batchProgress ? 0 : 8 }}>
            <button
              className={`tab${rightTab === "queue" ? " active" : ""}`}
              onClick={() => setRightTab("queue")}
            >
              Queue
              {batchProgress && batchProgress.totalJobs > 0 && (
                <span className="badge badge-muted" style={{ marginLeft: 4 }}>
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
                  <div className="queue-empty-icon">📂</div>
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
                          {RATIO_DISPLAY[job.ratio] ?? job.ratio}
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
                  <div className="text-muted text-sm" style={{ padding: 8 }}>
                    No log entries yet.
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
            </div>
          )}
        </aside>
      </div>
    </div>
  );
}
