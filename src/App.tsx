import { useState, useEffect, useMemo, useRef, useCallback } from "react";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import type {
  AspectRatio,
  AspectRatioTarget,
  BatchProgress,
  EncodingProfile,
  FileProgress,
  FileReadiness,
  LogoPosition,
  OrientationInfo,
  OutputJob,
  VideoProgress,
  VideoTransform,
  VideoEffectsSettings,
  CustomPreset,
  LogoOptions,
  PlatformPreset,
} from "./types/backend";
import "./App.css";
import { VideoCanvas } from "./components/VideoCanvas";
import { PresetsPanel } from "./components/PresetsPanel";

interface PresetComparableState {
  ratio: AspectRatio;
  encoding: EncodingProfile;
}
interface ModifiedPreset {
  basePresetId: string;
  overriddenEncoding: EncodingProfile;
  isDirty: true;
}
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
const EDITOR_UI_LABELS = {
  effects: {
    category: "Effects",
    blur: "Blur Background",
  },
  encode: {
    category: "Encoding",
  },
  presets: {
    category: "Presets",
  },
} as const;

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

function normalizeEncoding(encoding: EncodingProfile): EncodingProfile {
  return {
    crf: Number(encoding.crf),
    qualityPreset: encoding.qualityPreset,
    speedPreset: encoding.speedPreset,
    audioBitrate: encoding.audioBitrate,
  };
}

function normalizeEffects(effects: VideoEffectsSettings): VideoEffectsSettings {
  return {
    ...effects,
    logo: normalizeLogo(effects.logo ?? null),
    transform: normalizeTransform(effects.transform),
  };
}

function buildCurrentState(
  ratio: AspectRatio,
  encoding: EncodingProfile,
): PresetComparableState {
  return {
    ratio,
    encoding: normalizeEncoding(encoding),
  };
}

