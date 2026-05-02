import { useState, useEffect, useMemo, useRef, useCallback } from "react";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import type {
  AspectRatio,
  BatchProgress,
  ConversionOptions,
  FileProgress,
  FileReadiness,
  LogoOptions,
  LogoPosition,
  OrientationInfo,
  OutputFormat,
  QualityPreset,
  VideoPreset,
  VideoProgress,
  VideoTransform,
} from "./types/backend";
import "./App.css";
interface JobTargetRequestDTO {
  ratio?: AspectRatio;
  preset_id?: string;
  overrides?: ConversionOptionsRequestDTO;
}
interface ConversionOptionsRequestDTO {
  blur_background?: boolean;
  blur_sigma?: number;
  remove_audio?: boolean;
  generate_subtitles?: boolean;
  burn_subtitles?: boolean;
  skip_existing?: boolean;
  quality?: QualityPreset;
  output_format?: OutputFormat;
  logo?: LogoOptions | null;
  custom_encoding_enabled?: boolean;
  crf?: number | null;
  preset?: string | null;
  audio_bitrate?: string | null;
  transform?: VideoTransform | null;
}
interface PresetComparableState {
  ratio: AspectRatio;
  options: ConversionOptions;
}
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
const DEFAULT_OPTIONS: ConversionOptions = {
  blur_background: false,
  blur_sigma: 20.0,
  remove_audio: false,
  generate_subtitles: false,
  burn_subtitles: false,
  skip_existing: true,
  quality: "standard",
  output_format: "mp4",
  logo: null,
  custom_encoding_enabled: false,
  crf: 18,
  preset: "medium",
  audio_bitrate: "128k",
  transform: { rotate: 0, flip_h: false, flip_v: false },
};

const ASPECT_RATIOS: { label: string; value: AspectRatio }[] = [
  { label: "9:16", value: "ratio9x16" },
  { label: "1:1", value: "ratio1x1" },
  { label: "4:5", value: "ratio4x5" },
  { label: "2:3", value: "ratio2x3" },
  { label: "16:9", value: "ratio16x9" },
];

const RATIO_DISPLAY: Record<AspectRatio, string> = {
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

const PLATFORM_ICONS: Record<string, string> = {
  youtube: "▶",
  youtube_shorts: "▲",
  instagram: "◈",
  instagram_reels: "◈",
  tiktok: "♪",
  twitter_x: "✕",
  reddit: "◉",
};

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

function formatBytes(bytes: number): string {
  if (!isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = bytes;
  let i = 0;
  while (value >= 1024 && i < units.length - 1) {
    value /= 1024;
    i += 1;
  }
  return `${value.toFixed(i === 0 ? 0 : 1)} ${units[i]}`;
}

// Display-only helper to extract filename from a full path.
// The backend remains the source of truth for all path resolution.
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

function isImagePath(path: string): boolean {
  return /\.(png|jpe?g|svg|webp)$/i.test(path);
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

function normalizeOptions(options: ConversionOptions): ConversionOptions {
  return {
    ...options,
    logo: normalizeLogo(options.logo ?? null),
    crf: options.crf ?? null,
    preset: options.preset ?? null,
    audio_bitrate: options.audio_bitrate ?? null,
    transform: normalizeTransform(options.transform),
  };
}

function toConversionOptionsRequestDTO(
  options: ConversionOptions,
): ConversionOptionsRequestDTO {
  return normalizeOptions(options);
}

function buildCurrentState(
  ratio: AspectRatio,
  options: ConversionOptions,
): PresetComparableState {
  return {
    ratio,
    options: normalizeOptions({
      ...options,
      transform: options.transform ?? {
        rotate: 0,
        flip_h: false,
        flip_v: false,
      },
    }),
  };
}

function getOverrideCount(
  current: PresetComparableState,
  defaults: PresetComparableState | null,
): number {
  if (!defaults) return 0;
  const keys: (keyof ConversionOptions)[] = [
    "blur_background",
    "blur_sigma",
    "remove_audio",
    "generate_subtitles",
    "burn_subtitles",
    "skip_existing",
    "quality",
    "output_format",
    "custom_encoding_enabled",
    "crf",
    "preset",
    "audio_bitrate",
    "logo",
    "transform",
  ];

  let diff = current.ratio === defaults.ratio ? 0 : 1;
  for (const key of keys) {
    if (
      JSON.stringify(current.options[key]) !==
      JSON.stringify(defaults.options[key])
    ) {
      diff += 1;
    }
  }
  return diff;
}

function getModifiedOptionKeys(
  current: PresetComparableState,
  base: PresetComparableState | null,
): (keyof ConversionOptions)[] {
  if (!base) return [];
  const keys: (keyof ConversionOptions)[] = [
    "blur_background",
    "blur_sigma",
    "remove_audio",
    "generate_subtitles",
    "burn_subtitles",
    "skip_existing",
    "quality",
    "output_format",
    "custom_encoding_enabled",
    "crf",
    "preset",
    "audio_bitrate",
    "logo",
    "transform",
  ];

  return keys.filter(
    (key) =>
      JSON.stringify(current.options[key]) !==
      JSON.stringify(base.options[key]),
  );
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
    return (localStorage.getItem("asp-theme") as "day" | "night") || "night";
  });

  // Files
  const [inputFile, setInputFile] = useState("");
  const [batchFiles, setBatchFiles] = useState<string[]>([]);
  const [outputDir, setOutputDir] = useState("");
  const [enableSubfolders, setEnableSubfolders] = useState(false);
  const [isDragOver, setIsDragOver] = useState(false);

  // Ratios & Presets
  const [previewRatio, setPreviewRatio] = useState<AspectRatio>("ratio9x16");
  const [selectedRatios, setSelectedRatios] = useState<AspectRatio[]>([]);
  const [options, setOptions] = useState<ConversionOptions>(DEFAULT_OPTIONS);
  const [presets, setPresets] = useState<VideoPreset[]>([]);
  const [selectedPresetId, setSelectedPresetId] = useState<string>("");
  const [selectedPresetIds, setSelectedPresetIds] = useState<string[]>([]);
  // FIX: Store base values per preset id so overrides are computed correctly
  // per-preset rather than against a single globally-last-clicked preset.
  const [presetBaseValuesMap, setPresetBaseValuesMap] = useState<
    Map<string, PresetComparableState>
  >(new Map());
  // Kept for dirty-check display in the UI (against the most recently loaded preset)
  const [presetBaseValues, setPresetBaseValues] =
    useState<PresetComparableState | null>(null);

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
  const [fileProgresses, setFileProgresses] = useState<
    Record<string, FileProgress>
  >({});
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

  // ETA
  const [batchStartTime, setBatchStartTime] = useState<number | null>(null);

  // Refs
  const videoRef = useRef<HTMLVideoElement>(null);

  // ── Theme ──────────────────────────────────────────────────
  useEffect(() => {
    document.documentElement.setAttribute("data-theme", theme);
    localStorage.setItem("asp-theme", theme);
  }, [theme]);

  // ── Init ──────────────────────────────────────────────────
  useEffect(() => {
    loadPresets();
  }, []);

  // ── Tauri Event Listeners ─────────────────────────────────
  useEffect(() => {
    const u1 = listen<BatchProgress>("batch://progress", (e) => {
      setBatchProgress(e.payload);
      if (e.payload.percentage === 0 && e.payload.total_jobs > 0) {
        setBatchStartTime(Date.now());
      }
      if (e.payload.percentage >= 100) {
        addLog("Batch complete ✓", "success");
      }
    });

    const u2 = listen<FileProgress>("batch://file-status", (e) => {
      setFileProgresses((prev) => ({ ...prev, [e.payload.job_id]: e.payload }));
      const name = basename(e.payload.file_path);
      const ratio = RATIO_DISPLAY[e.payload.ratio];
      const s = e.payload.status;
      if (s === "processing")
        addLog(`Processing: ${name} → ${ratio}`, "accent");
      else if (s === "completed") addLog(`Done: ${name} → ${ratio}`, "success");
      else if (typeof s === "object" && s.error)
        addLog(`Failed: ${name} — ${s.error}`, "error");
    });

    const u3 = listen<VideoProgress>("video://progress", (e) => {
      setVideoProgresses((prev) => ({
        ...prev,
        [e.payload.job_id]: e.payload.percent,
      }));
    });

    return () => {
      u1.then((f) => f());
      u2.then((f) => f());
      u3.then((f) => f());
    };
  }, []);

  // ── Orientation detect on file change ─────────────────────
  useEffect(() => {
    if (inputFile && isVideoPath(inputFile)) {
      invoke<OrientationInfo>("detect_orientation", { filePath: inputFile })
        .then((info) => setOrientation(info))
        .catch(() => setOrientation(null));
    } else {
      setOrientation(null);
    }
  }, [inputFile]);

  useEffect(() => {
    if (inputFile && isVideoPath(inputFile)) {
      invoke<FileReadiness>("check_file_ready", { path: inputFile })
        .then((info) => setFileReadiness(info))
        .catch((e) => {
          setFileReadiness(null);
          addLog(`File readiness check failed: ${errorMessage(e)}`, "warn");
        });
    } else {
      setFileReadiness(null);
    }
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
      const all = await invoke<VideoPreset[]>("get_all_presets");
      setPresets(all);
    } catch (e) {
      addLog(`Failed to load presets: ${errorMessage(e)}`, "error");
    }
  };

  // ── Dirty check ────────────────────────────────────────────
  const selectedPreset = useMemo(
    () => presets.find((p) => p.id === selectedPresetId) || null,
    [presets, selectedPresetId],
  );

  const currentValues = useMemo(
    () => buildCurrentState(previewRatio, options),
    [previewRatio, options],
  );

  const presetOverrideCount = useMemo(
    () => getOverrideCount(currentValues, presetBaseValues),
    [currentValues, presetBaseValues],
  );

  const isDirty = presetOverrideCount > 0;

  // ── ETA calculation ────────────────────────────────────────
  const eta = useMemo(() => {
    if (!batchProgress || !batchStartTime || batchProgress.percentage <= 0)
      return null;
    const elapsed = (Date.now() - batchStartTime) / 1000;
    const remaining =
      (elapsed / batchProgress.percentage) * (100 - batchProgress.percentage);
    return remaining;
  }, [batchProgress, batchStartTime]);

  // ── File Handlers ──────────────────────────────────────────
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
        addLog(`Input folder selected, scanning...`, "info");
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

  // ── Drag and Drop ──────────────────────────────────────────
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
      if (files.length === 0) {
        addLog("No valid video files dropped", "warn");
        return;
      }
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

  // ── Logo Picker ────────────────────────────────────────────
  const handlePickLogo = async () => {
    try {
      const sel = await open({
        multiple: false,
        filters: [
          { name: "Image", extensions: ["png", "jpg", "jpeg", "svg", "webp"] },
        ],
      });
      if (sel && typeof sel === "string") {
        setOptions((prev) => ({
          ...prev,
          logo: {
            ...(prev.logo || DEFAULT_OPTIONS.logo!),
            enabled: true,
            path: sel,
          },
        }));
        addLog(`Logo loaded: ${basename(sel)}`, "info");
      }
    } catch (e) {
      addLog(`Logo picker error: ${errorMessage(e)}`, "error");
    }
  };

  // ── Preset Handlers ────────────────────────────────────────
  const handlePresetChange = (id: string) => {
    setSelectedPresetId(id);
    if (!id) {
      setOptions(DEFAULT_OPTIONS);
      setPreviewRatio("ratio9x16");
      setPresetBaseValues(null);
      return;
    }
    const p = presets.find((x) => x.id === id);
    if (!p) return;
    const appliedOptions: ConversionOptions = {
      ...p.options,
      logo: p.logo_path
        ? {
            enabled: true,
            position: "bottom_right",
            opacity: 1,
            gap: 20,
            scale: 0.15,
            path: p.logo_path,
          }
        : null,
      transform: p.options.transform ?? {
        rotate: 0,
        flip_h: false,
        flip_v: false,
      },
    };
    setPreviewRatio(p.ratio);
    setOptions(appliedOptions);
    const baseState = buildCurrentState(p.ratio, appliedOptions);
    setPresetBaseValues(baseState);
    addLog(`Preset loaded as starting point: ${p.name}`, "info");
  };

  // FIX: handleTogglePreset now correctly manages preset state on both
  // select and deselect, and stores per-preset base values for accurate
  // override computation in handleStartBatch.
  const handleTogglePreset = (id: string) => {
    const isCurrentlySelected = selectedPresetIds.includes(id);

    if (isCurrentlySelected) {
      // DESELECTING: remove from active list and clean up its stored base values
      const nextIds = selectedPresetIds.filter((x) => x !== id);
      setSelectedPresetIds(nextIds);

      // Remove this preset's base values from the map
      setPresetBaseValuesMap((prev) => {
        const next = new Map(prev);
        next.delete(id);
        return next;
      });

      // If we're deselecting the currently previewed preset, revert UI to
      // either the last remaining selected preset or clean defaults.
      if (selectedPresetId === id) {
        const fallbackId = nextIds[nextIds.length - 1] ?? "";
        handlePresetChange(fallbackId);
      }
    } else {
      // SELECTING: add to active list and store its base values
      if (selectedPresetIds.length >= 5) {
        addLog("Max 5 presets per batch", "warn");
        return;
      }

      const p = presets.find((x) => x.id === id);
      if (!p) return;

      const appliedOptions: ConversionOptions = {
        ...p.options,
        logo: p.logo_path
          ? {
              enabled: true,
              position: "bottom_right",
              opacity: 1,
              gap: 20,
              scale: 0.15,
              path: p.logo_path,
            }
          : null,
        transform: p.options.transform ?? {
          rotate: 0,
          flip_h: false,
          flip_v: false,
        },
      };
      const baseState = buildCurrentState(p.ratio, appliedOptions);

      // Store the base state for this specific preset
      setPresetBaseValuesMap((prev) => {
        const next = new Map(prev);
        next.set(id, baseState);
        return next;
      });

      setSelectedPresetIds((prev) => [...prev, id]);
    }
  };

  const handleToggleRatio = (r: AspectRatio) => {
    setSelectedRatios((prev) =>
      prev.includes(r) ? prev.filter((x) => x !== r) : [...prev, r],
    );
    setPreviewRatio(r);
  };

  const handleSavePreset = async () => {
    if (!newPresetName.trim()) return;
    const p: VideoPreset = {
      id: Date.now().toString(),
      name: newPresetName,
      description: "Custom preset",
      ratio: previewRatio,
      options,
      logo_path: options.logo?.enabled ? options.logo.path : null,
      platform_config: selectedPreset?.platform_config || null,
      is_builtin: false,
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
      if (selectedPresetId === id) {
        setSelectedPresetId("");
        setPresetBaseValues(null);
      }
      setSelectedPresetIds((prev) => prev.filter((x) => x !== id));
      // Clean up stored base values for the deleted preset
      setPresetBaseValuesMap((prev) => {
        const next = new Map(prev);
        next.delete(id);
        return next;
      });
      addLog("Preset deleted", "info");
    } catch (e) {
      addLog(`Delete failed: ${errorMessage(e)}`, "error");
    }
  };

  // ── Batch Actions ──────────────────────────────────────────
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
    if (fileReadiness?.is_locked) {
      addLog(
        "Selected file appears locked. Close other apps using it first.",
        "warn",
      );
      return;
    }

    const targets: JobTargetRequestDTO[] =
      selectedPresetIds.length > 0
        ? selectedPresetIds.flatMap((id) => {
            const p = presets.find((x) => x.id === id);
            if (!p) return [];

            // FIX: Use per-preset base values from the map, not a shared global.
            // This ensures each preset's overrides are computed relative to that
            // specific preset's defaults, not whatever preset was last clicked.
            const thisPresetBase = presetBaseValuesMap.get(id) ?? null;
            const modifiedKeys = getModifiedOptionKeys(
              currentValues,
              thisPresetBase,
            );
            const overrides: ConversionOptionsRequestDTO = {};
            for (const key of modifiedKeys) {
              (overrides as unknown as Record<string, unknown>)[String(key)] =
                currentValues.options[key];
            }
            return [
              {
                preset_id: p.id,
                overrides,
              },
            ];
          })
        : selectedRatios.map((r) => ({
            ratio: r,
            overrides: toConversionOptionsRequestDTO(options),
          }));

    if (targets.length === 0) {
      addLog("Select at least one preset or ratio", "warn");
      return;
    }

    const totalJobs = files.length * targets.length;
    addLog(
      `Starting batch: ${files.length} file(s) × ${targets.length} target(s) = ${totalJobs} job(s)`,
      "accent",
    );

    try {
      setFileProgresses({});
      setVideoProgresses({});
      setBatchStartTime(Date.now());
      await invoke("start_batch", {
        files,
        settings: {
          targets,
          output_dir: outputDir,
          enable_subfolders: enableSubfolders,
        },
      });
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
      setFileProgresses({});
      setVideoProgresses({});
      setBatchStartTime(null);
      addLog("Queue cleared", "info");
    } catch (e) {
      addLog(`Clear failed: ${errorMessage(e)}`, "error");
    }
  };

  // ── Preview helpers ────────────────────────────────────────
  const previewTransform = useMemo(() => {
    if (!options.transform) return {};
    const { rotate, flip_h, flip_v } = options.transform;
    let t = `rotate(${rotate}deg)`;
    if (flip_h) t += " scaleX(-1)";
    if (flip_v) t += " scaleY(-1)";
    return { transform: t };
  }, [options.transform]);

  // Compute preview box dimensions (max 220px in longest side)
  const previewDims = useMemo(() => {
    const ratio = RATIO_VALUE[previewRatio];
    const MAX = 200;
    if (ratio >= 1) return { width: MAX, height: Math.round(MAX / ratio) };
    return { width: Math.round(MAX * ratio), height: MAX };
  }, [previewRatio]);

  const previewInputFile = useMemo(
    () => (inputFile && isVideoPath(inputFile) ? inputFile : ""),
    [inputFile],
  );

  const isRunning =
    batchProgress &&
    batchProgress.percentage > 0 &&
    batchProgress.percentage < 100;
  const queueItems = Object.values(fileProgresses).reverse();

  const jobCount =
    (batchFiles.length || (inputFile ? 1 : 0)) *
    (selectedPresetIds.length || selectedRatios.length);
  const isFileLocked = !!fileReadiness?.is_locked;
  const hasTargetSelection =
    selectedPresetIds.length > 0 || selectedRatios.length > 0;

  // ── Render ─────────────────────────────────────────────────
  return (
    <div className="app-shell">
      {/* ── Top Bar ──────────────────────────────────────────── */}
      <header className="topbar">
        <div className="topbar-logo">
          <div className="topbar-logo-mark">AS</div>
          <div>
            <div className="topbar-title">AspectShift</div>
            <div className="topbar-subtitle">HTOV Converter</div>
          </div>
        </div>

        <div className="topbar-right">
          {batchProgress &&
            batchProgress.percentage > 0 &&
            batchProgress.percentage < 100 && (
              <span className="eta-badge active">
                <span className="spinner" />
                ETA {eta !== null ? formatETA(eta) : "…"}
              </span>
            )}
          {batchProgress && batchProgress.percentage >= 100 && (
            <span className="badge badge-success">✓ Complete</span>
          )}

          <button
            className="theme-toggle"
            onClick={() => setTheme((t) => (t === "day" ? "night" : "day"))}
            title="Toggle theme"
          >
            {theme === "night" ? "☀" : "☾"}
          </button>
        </div>
      </header>

      {/* ── Main 3-column layout ────────────────────────────── */}
      <div className="main-content">
        {/* ── Left Sidebar: File Import + Settings ─────────── */}
        <aside className="sidebar">
          {/* File Import */}
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
                  <>
                    <strong>{batchFiles.length} files</strong> selected
                  </>
                ) : batchFiles.length === 1 ? (
                  <>
                    <strong>{batchFiles[0]}</strong>
                  </>
                ) : inputFile ? (
                  <>
                    <strong>{basename(inputFile)}</strong>
                  </>
                ) : (
                  <>
                    <strong>Drop videos here</strong> or click to browse
                  </>
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

          {/* Output Dir */}
          <div className="sidebar-section">
            <div className="sidebar-section-title">Output Directory</div>
            <div className="path-row">
              <div
                className={`path-display${outputDir ? "" : " empty"}`}
                title={outputDir}
              >
                {outputDir ? outputDir : "No output directory selected"}
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
                <button
                  className="btn btn-sm"
                  onClick={handleOpenOutput}
                  title="Open folder"
                >
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

          {/* Settings Tabs */}
          <div className="tabs" style={{ margin: "0 0 0 0" }}>
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
              Save
            </button>
          </div>

          <div className="settings-scroll">
            {/* ── Effects Tab ── */}
            {settingsTab === "effects" && (
              <>
                <div className="settings-group">
                  <div className="settings-group-title">Background</div>
                  <div className="toggle-row">
                    <span className="toggle-label">
                      Blur Background
                      <span className="label-desc">Gaussian fill</span>
                    </span>
                    <Toggle
                      checked={options.blur_background}
                      onChange={(v) =>
                        setOptions({ ...options, blur_background: v })
                      }
                    />
                  </div>
                  {options.blur_background && (
                    <div className="slider-row mt-2">
                      <span className="text-sm text-muted">Sigma</span>
                      <input
                        className="slider"
                        type="range"
                        min="5"
                        max="60"
                        value={options.blur_sigma}
                        onChange={(e) =>
                          setOptions({
                            ...options,
                            blur_sigma: parseFloat(e.target.value),
                          })
                        }
                      />
                      <span className="slider-value">{options.blur_sigma}</span>
                    </div>
                  )}
                </div>

                <div className="settings-group">
                  <div className="settings-group-title">Audio</div>
                  <div className="toggle-row">
                    <span className="toggle-label">Remove Audio</span>
                    <Toggle
                      checked={options.remove_audio}
                      onChange={(v) =>
                        setOptions({ ...options, remove_audio: v })
                      }
                    />
                  </div>
                </div>

                <div className="settings-group">
                  <div className="settings-group-title">Subtitles</div>
                  <div className="toggle-row">
                    <span className="toggle-label">
                      Generate Subtitles
                      <span className="label-desc">via Whisper AI</span>
                    </span>
                    <Toggle
                      checked={options.generate_subtitles ?? false}
                      onChange={(v) =>
                        setOptions({ ...options, generate_subtitles: v })
                      }
                    />
                  </div>
                  <div className="toggle-row">
                    <span className="toggle-label">
                      Burn Subtitles
                      <span className="label-desc">hard-coded to video</span>
                    </span>
                    <Toggle
                      checked={options.burn_subtitles ?? false}
                      onChange={(v) =>
                        setOptions({
                          ...options,
                          burn_subtitles: v,
                          generate_subtitles: v
                            ? true
                            : (options.generate_subtitles ?? false),
                        })
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
                        setOptions({
                          ...options,
                          transform: {
                            rotate:
                              ((options.transform?.rotate ?? 0) + 90) % 360,
                            flip_h: options.transform?.flip_h ?? false,
                            flip_v: options.transform?.flip_v ?? false,
                          },
                        })
                      }
                    >
                      ↻ Rotate {options.transform?.rotate || 0}°
                    </button>
                    <button
                      className="btn btn-sm"
                      onClick={() =>
                        setOptions({
                          ...options,
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
                        id="flip-h"
                        checked={options.transform?.flip_h || false}
                        onChange={(e) =>
                          setOptions({
                            ...options,
                            transform: {
                              ...options.transform!,
                              flip_h: e.target.checked,
                            },
                          })
                        }
                      />
                      <label className="checkbox-label" htmlFor="flip-h">
                        Flip H
                      </label>
                    </div>
                    <div className="checkbox-row">
                      <input
                        type="checkbox"
                        id="flip-v"
                        checked={options.transform?.flip_v || false}
                        onChange={(e) =>
                          setOptions({
                            ...options,
                            transform: {
                              ...options.transform!,
                              flip_v: e.target.checked,
                            },
                          })
                        }
                      />
                      <label className="checkbox-label" htmlFor="flip-v">
                        Flip V
                      </label>
                    </div>
                  </div>
                </div>

                <div className="settings-group">
                  <div className="settings-group-title">Logo Watermark</div>
                  <div className="toggle-row mb-2">
                    <span className="toggle-label">Enable Logo</span>
                    <Toggle
                      checked={options.logo?.enabled || false}
                      onChange={(v) =>
                        setOptions({
                          ...options,
                          logo: v
                            ? {
                                ...(options.logo || {
                                  position: "bottom_right",
                                  opacity: 1,
                                  gap: 20,
                                  scale: 0.15,
                                  path: null,
                                }),
                                enabled: true,
                              }
                            : null,
                        })
                      }
                    />
                  </div>
                  {options.logo?.enabled && (
                    <>
                      <div
                        className="logo-upload-zone mt-2"
                        onClick={handlePickLogo}
                      >
                        {options.logo?.path &&
                        isImagePath(options.logo.path) ? (
                          <img
                            className="logo-preview-thumb"
                            src={convertFileSrc(options.logo.path)}
                            alt="logo preview"
                          />
                        ) : (
                          <span style={{ fontSize: 24, opacity: 0.5 }}>🖼</span>
                        )}
                        <div className="logo-upload-text">
                          <strong>
                            {options.logo?.path
                              ? basename(options.logo.path)
                              : "No logo"}
                          </strong>
                          {options.logo?.path
                            ? "Click to change"
                            : "Click to upload PNG/SVG"}
                        </div>
                      </div>
                      <div className="mt-4">
                        <label className="input-label">Position</label>
                        <select
                          className="input select"
                          value={options.logo?.position || "bottom_right"}
                          onChange={(e) =>
                            setOptions({
                              ...options,
                              logo: {
                                ...options.logo!,
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
                      </div>
                      <div className="slider-row mt-2">
                        <span className="text-sm text-muted">Opacity</span>
                        <input
                          className="slider"
                          type="range"
                          min="0"
                          max="1"
                          step="0.05"
                          value={options.logo?.opacity || 1}
                          onChange={(e) =>
                            setOptions({
                              ...options,
                              logo: {
                                ...options.logo!,
                                opacity: parseFloat(e.target.value),
                              },
                            })
                          }
                        />
                        <span className="slider-value">
                          {Math.round((options.logo?.opacity || 1) * 100)}%
                        </span>
                      </div>
                      <div className="slider-row mt-2">
                        <span className="text-sm text-muted">Scale</span>
                        <input
                          className="slider"
                          type="range"
                          min="0.05"
                          max="0.5"
                          step="0.01"
                          value={options.logo?.scale || 0.15}
                          onChange={(e) =>
                            setOptions({
                              ...options,
                              logo: {
                                ...options.logo!,
                                scale: parseFloat(e.target.value),
                              },
                            })
                          }
                        />
                        <span className="slider-value">
                          {Math.round((options.logo?.scale || 0.15) * 100)}%
                        </span>
                      </div>
                    </>
                  )}
                </div>

                <div className="settings-group">
                  <div className="settings-group-title">Misc</div>
                  <div className="toggle-row">
                    <span className="toggle-label">Skip Existing</span>
                    <Toggle
                      checked={options.skip_existing}
                      onChange={(v) =>
                        setOptions({ ...options, skip_existing: v })
                      }
                    />
                  </div>
                  <div className="mt-4">
                    <label className="input-label">Output Format</label>
                    <select
                      className="input select"
                      value={options.output_format}
                      onChange={(e) =>
                        setOptions({
                          ...options,
                          output_format: e.target.value as OutputFormat,
                        })
                      }
                    >
                      <option value="mp4">MP4</option>
                      <option value="mov">MOV</option>
                      <option value="webm">WebM</option>
                    </select>
                  </div>
                </div>
              </>
            )}

            {/* ── Encode Tab ── */}
            {settingsTab === "encode" && (
              <>
                <div className="settings-group">
                  <div className="settings-group-title">Quality</div>
                  <div className="mt-2">
                    <label className="input-label">Quality Preset</label>
                    <select
                      className="input select"
                      value={options.quality}
                      onChange={(e) =>
                        setOptions({
                          ...options,
                          quality: e.target.value as QualityPreset,
                        })
                      }
                    >
                      <option value="draft">Draft (fast)</option>
                      <option value="standard">Standard</option>
                      <option value="high">High (slow)</option>
                    </select>
                  </div>
                  <div className="toggle-row mt-4">
                    <span className="toggle-label">
                      Custom Encoding
                      <span className="label-desc">
                        override quality preset
                      </span>
                    </span>
                    <Toggle
                      checked={options.custom_encoding_enabled}
                      onChange={(v) =>
                        setOptions({ ...options, custom_encoding_enabled: v })
                      }
                    />
                  </div>
                </div>

                {options.custom_encoding_enabled && (
                  <div className="settings-group">
                    <div className="settings-group-title">Advanced</div>
                    <div className="slider-row">
                      <span className="text-sm text-muted">CRF</span>
                      <input
                        className="slider"
                        type="range"
                        min="0"
                        max="51"
                        value={options.crf ?? 18}
                        onChange={(e) =>
                          setOptions({
                            ...options,
                            crf: Number.parseInt(e.target.value, 10),
                          })
                        }
                      />
                      <span className="slider-value">{options.crf ?? 18}</span>
                    </div>
                    <div className="mt-4">
                      <label className="input-label">Speed Preset</label>
                      <select
                        className="input select"
                        value={options.preset || "medium"}
                        onChange={(e) =>
                          setOptions({ ...options, preset: e.target.value })
                        }
                      >
                        <option value="slow">Slow (best quality)</option>
                        <option value="medium">Medium</option>
                        <option value="fast">Fast</option>
                        <option value="veryfast">Very Fast</option>
                      </select>
                    </div>
                    <div className="mt-4">
                      <label className="input-label">Audio Bitrate</label>
                      <select
                        className="input select"
                        value={options.audio_bitrate || "128k"}
                        onChange={(e) =>
                          setOptions({
                            ...options,
                            audio_bitrate: e.target.value,
                          })
                        }
                      >
                        <option value="96k">96k</option>
                        <option value="128k">128k</option>
                        <option value="192k">192k</option>
                        <option value="256k">256k</option>
                        <option value="320k">320k</option>
                      </select>
                    </div>
                  </div>
                )}
              </>
            )}

            {/* ── Preset Save Tab ── */}
            {settingsTab === "presets" && (
              <div className="settings-group">
                <div className="settings-group-title">Save as Preset</div>
                {isDirty && selectedPreset && (
                  <div className="banner banner-warning mb-4">
                    ⚠ You customized {presetOverrideCount} setting
                    {presetOverrideCount > 1 ? "s" : ""} from "
                    {selectedPreset.name}"
                  </div>
                )}
                <div className="preset-builder">
                  <div className="preset-builder-title">
                    New preset from current settings
                  </div>
                  <div className="mt-2">
                    <label className="input-label">Name</label>
                    <input
                      className="input"
                      placeholder="e.g. My TikTok 4K"
                      value={newPresetName}
                      onChange={(e) => setNewPresetName(e.target.value)}
                    />
                  </div>
                  <div className="mt-2">
                    <button
                      className="btn btn-primary btn-sm btn-full mt-2"
                      onClick={handleSavePreset}
                      disabled={!newPresetName.trim()}
                    >
                      Save Preset
                    </button>
                  </div>
                </div>

                {presets.filter((p) => !p.is_builtin).length > 0 && (
                  <div className="mt-8">
                    <div className="settings-group-title">Custom Presets</div>
                    {presets
                      .filter((p) => !p.is_builtin)
                      .map((p) => (
                        <div
                          key={p.id}
                          className="flex items-center justify-between"
                          style={{
                            padding: "5px 0",
                            borderBottom: "1px solid var(--border)",
                          }}
                        >
                          <span
                            className="text-sm"
                            style={{
                              color: "var(--text-primary)",
                              fontWeight: 600,
                            }}
                          >
                            {p.name}
                          </span>
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

        {/* ── Center: Preview + Ratios + Controls ──────────── */}
        <main className="center-panel">
          {/* Ratio Selector */}
          <div
            style={{
              padding: "14px 20px 0",
              borderBottom: "1px solid var(--border)",
              background: "var(--bg-card)",
              flexShrink: 0,
            }}
          >
            <div className="flex items-center justify-between mb-4">
              <span className="section-title">Aspect Ratio Targets</span>
              {selectedPreset && presetOverrideCount > 0 && (
                <span className="badge badge-accent">
                  {presetOverrideCount} setting
                  {presetOverrideCount > 1 ? "s" : ""} customized
                </span>
              )}
            </div>
            <div className="ratio-pills" style={{ paddingBottom: 12 }}>
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

          {/* Video Preview */}
          <div className="preview-wrapper">
            {previewInputFile ? (
              <>
                <div
                  className="preview-aspect-container"
                  style={{
                    width: previewDims.width,
                    height: previewDims.height,
                  }}
                >
                  {options.blur_background ? (
                    <>
                      <video
                        src={convertFileSrc(previewInputFile)}
                        className="preview-blur-bg"
                        style={{
                          filter: `blur(${options.blur_sigma}px)`,
                          ...previewTransform,
                        }}
                        autoPlay
                        muted
                        loop
                        playsInline
                      />
                      <video
                        src={convertFileSrc(previewInputFile)}
                        className="preview-video-fg"
                        style={previewTransform}
                        autoPlay
                        muted
                        loop
                        playsInline
                      />
                    </>
                  ) : (
                    <video
                      key={previewInputFile}
                      ref={videoRef}
                      src={convertFileSrc(previewInputFile)}
                      className="preview-video"
                      style={previewTransform}
                      autoPlay
                      muted
                      loop
                      playsInline
                    />
                  )}
                </div>
                {orientation && (
                  <div className="preview-meta">
                    <span className="preview-meta-item">
                      <span style={{ opacity: 0.5 }}>⬛</span>
                      {orientation.display_width}×{orientation.display_height}
                    </span>
                    <span className="preview-meta-item">
                      <span style={{ opacity: 0.5 }}>◱</span>
                      {orientation.is_vertical ? "Vertical" : "Horizontal"}
                    </span>
                    {orientation.rotation !== 0 && (
                      <span className="preview-meta-item">
                        <span style={{ opacity: 0.5 }}>↻</span>
                        {orientation.rotation}°
                      </span>
                    )}
                    <span className="preview-meta-item">
                      <span style={{ opacity: 0.5 }}>◱</span>
                      {RATIO_DISPLAY[previewRatio]}
                    </span>
                    {fileReadiness && (
                      <>
                        <span className="preview-meta-item">
                          <span style={{ opacity: 0.5 }}>⏱</span>
                          {formatDuration(
                            fileReadiness.estimated_duration_secs,
                          )}
                        </span>
                        <span className="preview-meta-item">
                          <span style={{ opacity: 0.5 }}>⇩</span>
                          {formatBytes(fileReadiness.file_size_bytes)}
                        </span>
                        {fileReadiness.is_locked && (
                          <span
                            className="preview-meta-item"
                            style={{ color: "var(--warning)" }}
                          >
                            <span style={{ opacity: 0.9 }}>⚠</span>
                            File locked
                          </span>
                        )}
                      </>
                    )}
                  </div>
                )}
              </>
            ) : (
              <div className="preview-empty">
                <div className="preview-empty-icon">🎬</div>
                <div className="preview-empty-text">No video selected</div>
              </div>
            )}
          </div>

          {/* Preset Cards (Platform Selector) */}
          <div
            style={{
              padding: "10px 20px",
              borderTop: "1px solid var(--border)",
              background: "var(--bg-card)",
              flexShrink: 0,
            }}
          >
            <div className="flex items-center justify-between mb-4">
              <span className="section-title">Platform Presets</span>
              <span className="text-xs text-muted">
                Select up to 5 for batch
              </span>
            </div>
            <div className="presets-grid">
              {presets.map((p) => {
                const isSelected = selectedPresetIds.includes(p.id);
                return (
                  <div
                    key={p.id}
                    className={`preset-card${isSelected ? " selected" : ""}`}
                    onClick={() => {
                      // FIX: Only load this preset's settings into the UI panel
                      // when SELECTING it. When DESELECTING, handleTogglePreset
                      // handles the state cleanup (reverts to fallback or defaults).
                      // This prevents a deselected preset from persisting its
                      // settings as the "active" configuration.
                      if (!isSelected) {
                        handlePresetChange(p.id);
                      }
                      handleTogglePreset(p.id);
                    }}
                  >
                    <input
                      type="checkbox"
                      checked={isSelected}
                      onChange={() => {}}
                      onClick={(e) => e.stopPropagation()}
                    />
                    <div className="preset-card-name">
                      {PLATFORM_ICONS[p.id] && (
                        <span style={{ marginRight: 4, opacity: 0.6 }}>
                          {PLATFORM_ICONS[p.id]}
                        </span>
                      )}
                      {p.name}
                    </div>
                    <div className="preset-card-ratio">
                      {RATIO_DISPLAY[p.ratio]}
                    </div>
                    {p.platform_config && (
                      <div className="preset-card-desc text-xs text-muted">
                        {p.platform_config.target_width}×
                        {p.platform_config.target_height}
                      </div>
                    )}
                    {!p.is_builtin && (
                      <span className="preset-card-badge">custom</span>
                    )}
                  </div>
                );
              })}
            </div>
          </div>

          {/* Start / Cancel Controls */}
          <div className="controls-bar">
            <button
              className="btn btn-primary btn-lg flex-1"
              onClick={handleStartBatch}
              disabled={!!isRunning || isFileLocked || !hasTargetSelection}
              title={
                isFileLocked
                  ? "File is locked. Close other applications using it first."
                  : undefined
              }
            >
              {isRunning ? (
                <>
                  <span className="spinner" /> Processing…
                </>
              ) : (
                <>
                  ▶{" "}
                  {jobCount > 1
                    ? `Start Batch (${jobCount} jobs)`
                    : "Convert Now"}
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
                  disabled={!!isRunning}
                >
                  Clear
                </button>
              </>
            )}
            {outputDir && (
              <button
                className="btn btn-ghost"
                onClick={handleOpenOutput}
                title="Open output folder"
              >
                ↗
              </button>
            )}
          </div>
        </main>

        {/* ── Right Panel: Queue + Log ──────────────────────── */}
        <aside className="right-panel">
          {/* Overall progress */}
          {batchProgress && batchProgress.total_jobs > 0 && (
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
                  style={{
                    fontFamily: "'JetBrains Mono', monospace",
                    fontSize: 12,
                    fontWeight: 700,
                    color: "var(--accent)",
                  }}
                >
                  {batchProgress.percentage.toFixed(1)}%
                </span>
              </div>
              <div className="progress-bar" style={{ marginBottom: 6 }}>
                <div
                  className={`progress-bar-fill${isRunning ? " animated" : ""}`}
                  style={{ width: `${batchProgress.percentage}%` }}
                />
              </div>
              <div className="batch-stats-nums">
                <span className="stat-pill stat-completed">
                  ✓ {batchProgress.completed_jobs}
                </span>
                {batchProgress.failed_jobs > 0 && (
                  <span className="stat-pill stat-failed">
                    ✕ {batchProgress.failed_jobs}
                  </span>
                )}
                <span className="stat-pill stat-pending">
                  {batchProgress.total_jobs -
                    batchProgress.completed_jobs -
                    batchProgress.failed_jobs}{" "}
                  pending
                </span>
                {eta !== null && isRunning && (
                  <span
                    className="stat-pill"
                    style={{
                      background: "var(--accent-subtle)",
                      color: "var(--accent)",
                    }}
                  >
                    ⏱ {formatETA(eta)}
                  </span>
                )}
              </div>
            </div>
          )}

          {/* Tabs */}
          <div className="tabs" style={{ marginTop: 8 }}>
            <button
              className={`tab${rightTab === "queue" ? " active" : ""}`}
              onClick={() => setRightTab("queue")}
            >
              Queue {queueItems.length > 0 && `(${queueItems.length})`}
            </button>
            <button
              className={`tab${rightTab === "log" ? " active" : ""}`}
              onClick={() => setRightTab("log")}
            >
              Log {logs.length > 0 && `(${logs.length})`}
            </button>
          </div>

          {/* Queue */}
          {rightTab === "queue" && (
            <div className="queue-list">
              {queueItems.length === 0 ? (
                <div
                  style={{
                    padding: "30px 0",
                    textAlign: "center",
                    color: "var(--text-muted)",
                    fontSize: 12,
                  }}
                >
                  <div style={{ fontSize: 28, marginBottom: 8, opacity: 0.3 }}>
                    📋
                  </div>
                  No jobs yet
                </div>
              ) : (
                queueItems.map((job) => {
                  const statusStr =
                    typeof job.status === "string" ? job.status : "failed";
                  const isProc = job.status === "processing";
                  const vp = videoProgresses[job.job_id] || 0;

                  return (
                    <div
                      key={job.job_id}
                      className={`queue-item${isProc ? " is-processing" : ""}`}
                    >
                      <div className="queue-item-icon">
                        {statusStr === "processing"
                          ? "⚙"
                          : statusStr === "completed"
                            ? "✓"
                            : statusStr === "failed"
                              ? "✕"
                              : "·"}
                      </div>
                      <div className="queue-item-body">
                        <div className="queue-item-name">
                          {basename(job.file_path)}
                        </div>
                        <div className="queue-item-meta">
                          <span className="queue-item-ratio">
                            {RATIO_DISPLAY[job.ratio]}
                          </span>
                          {isProc && vp > 0 && (
                            <span
                              style={{ fontSize: 10, color: "var(--accent)" }}
                            >
                              {vp.toFixed(0)}%
                            </span>
                          )}
                          {typeof job.status === "object" &&
                            job.status.error && (
                              <span
                                style={{ color: "var(--error)", fontSize: 10 }}
                                title={job.status.error}
                              >
                                {job.status.error.slice(0, 30)}…
                              </span>
                            )}
                        </div>
                        {isProc && (
                          <div
                            className="progress-bar"
                            style={{ marginTop: 4, height: 3 }}
                          >
                            <div
                              className={`progress-bar-fill${vp <= 0 ? " animated" : ""}`}
                              style={{ width: `${vp || 5}%` }}
                            />
                          </div>
                        )}
                      </div>
                      <div className={`queue-item-status status-${statusStr}`}>
                        {statusStr === "processing"
                          ? "⟳"
                          : statusStr === "completed"
                            ? "✓"
                            : statusStr === "failed"
                              ? "✕"
                              : statusStr === "cancelled"
                                ? "⊘"
                                : "·"}
                      </div>
                    </div>
                  );
                })
              )}
            </div>
          )}

          {/* Log */}
          {rightTab === "log" && (
            <div className="log-panel">
              <div className="log-body" ref={logRef}>
                {logs.length === 0 ? (
                  <div
                    style={{
                      padding: "30px 0",
                      textAlign: "center",
                      color: "var(--text-muted)",
                      fontSize: 11,
                    }}
                  >
                    Activity log will appear here
                  </div>
                ) : (
                  logs.map((entry, i) => (
                    <div key={i} className="log-entry">
                      <span className="log-time">{entry.time}</span>
                      <span className={`log-msg log-${entry.type}`}>
                        {entry.msg}
                      </span>
                    </div>
                  ))
                )}
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