function getOverrideCount(
  current: PresetComparableState,
  defaults: PresetComparableState | null,
): number {
  if (!defaults) return 0;
  const keys: (keyof EncodingProfile)[] = [
    "crf",
    "qualityPreset",
    "speedPreset",
    "audioBitrate",
  ];

  let diff = current.ratio === defaults.ratio ? 0 : 1;
  for (const key of keys) {
    if (
      JSON.stringify(current.encoding[key]) !==
      JSON.stringify(defaults.encoding[key])
    ) {
      diff += 1;
    }
  }
  return diff;
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

  // Selection & Ratios
  const [selectionHistory, setSelectionHistory] = useState<SelectionItem[]>([]);
  const [selectedRatios, setSelectedRatios] = useState<AspectRatio[]>([]);
  const [selectedPresetIds, setSelectedPresetIds] = useState<string[]>([]);
  const [aspectRatioTargets, setAspectRatioTargets] = useState<
    AspectRatioTarget[]
  >([]);

  // Settings State
  const [encodingState, setEncodingState] =
    useState<EncodingProfile>(DEFAULT_ENCODING);
  const [effectsState, setEffectsState] =
    useState<VideoEffectsSettings>(DEFAULT_EFFECTS);
  const [presets, setPresets] = useState<PlatformPreset[]>([]);
  const [selectedPresetId, setSelectedPresetId] = useState<string>("");
  const [modifiedPresets, setModifiedPresets] = useState<
    Record<string, ModifiedPreset>
  >({});
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

  // Derived Active State
  const activeSelection = selectionHistory[selectionHistory.length - 1] || null;

  const effectivePreviewRatio = useMemo(() => {
    if (!activeSelection) {
      if (orientation)
        return orientation.displayWidth / orientation.displayHeight;
      return 9 / 16;
    }
    if (activeSelection.type === "aspectRatio") {
      return RATIO_VALUE[activeSelection.id];
    }
    const p = presets.find((x) => x.id === activeSelection.id);
    return p ? RATIO_VALUE[p.ratio] : 9 / 16;
  }, [activeSelection, orientation, presets]);

  // Sync settings when active selection changes
  useEffect(() => {
    if (!activeSelection) {
      setSelectedPresetId("");
      setPresetBaseValues(null);
      return;
    }

    if (activeSelection.type === "platform") {
      const p = presets.find((x) => x.id === activeSelection.id);
      if (p) {
        setSelectedPresetId(p.id);
        const modified = modifiedPresets[p.id];
        const appliedEncoding = modified
          ? deepClone(modified.overriddenEncoding)
          : deepClone(p.encoding);
        setEncodingState(appliedEncoding);
        setPresetBaseValues(buildCurrentState(p.ratio, p.encoding));
      }
    } else {
      setSelectedPresetId("");
      setPresetBaseValues(null);
    }
  }, [activeSelection, presets]);

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
    const u1 = listen<BatchProgress>("batch://progress", (e) => {
      setBatchProgress(e.payload);
      if (e.payload.status === "processing" && e.payload.percentage === 0) {
        setBatchStartTime(Date.now());
      }

      if (e.payload.status === "completed") {
        addLog("Batch complete ✓", "success");
      } else if (e.payload.status === "cancelled") {
        addLog("Batch cancelled by user", "warn");
      } else if (e.payload.status === "failed") {
        addLog("Batch finished with errors", "error");
      }
    });

    const u2 = listen<FileProgress>("batch://file-status", (e) => {
      const name = basename(e.payload.filePath);
      const ratio = RATIO_DISPLAY[e.payload.ratio];
      const s = e.payload.status;

      if (s === "processing")
        addLog(`Processing: ${name} → ${ratio}`, "accent");
      else if (s === "completed") addLog(`Done: ${name} → ${ratio}`, "success");
      else if (s === "cancelled")
        addLog(`Cancelled: ${name} → ${ratio}`, "warn");
      else if (typeof s === "object" && s.error)
        addLog(`Failed: ${name} — ${s.error}`, "error");
    });

    const u3 = listen<VideoProgress>("video://progress", (e) => {
      setVideoProgresses((prev) => ({
        ...prev,
        [e.payload.jobId]: e.payload.percent,
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

      invoke<FileReadiness>("check_file_ready", { path: inputFile })
        .then((info) => setFileReadiness(info))
        .catch((e) => {
          setFileReadiness(null);
          addLog(`File readiness check failed: ${errorMessage(e)}`, "warn");
        });
    } else {
      setOrientation(null);
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
      const all = await invoke<PlatformPreset[]>("get_all_presets");
      setPresets(all);
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

  // ── Dirty check ────────────────────────────────────────────
  const currentRatioTag = useMemo(() => {
    if (!activeSelection) return "ratio9x16"; // Dummy for buildCurrentState
    if (activeSelection.type === "aspectRatio") return activeSelection.id;
    const p = presets.find((x) => x.id === activeSelection.id);
    return p ? p.ratio : "ratio9x16";
  }, [activeSelection, presets]);

  const presetOverrideCount = useMemo(() => {
    if (!selectedPresetId) return 0;
    return getOverrideCount(
      buildCurrentState(currentRatioTag, encodingState),
      presetBaseValues,
    );
  }, [selectedPresetId, currentRatioTag, encodingState, presetBaseValues]);

  const isDirty = !!modifiedPresets[selectedPresetId]?.isDirty;

  useEffect(() => {
    if (!selectedPresetId) return;
    if (presetOverrideCount === 0) {
      setModifiedPresets((prev) => {
        if (!prev[selectedPresetId]) return prev;
        const next = { ...prev };
        delete next[selectedPresetId];
        return next;
      });
      return;
    }
    setModifiedPresets((prev) => ({
      ...prev,
      [selectedPresetId]: {
        basePresetId: selectedPresetId,
        overriddenEncoding: deepClone(encodingState),
        isDirty: true,
      },
    }));
  }, [encodingState, presetOverrideCount, selectedPresetId]);

  // ── ETA calculation ────────────────────────────────────────
  const eta = useMemo(() => {
    if (!batchProgress || !batchStartTime || batchProgress.percentage <= 0)
      return null;
    const elapsed = (Date.now() - batchStartTime) / 1000;
    const remaining =
      (elapsed / batchProgress.percentage) * (100 - batchProgress.percentage);
    return remaining;
  }, [batchProgress, batchStartTime]);

  // ── Handlers ───────────────────────────────────────────
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
            ...(prev.logo || {
              position: "bottom_right",
              opacity: 1,
              gap: 20,
              scale: 0.15,
              path: null,
            }),
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

  const handleToggleRatio = (r: AspectRatio) => {
    setSelectedRatios((prev) => {
      const isSelected = prev.includes(r);
      if (isSelected) {
        setSelectionHistory((h) =>
          h.filter((x) => !(x.type === "aspectRatio" && x.id === r)),
        );
        return prev.filter((x) => x !== r);
      } else {
        setSelectionHistory((h) => [...h, { type: "aspectRatio", id: r }]);
        return [...prev, r];
      }
    });
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

    const selections = [
      ...selectedPresetIds.map((id) => ({ type: "platform" as const, id })),
      ...selectedRatios.map((id) => ({ type: "aspectRatio" as const, id })),
    ];

    if (selections.length === 0) {
      addLog("Select at least one preset or ratio", "warn");
      return;
    }

    const targets: OutputJob[] = selections.map((sel) => {
      if (sel.type === "platform") {
        const p = presets.find((x) => x.id === sel.id)!;
        const encoding =
          modifiedPresets[p.id]?.overriddenEncoding ?? deepClone(p.encoding);
        return {
          id: generateId(),
          sourcePresetId: `platform:${p.id}`,
          ratio: p.ratio,
          encoding,
          effects: deepClone(normalizeEffects(effectsState)),
          platformConfig: p.platformConfig,
          presetName: p.name,
        };
      } else {
        const target = aspectRatioTargets.find((t) => t.ratio === sel.id)!;
        return {
          id: generateId(),
          sourcePresetId: `aspectRatio:${sel.id}`,
          ratio: sel.id,
          encoding: deepClone(target?.encoding ?? DEFAULT_ENCODING),
          effects: deepClone(normalizeEffects(effectsState)),
          platformConfig: null,
          presetName: null,
        };
      }
    });

    try {
      setVideoProgresses({});
      setBatchStartTime(Date.now());
      await invoke("start_batch", {
        files,
        settings: { targets, outputDir, enableSubfolders },
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
      setVideoProgresses({});
      setBatchStartTime(null);
      addLog("Queue cleared", "info");
    } catch (e) {
      addLog(`Clear failed: ${errorMessage(e)}`, "error");
    }
  };

  const handleSavePreset = async () => {
    if (!newPresetName.trim() || !activeSelection) return;
    const p: CustomPreset = {
      id: Date.now().toString(),
      name: newPresetName,
      ratio: currentRatioTag,
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

  // UI state for guides
  const [showGuides, setShowGuides] = useState(true);
  const [showSafeFrames, setShowSafeFrames] = useState(false);

  const isRunning = batchProgress?.status === "processing";

  // Authoritative queue from backend, sorted so current/completed are visible
  const queueItems = useMemo(() => {
    if (!batchProgress?.queue) return [];
    // Sort: Processing first, then Queued, then others (Completed/Failed)
    return [...batchProgress.queue].sort((a, b) => {
      const score = (s: typeof a.status) => {
        if (s === "processing") return 0;
        if (s === "queued") return 1;
        if (s === "completed") return 2;
        return 3;
      };
      return score(a.status) - score(b.status);
    });
  }, [batchProgress?.queue]);

  const jobCount =
    (batchFiles.length || (inputFile ? 1 : 0)) *
    (selectedPresetIds.length + selectedRatios.length);
  const isFileLocked = !!fileReadiness?.isLocked;

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
          {batchProgress && batchProgress.percentage >= 100 && (
            <span className="badge badge-success">✓ Complete</span>
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
              {EDITOR_UI_LABELS.effects.category}
            </button>
            <button
              className={`tab${settingsTab === "encode" ? " active" : ""}`}
              onClick={() => setSettingsTab("encode")}
            >
              {EDITOR_UI_LABELS.encode.category}
            </button>
            <button
              className={`tab${settingsTab === "presets" ? " active" : ""}`}
              onClick={() => setSettingsTab("presets")}
            >
              {EDITOR_UI_LABELS.presets.category}
            </button>
          </div>

          <div className="settings-scroll">
            {settingsTab === "effects" && (
              <>
                <div className="settings-group">
                  <div className="settings-group-title">
                    {EDITOR_UI_LABELS.effects.category}
                  </div>
                  <div className="toggle-row">
                    <span className="toggle-label">
                      {EDITOR_UI_LABELS.effects.blur}
                    </span>
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
                            v || effectsState.generateSubtitles,
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
                      <label htmlFor="fh">Flip H</label>
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
                      <label htmlFor="fv">Flip V</label>
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
                                ...(effectsState.logo || {
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
                    </>
                  )}
                </div>
              </>
            )}
            {settingsTab === "encode" && (
              <div className="settings-group">
                <div className="settings-group-title">Quality</div>
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
              </div>
            )}
            {settingsTab === "presets" && (
              <div className="settings-group">
                <div className="settings-group-title">Save Preset</div>
                <input
                  className="input"
                  placeholder="Name"
                  value={newPresetName}
                  onChange={(e) => setNewPresetName(e.target.value)}
                />
                <button
                  className="btn btn-primary btn-sm btn-full mt-2"
                  onClick={handleSavePreset}
                  disabled={!newPresetName.trim()}
                >
                  Save
                </button>
                {presets.filter((p) => !p.isBuiltin).length > 0 && (
                  <div className="mt-8">
                    {presets
                      .filter((p) => !p.isBuiltin)
                      .map((p) => (
                        <div
                          key={p.id}
                          className="flex items-center justify-between py-1 border-b border-[var(--border)]"
                        >
                          <span className="text-sm font-semibold">
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

        <main className="center-panel">
          <div className="p-4 border-b border-[var(--border)] bg-[var(--bg-card)] shrink-0">
            <div className="flex items-center justify-between mb-4">
              <span className="section-title">Aspect Ratio Targets</span>
              {isDirty && <span className="badge badge-accent">Modified</span>}
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

          <div className="p-2 px-5 border-t border-[var(--border)] bg-[var(--bg-card)] shrink-0">
            <PresetsPanel
              presets={presets}
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
                  className="progress-bar-fill"
                  style={{ width: `${batchProgress.percentage}%` }}
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
                {isRunning && eta !== null && (
                  <span className="stat-pill bg-[var(--accent-subtle)] text-accent">
                    ⏱ {formatETA(eta)}
                  </span>
                )}
              </div>
            </div>
          )}

          <div className="tabs mt-2">
            <button
              className={`tab${rightTab === "queue" ? " active" : ""}`}
              onClick={() => setRightTab("queue")}
            >
              Queue
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
                const isProc = job.status === "processing";
                const isDone = job.status === "completed";
                const isFail =
                  typeof job.status === "object" && "error" in job.status;
                const isCancelled = job.status === "cancelled";

                const progress = isProc
                  ? (videoProgresses[job.jobId] ?? job.progress)
                  : job.progress;

                return (
                  <div
                    key={job.jobId}
                    className={`queue-item ${job.status} ${isProc ? "is-processing" : ""}`}
                  >
                    {job.thumbnailPath && (
                      <div className="queue-item-thumb">
                        <img
                          src={convertFileSrc(job.thumbnailPath)}
                          alt="thumb"
                        />
                      </div>
                    )}
                    <div className="queue-item-body">
                      <div className="queue-item-name">
                        {basename(job.filePath)}
                      </div>
                      <div className="queue-item-meta">
                        <span className="queue-item-ratio">
                          {RATIO_DISPLAY[job.ratio]}
                        </span>
                        <span
                          className={`queue-item-status-text ${isFail ? "text-error" : ""}`}
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
                      {(isProc || (progress > 0 && !isDone)) && (
                        <div className="queue-item-progress">
                          <div
                            className="queue-item-progress-fill"
                            style={{ width: `${progress}%` }}
                          />
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
