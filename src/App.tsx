import { useState, useEffect, useMemo, useRef, useCallback } from "react";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import {
  getIdentifier,
  getName,
  getTauriVersion,
  getVersion,
} from "@tauri-apps/api/app";
import { open } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import {
  check,
  type DownloadEvent,
  type Update,
} from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import type {
  ActivationResult,
  AuthState,
  AppConfig,
  AppDepsState,
  AspectRatio,
  AspectRatioTarget,
  BatchProgress,
  CustomPreset,
  DependencyId,
  EncodingProfile,
  FileProgress,
  FileReadiness,
  LogoOptions,
  LogoPosition,
  OrientationInfo,
  OutputFormat,
  OutputJob,
  PlatformPreset,
  PreviewLayoutRequest,
  PreviewRenderLayout,
  SelectionMetadata,
  TargetType,
  UpdateEntitlementCheckResult,
  VideoEffectsSettings,
  VideoProgress,
  VideoTransform,
} from "./types/backend";
import "./App.css";
import { VideoCanvas } from "./components/VideoCanvas";
import { PresetsPanel, type DisplayPreset } from "./components/PresetsPanel";
import { Header } from "./components/layout/Header";
import { OnboardingModal } from "./components/modals/OnboardingModal";
import { DependencyModal } from "./components/modals/DependencyModal";
import { FirstRunOnboardingOverlay } from "./components/modals/FirstRunOnboardingOverlay";
import { ONBOARDING_FLOW_GAP_MS } from "./components/modals/onboardingMotion";
import { LicensePanelModal } from "./components/modals/LicensePanelModal";
import { AboutDialog } from "./components/modals/AboutDialog";
import {
  UpdateModal,
  type UpdateFlowStage,
} from "./components/modals/UpdateModal";
import { SettingsOverlay } from "./components/layout/SettingsOverlay";
import {
  AppShellProvider,
} from "./context/AppShellContext";
import { getLicenseIndicatorState } from "./utils/licenseIndicatorMapping";
import {
  getMissingDependencies,
  hasRequiredDependencies,
  formatDependencyProgressPercent,
  SUBTITLE_CORE_DEPENDENCY_IDS,
  type DependencyPromptMode,
} from "./services/dependencyManager";

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

interface DependencyInstallEvent {
  id: DependencyId;
  lifecycle:
    | "idle"
    | "checking"
    | "missing"
    | "downloading"
    | "verifying"
    | "extracting"
    | "installed"
    | "failed";
  progressPercent: number | null;
  message: string | null;
}

type UpdateNoticeTone = "success" | "warning" | "error" | "info";

interface UpdateFlowState {
  stage: UpdateFlowStage;
  tone: UpdateNoticeTone;
  message: string;
  currentVersion: string | null;
  latestVersion: string | null;
  releaseNotes: string | null;
  progressPercent: number | null;
  progressLabel: string | null;
  errorMessage: string | null;
}

interface AppNotification {
  id: number;
  tone: UpdateNoticeTone;
  message: string;
  progressPercent: number | null;
  progressLabel: string | null;
  persistent: boolean;
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

const NOTIFICATION_EXIT_MS = 320;
const NOTIFICATION_DURATIONS: Record<UpdateNoticeTone, number> = {
  info: 3600,
  success: 4600,
  warning: 6200,
  error: 0,
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
const ONBOARDING_STORAGE_KEY = "aspectshift.hasCompletedOnboarding";
const SKIPPED_DEPENDENCY_PROMPT_KEY = "aspectshift.skippedDependencyPrompt";
const FIRST_LAUNCH_DEPENDENCY_PROMPT_KEY =
  "aspectshift.firstLaunchDependencyPromptSeen";

type SubtitleIntent = "exportSubtitles" | "burnSubtitles";
type FirstRunPanel = "license" | "dependency";

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

function formatUpdateProgressBytes(value: number): string {
  if (!Number.isFinite(value) || value < 0) return "0 B";
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`;
  return `${(value / (1024 * 1024)).toFixed(1)} MB`;
}

function formatUpdateVersion(version: string | null | undefined): string {
  if (!version) return "n/a";
  return version.startsWith("v") ? version : `v${version}`;
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
        | "night") || "day"
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
  const [previewLayout, setPreviewLayout] =
    useState<PreviewRenderLayout | null>(null);
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
  const [depsState, setDepsState] = useState<AppDepsState | null>(null);
  const [depsStateLoaded, setDepsStateLoaded] = useState(false);
  const [depsInstalling, setDepsInstalling] = useState(false);
  const [depsInstallMessage, setDepsInstallMessage] = useState<string | null>(
    null,
  );
  const [depsProgressById, setDepsProgressById] = useState<
    Partial<Record<DependencyId, number>>
  >({});
  const [authState, setAuthState] = useState<AuthState | null>(null);
  const [licenseKeyInput, setLicenseKeyInput] = useState("");
  const [authErrorMessage, setAuthErrorMessage] = useState<string | null>(null);
  const [isActivatingLicense, setIsActivatingLicense] = useState(false);
  const [isCheckingUpdates, setIsCheckingUpdates] = useState(false);
  const [updateDialogOpen, setUpdateDialogOpen] = useState(false);
  const [activeNotification, setActiveNotification] =
    useState<AppNotification | null>(null);
  const [notificationQueue, setNotificationQueue] = useState<AppNotification[]>(
    [],
  );
  const [isNotificationExiting, setIsNotificationExiting] = useState(false);
  const [isUpdateBannerDismissed, setIsUpdateBannerDismissed] = useState(false);
  const [isUpdateBannerExiting, setIsUpdateBannerExiting] = useState(false);
  const [updateFlow, setUpdateFlow] = useState<UpdateFlowState>({
    stage: "idle",
    tone: "info",
    message: "",
    currentVersion: null,
    latestVersion: null,
    releaseNotes: null,
    progressPercent: null,
    progressLabel: null,
    errorMessage: null,
  });
  const [hasCompletedOnboarding, setHasCompletedOnboarding] = useState(() => {
    return localStorage.getItem(ONBOARDING_STORAGE_KEY) === "true";
  });
  const [onboardingSuccess, setOnboardingSuccess] = useState(false);
  const [firstRunPanel, setFirstRunPanel] = useState<FirstRunPanel | null>(
    () => (localStorage.getItem(ONBOARDING_STORAGE_KEY) === "true" ? null : "license"),
  );
  const [firstRunPanelOpen, setFirstRunPanelOpen] = useState(() => {
    return localStorage.getItem(ONBOARDING_STORAGE_KEY) !== "true";
  });
  const [firstRunOverlayOpen, setFirstRunOverlayOpen] = useState(() => {
    return localStorage.getItem(ONBOARDING_STORAGE_KEY) !== "true";
  });
  const [firstRunPendingNext, setFirstRunPendingNext] = useState<
    "dependency" | "finish" | null
  >(null);
  const [firstRunGapReady, setFirstRunGapReady] = useState(false);
  const [skippedDependencyPrompt] = useState(() => {
    return localStorage.getItem(SKIPPED_DEPENDENCY_PROMPT_KEY) === "true";
  });
  const [seenFirstLaunchDependencyPrompt, setSeenFirstLaunchDependencyPrompt] =
    useState(() => {
      return (
        localStorage.getItem(FIRST_LAUNCH_DEPENDENCY_PROMPT_KEY) === "true"
      );
    });
  const [dependencyModalOpen, setDependencyModalOpen] = useState(false);
  const [dependencyPromptMode, setDependencyPromptMode] =
    useState<DependencyPromptMode>("startup");
  const [pendingSubtitleIntent, setPendingSubtitleIntent] =
    useState<SubtitleIntent | null>(null);
  const dependencyPromptDismissedKeyRef = useRef<string | null>(null);
  const [settingsOverlayOpen, setSettingsOverlayOpen] = useState(false);
  const [licensePanelOpen, setLicensePanelOpen] = useState(false);
  const [aboutDialogOpen, setAboutDialogOpen] = useState(false);
  const [aboutMetadata, setAboutMetadata] = useState({
    appName: "AspectShift-HtoV",
    appVersion: "0.1.1",
    tauriVersion: "2",
    identifier: "com.softwarefromvish.aspectshift-htov",
    buildMode: import.meta.env.MODE,
  });
  const [volumeSliderActive, setVolumeSliderActive] = useState(false);
  const [volumeSliderInteracting, setVolumeSliderInteracting] = useState(false);
  const [volumeSliderHovering, setVolumeSliderHovering] = useState(false);
  const [volumeSliderFocusWithin, setVolumeSliderFocusWithin] = useState(false);
  const volumeCollapseTimer = useRef<ReturnType<typeof setTimeout> | null>(
    null,
  );
  const firstRunSuccessTimerRef = useRef<ReturnType<typeof setTimeout> | null>(
    null,
  );
  const firstRunGapTimerRef = useRef<ReturnType<typeof setTimeout> | null>(
    null,
  );
  const previewVolumeRef = useRef<HTMLDivElement | null>(null);
  const isAuthHydrating =
    !authState ||
    authState.status === "initializing" ||
    authState.status === "credentials_found" ||
    authState.status === "validating";
  const licenseIndicatorState = getLicenseIndicatorState(
    isAuthHydrating ? "loading" : authState?.status,
  );
  const isLicensed = licenseIndicatorState.isAccessAllowed;
  const missingSubtitleDependencies = getMissingDependencies(
    depsState,
    SUBTITLE_CORE_DEPENDENCY_IDS,
  );
  const missingSubtitleDependencyKey = missingSubtitleDependencies.join("|");
  const subtitleCoreReady = hasRequiredDependencies(
    depsState,
    SUBTITLE_CORE_DEPENDENCY_IDS,
  );
  const shouldShowFirstRunDependencyStep =
    !subtitleCoreReady &&
    !skippedDependencyPrompt &&
    !seenFirstLaunchDependencyPrompt &&
    !!depsState &&
    depsState.scanSource === "first_launch" &&
    depsState.scanStatus === "scan_completed" &&
    missingSubtitleDependencies.length > 0;
  const updateRef = useRef<Update | null>(null);
  const notificationIdRef = useRef(0);
  const notificationExitTimerRef = useRef<ReturnType<typeof setTimeout> | null>(
    null,
  );
  const updateBannerExitTimerRef = useRef<ReturnType<typeof setTimeout> | null>(
    null,
  );
  const isUpdateFlowBusy =
    updateFlow.stage === "checking_entitlement" ||
    updateFlow.stage === "checking_updater" ||
    updateFlow.stage === "downloading" ||
    updateFlow.stage === "installing";

  const closePendingUpdate = useCallback(async () => {
    const pendingUpdate = updateRef.current;
    updateRef.current = null;
    if (!pendingUpdate) return;
    try {
      await pendingUpdate.close();
    } catch {
      // The resource is best-effort cleanup only.
    }
  }, []);

  useEffect(() => {
    return () => {
      void closePendingUpdate();
    };
  }, [closePendingUpdate]);

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

  const handleAuthState = useCallback((state: AuthState) => {
    setAuthState(state);
    setAuthErrorMessage(null);
    setIsActivatingLicense(state.status === "activating");
  }, []);

  const clearFirstRunTimers = useCallback(() => {
    if (firstRunSuccessTimerRef.current !== null) {
      clearTimeout(firstRunSuccessTimerRef.current);
      firstRunSuccessTimerRef.current = null;
    }
    if (firstRunGapTimerRef.current !== null) {
      clearTimeout(firstRunGapTimerRef.current);
      firstRunGapTimerRef.current = null;
    }
  }, []);

  const beginFirstRunPanelClose = useCallback(
    (next: "dependency" | "finish") => {
      clearFirstRunTimers();
      setFirstRunPendingNext(next);
      setFirstRunGapReady(false);
      setFirstRunPanelOpen(false);
    },
    [clearFirstRunTimers],
  );

  const handleFirstRunPanelExited = useCallback(() => {
    if (firstRunPanel === "license") {
      setOnboardingSuccess(false);
    }
    setFirstRunPanel(null);
    setFirstRunGapReady(false);
    if (firstRunPendingNext === null) return;

    if (firstRunGapTimerRef.current !== null) {
      clearTimeout(firstRunGapTimerRef.current);
      firstRunGapTimerRef.current = null;
    }

    firstRunGapTimerRef.current = window.setTimeout(() => {
      setFirstRunGapReady(true);
    }, ONBOARDING_FLOW_GAP_MS);
  }, [firstRunPanel, firstRunPendingNext]);

  const finishFirstRunFlow = useCallback(() => {
    clearFirstRunTimers();
    setFirstRunPendingNext(null);
    setFirstRunGapReady(false);
    setFirstRunPanel(null);
    setFirstRunPanelOpen(false);
    setFirstRunOverlayOpen(false);
    setHasCompletedOnboarding(true);
    setOnboardingSuccess(false);
  }, [clearFirstRunTimers]);

  const handleActivateLicense = useCallback(async () => {
    if (!licenseKeyInput.trim() || isActivatingLicense) return;
    setIsActivatingLicense(true);
    setAuthErrorMessage(null);
    try {
      const result = await invoke<ActivationResult>("activate_license", {
        licenseKey: licenseKeyInput.trim(),
      });
      if (!result.success) {
        setAuthErrorMessage(
          result.message ?? "Activation failed. Please check your license key.",
        );
        setIsActivatingLicense(false);
        return;
      }
      handleAuthState(result.authState);
      setOnboardingSuccess(true);
      setLicenseKeyInput("");
      localStorage.setItem(ONBOARDING_STORAGE_KEY, "true");
      clearFirstRunTimers();
      const nextStep =
        !depsStateLoaded || shouldShowFirstRunDependencyStep
          ? "dependency"
          : "finish";
      firstRunSuccessTimerRef.current = window.setTimeout(() => {
        beginFirstRunPanelClose(nextStep);
      }, 650);
    } catch (error) {
      setAuthErrorMessage(errorMessage(error));
      setIsActivatingLicense(false);
    }
  }, [
    beginFirstRunPanelClose,
    clearFirstRunTimers,
    handleAuthState,
    isActivatingLicense,
    licenseKeyInput,
    shouldShowFirstRunDependencyStep,
  ]);

  const handleFirstRunBypass = useCallback(() => {
    localStorage.setItem(ONBOARDING_STORAGE_KEY, "true");
    const nextStep =
      !depsStateLoaded || shouldShowFirstRunDependencyStep
        ? "dependency"
        : "finish";
    setOnboardingSuccess(false);
    beginFirstRunPanelClose(nextStep);
  }, [beginFirstRunPanelClose, depsStateLoaded, shouldShowFirstRunDependencyStep]);

  const handleRefreshLicense = useCallback(async () => {
    try {
      const state = await invoke<AuthState>("refresh_license");
      handleAuthState(state);
    } catch (error) {
      setAuthErrorMessage(errorMessage(error));
    }
  }, [handleAuthState]);

  const handleClearLicense = useCallback(async () => {
    try {
      await invoke("clear_license");
      const state = await invoke<AuthState>("get_auth_state");
      handleAuthState(state);
      setLicenseKeyInput("");
    } catch (error) {
      setAuthErrorMessage(errorMessage(error));
    }
  }, [handleAuthState]);

  const addLog = useCallback((msg: string, type: LogEntry["type"] = "info") => {
    setLogs((prev) => {
      const next = [...prev, { time: formatTime(new Date()), msg, type }];
      return next.length > 200 ? next.slice(-200) : next;
    });
  }, []);

  const beginNotificationExit = useCallback(() => {
    if (!activeNotification || isNotificationExiting) return;
    setIsNotificationExiting(true);
    if (notificationExitTimerRef.current) {
      clearTimeout(notificationExitTimerRef.current);
    }
    notificationExitTimerRef.current = setTimeout(() => {
      setActiveNotification(null);
      setIsNotificationExiting(false);
      notificationExitTimerRef.current = null;
    }, NOTIFICATION_EXIT_MS);
  }, [activeNotification, isNotificationExiting]);

  useEffect(() => {
    if (activeNotification || notificationQueue.length === 0) return;
    setActiveNotification(notificationQueue[0]);
    setNotificationQueue((queue) => queue.slice(1));
  }, [activeNotification, notificationQueue]);

  const hasUpdateBanner =
    updateFlow.stage !== "idle" &&
    (!isUpdateBannerDismissed || isUpdateBannerExiting);

  useEffect(() => {
    if (hasUpdateBanner) return;
    if (!activeNotification || activeNotification.persistent) return;
    const duration = NOTIFICATION_DURATIONS[activeNotification.tone];
    if (duration <= 0) return;
    const timer = window.setTimeout(beginNotificationExit, duration);
    return () => window.clearTimeout(timer);
  }, [activeNotification, beginNotificationExit, hasUpdateBanner]);

  useEffect(() => {
    return () => {
      if (notificationExitTimerRef.current) {
        clearTimeout(notificationExitTimerRef.current);
      }
    };
  }, []);

  const showNotification = useCallback(
    (
      next: {
        tone: UpdateNoticeTone;
        message: string;
        progressPercent?: number | null;
        progressLabel?: string | null;
      },
      options?: { persistent?: boolean },
    ) => {
      const queuedNotification: AppNotification = {
        id: notificationIdRef.current + 1,
        tone: next.tone,
        message: next.message,
        progressPercent: next.progressPercent ?? null,
        progressLabel: next.progressLabel ?? null,
        persistent:
          options?.persistent ?? next.tone === "error",
      };
      notificationIdRef.current = queuedNotification.id;
      setNotificationQueue((queue) => [...queue, queuedNotification]);
    },
    [],
  );

  const clearNotification = useCallback(() => {
    if (activeNotification) {
      beginNotificationExit();
      return;
    }
    setNotificationQueue([]);
  }, [activeNotification, beginNotificationExit]);

  const updateBannerKey = `${updateFlow.stage}:${updateFlow.tone}:${updateFlow.message}:${updateFlow.progressLabel ?? ""}:${updateFlow.progressPercent ?? ""}`;
  const isUpdateBannerPersistent =
    updateFlow.tone === "error" ||
    updateFlow.stage === "downloading" ||
    updateFlow.stage === "installing";

  const dismissUpdateBanner = useCallback(() => {
    if (
      updateFlow.stage === "idle" ||
      isUpdateBannerDismissed ||
      isUpdateBannerExiting
    ) {
      return;
    }
    setIsUpdateBannerExiting(true);
    if (updateBannerExitTimerRef.current) {
      clearTimeout(updateBannerExitTimerRef.current);
    }
    updateBannerExitTimerRef.current = setTimeout(() => {
      setIsUpdateBannerDismissed(true);
      setIsUpdateBannerExiting(false);
      updateBannerExitTimerRef.current = null;
    }, NOTIFICATION_EXIT_MS);
  }, [isUpdateBannerDismissed, isUpdateBannerExiting, updateFlow.stage]);

  useEffect(() => {
    setIsUpdateBannerDismissed(false);
    setIsUpdateBannerExiting(false);
    if (updateBannerExitTimerRef.current) {
      clearTimeout(updateBannerExitTimerRef.current);
      updateBannerExitTimerRef.current = null;
    }
  }, [updateBannerKey]);

  useEffect(() => {
    if (updateFlow.stage === "idle" || isUpdateBannerDismissed) return;
    if (isUpdateBannerPersistent) return;
    const duration = NOTIFICATION_DURATIONS[updateFlow.tone];
    if (duration <= 0) return;
    const timer = window.setTimeout(dismissUpdateBanner, duration);
    return () => window.clearTimeout(timer);
  }, [
    dismissUpdateBanner,
    isUpdateBannerDismissed,
    isUpdateBannerPersistent,
    updateFlow.stage,
    updateFlow.tone,
  ]);

  useEffect(() => {
    return () => {
      if (updateBannerExitTimerRef.current) {
        clearTimeout(updateBannerExitTimerRef.current);
      }
    };
  }, []);

  const handleRefreshApp = useCallback(() => {
    window.location.reload();
  }, []);

  const handleInstallDependencies = useCallback(
    async (forceAll = false): Promise<boolean> => {
      const targetIds = forceAll
        ? SUBTITLE_CORE_DEPENDENCY_IDS
        : missingSubtitleDependencies;

      if (targetIds.length === 0) {
        setDepsInstallMessage("All subtitle dependencies are already ready.");
        return true;
      }

      try {
        setDepsInstalling(true);
        setDepsInstallMessage("Starting install...");
        for (const dependencyId of targetIds) {
          await invoke("install_dependency", { id: dependencyId });
        }
        const refreshed = await invoke<AppDepsState>("rescan_dependencies", {
          scan_source: "post_download",
        });
        setDepsState(refreshed);
        setDepsInstallMessage("Dependency installation complete.");
        return true;
      } catch (error) {
        setDepsInstallMessage(`Install failed: ${errorMessage(error)}`);
        return false;
      } finally {
        setDepsInstalling(false);
      }
    },
    [missingSubtitleDependencies],
  );

  useEffect(() => {
    return () => {
      clearFirstRunTimers();
    };
  }, [clearFirstRunTimers]);

  const handleFirstRunDependencyInstall = useCallback(async () => {
    const success = await handleInstallDependencies(false);
    if (!success) return;
    setFirstRunPanelOpen(false);
    setFirstRunPendingNext("finish");
    setFirstRunGapReady(false);
  }, [handleInstallDependencies]);

  const handleRescanDependencies = useCallback(() => {
    invoke<AppDepsState>("rescan_dependencies", { scan_source: "manual" })
      .then(setDepsState)
      .catch((error) => {
        setDepsInstallMessage(`Rescan failed: ${errorMessage(error)}`);
      });
  }, []);

  const openDependencyPrompt = useCallback(
    (
      mode: DependencyPromptMode,
      subtitleIntent: SubtitleIntent | null = null,
    ) => {
      if (mode === "startup") {
        localStorage.setItem(FIRST_LAUNCH_DEPENDENCY_PROMPT_KEY, "true");
        setSeenFirstLaunchDependencyPrompt(true);
      }
      dependencyPromptDismissedKeyRef.current = null;
      setDependencyPromptMode(mode);
      setPendingSubtitleIntent(subtitleIntent);
      setDependencyModalOpen(true);
    },
    [],
  );

  const closeDependencyPrompt = useCallback(() => {
    dependencyPromptDismissedKeyRef.current = missingSubtitleDependencyKey;
    setPendingSubtitleIntent(null);
    setDependencyModalOpen(false);
  }, [missingSubtitleDependencyKey]);

  const handleSubtitleFeatureToggle = useCallback(
    (intent: SubtitleIntent, enabled: boolean) => {
      if (
        enabled &&
        !subtitleCoreReady &&
        missingSubtitleDependencies.length > 0
      ) {
        openDependencyPrompt(
          intent === "exportSubtitles" ? "subtitle_export" : "subtitle_burn",
          intent,
        );
        return;
      }
      setEffectsState((current) => ({ ...current, [intent]: enabled }));
    },
    [
      missingSubtitleDependencies.length,
      openDependencyPrompt,
      subtitleCoreReady,
    ],
  );

  const handleCheckForUpdates = useCallback(async () => {
    if (isAuthHydrating || !isLicensed) return;

    if (
      updateFlow.stage === "update_available" ||
      updateFlow.stage === "installed_restart_required"
    ) {
      setUpdateDialogOpen(true);
      return;
    }

    if (isCheckingUpdates || isUpdateFlowBusy) return;

    await closePendingUpdate();

    const currentVersion = aboutMetadata.appVersion;
    setIsCheckingUpdates(true);
    setUpdateDialogOpen(false);
    setUpdateFlow({
      stage: "checking_entitlement",
      tone: "info",
      message: "Checking for updates...",
      currentVersion,
      latestVersion: null,
      releaseNotes: null,
      progressPercent: null,
      progressLabel: "Checking for updates...",
      errorMessage: null,
    });
    addLog("[Updater] Entitlement check started", "info");

    try {
      const entitlement = await invoke<UpdateEntitlementCheckResult>(
        "check_update_entitlement",
      );

      addLog(
        `Update entitlement response: status=${entitlement.status}${
          entitlement.data
            ? `, latestVersion=${entitlement.data.latestVersion}`
            : ""
        }`,
        entitlement.status === "update_available" ? "success" : "info",
      );

      if (entitlement.status === "no_update") {
        setUpdateFlow({
          stage: "already_latest",
          tone: "success",
          message: "Already on the latest version.",
          currentVersion,
          latestVersion: currentVersion,
          releaseNotes: null,
          progressPercent: null,
          progressLabel: null,
          errorMessage: null,
        });
        addLog("[Updater] Already on the latest version", "info");
        return;
      }

      if (entitlement.status === "not_entitled") {
        setUpdateFlow({
          stage: "entitlement_denied",
          tone: "warning",
          message: "Update entitlement denied.",
          currentVersion,
          latestVersion: entitlement.data?.latestVersion ?? null,
          releaseNotes: null,
          progressPercent: null,
          progressLabel: null,
          errorMessage: null,
        });
        addLog("[Updater] Update entitlement denied", "warn");
        return;
      }

      if (entitlement.status === "channel_not_allowed") {
        setUpdateFlow({
          stage: "entitlement_denied",
          tone: "warning",
          message: "Update entitlement denied for this release channel.",
          currentVersion,
          latestVersion: entitlement.data?.latestVersion ?? null,
          releaseNotes: null,
          progressPercent: null,
          progressLabel: null,
          errorMessage: null,
        });
        addLog(
          "[Updater] Update entitlement denied for this release channel",
          "warn",
        );
        return;
      }

      if (entitlement.status === "auth_required") {
        setUpdateFlow({
          stage: "entitlement_denied",
          tone: "error",
          message: "Please sign in again before checking for updates.",
          currentVersion,
          latestVersion: entitlement.data?.latestVersion ?? null,
          releaseNotes: null,
          progressPercent: null,
          progressLabel: null,
          errorMessage: null,
        });
        addLog(
          "[Updater] Update entitlement requires re-authentication",
          "warn",
        );
        return;
      }

      if (entitlement.status === "offline") {
        setUpdateFlow({
          stage: "failed",
          tone: "warning",
          message: "Offline: unable to verify update entitlement.",
          currentVersion,
          latestVersion: entitlement.data?.latestVersion ?? null,
          releaseNotes: null,
          progressPercent: null,
          progressLabel: null,
          errorMessage: "Offline: unable to verify update entitlement.",
        });
        addLog("[Updater] Update entitlement check failed: offline", "warn");
        return;
      }

      if (entitlement.status === "server_error") {
        setUpdateFlow({
          stage: "failed",
          tone: "error",
          message: "Update check failed. Please try again later.",
          currentVersion,
          latestVersion: entitlement.data?.latestVersion ?? null,
          releaseNotes: null,
          progressPercent: null,
          progressLabel: null,
          errorMessage: "Update check failed. Please try again later.",
        });
        addLog(
          "[Updater] Update entitlement check failed: server error",
          "error",
        );
        return;
      }

      if (entitlement.status !== "update_available") {
        setUpdateFlow({
          stage: "failed",
          tone: "error",
          message: "Update check failed.",
          currentVersion,
          latestVersion: entitlement.data?.latestVersion ?? null,
          releaseNotes: null,
          progressPercent: null,
          progressLabel: null,
          errorMessage: "Update check failed.",
        });
        addLog(
          `[Updater] Unexpected entitlement status: ${entitlement.status}`,
          "error",
        );
        return;
      }

      addLog(
        "[Updater] Entitlement approved; checking updater manifest",
        "success",
      );
      setUpdateFlow({
        stage: "checking_updater",
        tone: "info",
        message: "Checking for updates...",
        currentVersion,
        latestVersion: entitlement.data?.latestVersion ?? null,
        releaseNotes: null,
        progressPercent: null,
        progressLabel: "Checking for updates...",
        errorMessage: null,
      });

      const availableUpdate = await check();
      if (!availableUpdate) {
        setUpdateFlow({
          stage: "already_latest",
          tone: "success",
          message: "Already on the latest version.",
          currentVersion,
          latestVersion: currentVersion,
          releaseNotes: null,
          progressPercent: null,
          progressLabel: null,
          errorMessage: null,
        });
        addLog("[Updater] Updater reported no available update", "info");
        return;
      }

      updateRef.current = availableUpdate;
      const updateVersion =
        availableUpdate.version ?? entitlement.data?.latestVersion ?? null;
      const updateCurrentVersion =
        availableUpdate.currentVersion ?? currentVersion ?? null;

      setUpdateFlow({
        stage: "update_available",
        tone: "success",
        message: `Update available: ${formatUpdateVersion(updateVersion)}.`,
        currentVersion: updateCurrentVersion,
        latestVersion: updateVersion,
        releaseNotes: availableUpdate.body ?? null,
        progressPercent: null,
        progressLabel: null,
        errorMessage: null,
      });
      setUpdateDialogOpen(true);
      addLog(
        `[Updater] Update detected: current=${updateCurrentVersion ?? "unknown"}, latest=${updateVersion ?? "unknown"}`,
        "success",
      );
    } catch (error) {
      await closePendingUpdate();
      setUpdateFlow({
        stage: "failed",
        tone: "error",
        message: "Updater check failed.",
        currentVersion,
        latestVersion: null,
        releaseNotes: null,
        progressPercent: null,
        progressLabel: null,
        errorMessage: errorMessage(error),
      });
      addLog(
        `[Updater] Update flow failed during check: ${errorMessage(error)}`,
        "error",
      );
    } finally {
      setIsCheckingUpdates(false);
    }
  }, [
    aboutMetadata.appVersion,
    addLog,
    closePendingUpdate,
    isCheckingUpdates,
    isAuthHydrating,
    isLicensed,
    isUpdateFlowBusy,
    updateFlow.stage,
  ]);

  const handleDownloadAndInstallUpdate = useCallback(async () => {
    const pendingUpdate = updateRef.current;
    if (!pendingUpdate || updateFlow.stage !== "update_available") return;

    setUpdateDialogOpen(true);
    setUpdateFlow((current) => ({
      ...current,
      stage: "downloading",
      tone: "info",
      message: "Downloading update...",
      progressPercent: 0,
      progressLabel: "Downloading update...",
      errorMessage: null,
    }));

    let downloadedBytes = 0;
    let totalBytes: number | null = null;

    const applyDownloadProgress = (event: DownloadEvent) => {
      if (event.event === "Started") {
        totalBytes = event.data.contentLength ?? null;
        downloadedBytes = 0;
        setUpdateFlow((current) => ({
          ...current,
          stage: "downloading",
          tone: "info",
          message: "Downloading update...",
          progressPercent: 0,
          progressLabel:
            totalBytes !== null
              ? `Downloading 0 of ${formatUpdateProgressBytes(totalBytes)}`
              : "Downloading update...",
          errorMessage: null,
        }));
        return;
      }

      if (event.event === "Progress") {
        downloadedBytes += event.data.chunkLength;
        const progressPercent =
          totalBytes && totalBytes > 0
            ? Math.min(99, Math.round((downloadedBytes / totalBytes) * 100))
            : null;
        setUpdateFlow((current) => ({
          ...current,
          stage: "downloading",
          tone: "info",
          message: "Downloading update...",
          progressPercent,
          progressLabel:
            totalBytes !== null
              ? `Downloading ${formatUpdateProgressBytes(downloadedBytes)} of ${formatUpdateProgressBytes(totalBytes)}`
              : `Downloading ${formatUpdateProgressBytes(downloadedBytes)}`,
          errorMessage: null,
        }));
        return;
      }

      setUpdateFlow((current) => ({
        ...current,
        stage: "installing",
        tone: "info",
        message: "Installing update...",
        progressPercent: 100,
        progressLabel: "Installing update...",
        errorMessage: null,
      }));
    };

    try {
      addLog(
        `[Updater] Download started for version ${pendingUpdate.version}`,
        "info",
      );
      await pendingUpdate.download(applyDownloadProgress);

      setUpdateFlow((current) => ({
        ...current,
        stage: "installing",
        tone: "info",
        message: "Installing update...",
        progressPercent: 100,
        progressLabel: "Installing update...",
        errorMessage: null,
      }));

      addLog("[Updater] Download complete; installing update", "info");
      await pendingUpdate.install();
      await closePendingUpdate();

      const installedVersion =
        pendingUpdate.version ?? updateFlow.latestVersion;
      setUpdateFlow({
        stage: "installed_restart_required",
        tone: "success",
        message: `Update ${formatUpdateVersion(installedVersion)} installed. Restart required.`,
        currentVersion:
          pendingUpdate.currentVersion ?? aboutMetadata.appVersion,
        latestVersion: installedVersion ?? null,
        releaseNotes: pendingUpdate.body ?? null,
        progressPercent: 100,
        progressLabel: "Installed successfully",
        errorMessage: null,
      });
      setUpdateDialogOpen(true);
      addLog(
        "[Updater] Update installed successfully; restart required",
        "success",
      );
    } catch (error) {
      await closePendingUpdate();
      setUpdateFlow({
        stage: "failed",
        tone: "error",
        message: "Update installation failed.",
        currentVersion:
          pendingUpdate.currentVersion ?? aboutMetadata.appVersion,
        latestVersion:
          pendingUpdate.version ?? updateFlow.latestVersion ?? null,
        releaseNotes: pendingUpdate.body ?? null,
        progressPercent: null,
        progressLabel: null,
        errorMessage: errorMessage(error),
      });
      setUpdateDialogOpen(true);
      addLog(
        `[Updater] Update installation failed: ${errorMessage(error)}`,
        "error",
      );
    }
  }, [
    aboutMetadata.appVersion,
    addLog,
    closePendingUpdate,
    updateFlow.latestVersion,
    updateFlow.stage,
  ]);

  const handleRestartNow = useCallback(async () => {
    if (updateFlow.stage !== "installed_restart_required") return;
    try {
      addLog("[Updater] Restart requested by user", "info");
      await relaunch();
    } catch (error) {
      setUpdateFlow((current) => ({
        ...current,
        stage: "failed",
        tone: "error",
        message: "Restart failed. Please restart the app manually.",
        errorMessage: errorMessage(error),
      }));
      setUpdateDialogOpen(true);
      addLog(`[Updater] Restart failed: ${errorMessage(error)}`, "error");
    }
  }, [addLog, updateFlow.stage]);

  const handleDismissUpdateDialog = useCallback(() => {
    setUpdateDialogOpen(false);
  }, []);

  useEffect(() => {
    Promise.all([getName(), getVersion(), getTauriVersion(), getIdentifier()])
      .then(([appName, appVersion, tauriVersion, identifier]) => {
        setAboutMetadata({
          appName,
          appVersion,
          tauriVersion,
          identifier,
          buildMode: import.meta.env.MODE,
        });
      })
      .catch(() => {});
  }, []);

  useEffect(() => {
    const setup = async () => {
      try {
        const state = await invoke<AuthState>("get_auth_state");
        handleAuthState(state);
      } catch (error) {
        setAuthErrorMessage(errorMessage(error));
      }
    };

    const unsubs: Array<() => void> = [];
    let disposed = false;

    setup();

    const bind = async () => {
      const channels = [
        "auth://status-changed",
        "auth://activation-success",
        "auth://refresh-required",
        "auth://license-invalid",
      ] as const;

      for (const channel of channels) {
        const unsub = await listen<{ authState: AuthState }>(
          channel,
          (event) => {
            if (!disposed) handleAuthState(event.payload.authState);
          },
        );
        if (disposed) unsub();
        else unsubs.push(unsub);
      }

      const failureUnsub = await listen<{ reason: string }>(
        "auth://activation-failed",
        (event) => {
          if (disposed) return;
          setIsActivatingLicense(false);
          setOnboardingSuccess(false);
          setAuthErrorMessage(event.payload.reason);
        },
      );
      if (disposed) failureUnsub();
      else unsubs.push(failureUnsub);
    };

    bind();

    return () => {
      disposed = true;
      unsubs.forEach((unsub) => unsub());
    };
  }, [handleAuthState]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.ctrlKey && event.key.toLowerCase() === "r") {
        event.preventDefault();
        handleRefreshApp();
      }
    };

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [handleRefreshApp]);

  useEffect(() => {
    if (!firstRunGapReady || !firstRunPendingNext) return;

    if (firstRunPendingNext === "dependency") {
      if (!depsStateLoaded) return;
      if (!shouldShowFirstRunDependencyStep) {
        setFirstRunPendingNext("finish");
        return;
      }

      localStorage.setItem(FIRST_LAUNCH_DEPENDENCY_PROMPT_KEY, "true");
      setSeenFirstLaunchDependencyPrompt(true);
      setFirstRunPanel("dependency");
      setFirstRunPanelOpen(true);
      setFirstRunPendingNext(null);
      setFirstRunGapReady(false);
      return;
    }

    setFirstRunOverlayOpen(false);
    setFirstRunPendingNext(null);
    setFirstRunGapReady(false);
  }, [
    depsStateLoaded,
    firstRunGapReady,
    firstRunPendingNext,
    shouldShowFirstRunDependencyStep,
  ]);

  useEffect(() => {
    if (!depsStateLoaded) return;
    if (!hasCompletedOnboarding) return;
    if (subtitleCoreReady) return;
    if (skippedDependencyPrompt) return;
    if (seenFirstLaunchDependencyPrompt) return;
    if (!depsState) return;
    if (depsState.scanSource !== "first_launch") return;
    if (depsState.scanStatus !== "scan_completed") return;
    if (dependencyModalOpen) return;
    if (
      dependencyPromptDismissedKeyRef.current === missingSubtitleDependencyKey
    )
      return;
    openDependencyPrompt("startup");
  }, [
    depsState,
    depsStateLoaded,
    dependencyModalOpen,
    hasCompletedOnboarding,
    openDependencyPrompt,
    missingSubtitleDependencyKey,
    skippedDependencyPrompt,
    seenFirstLaunchDependencyPrompt,
    subtitleCoreReady,
  ]);

  useEffect(() => {
    if (subtitleCoreReady) {
      dependencyPromptDismissedKeyRef.current = null;
    }
    if (!subtitleCoreReady || !pendingSubtitleIntent) return;
    setEffectsState((current) => ({
      ...current,
      [pendingSubtitleIntent]: true,
    }));
    setPendingSubtitleIntent(null);
    setDependencyModalOpen(false);
  }, [pendingSubtitleIntent, subtitleCoreReady]);

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
  const previewLayoutRequest = useMemo<PreviewLayoutRequest | null>(() => {
    if (!orientation) return null;
    const sourceAspectRatio =
      orientation.displayWidth / orientation.displayHeight;

    if (!activeSelection) {
      return {
        ratio: "ratio16x9",
        targetAspectRatio: sourceAspectRatio,
        effects: effectsState,
        platformConfig: null,
      };
    }
    if (activeSelection.type === "aspectRatio") {
      return {
        ratio: activeSelection.id,
        targetAspectRatio: null,
        effects: effectsState,
        platformConfig: null,
      };
    }
    const p = platformPresets.find((x) => x.id === activeSelection.id);
    if (p) {
      return {
        ratio: p.ratio,
        targetAspectRatio: null,
        effects: effectsState,
        platformConfig: p.platformConfig ?? null,
      };
    }
    const cp = customPresets.find((x) => x.id === activeSelection.id);
    if (cp) {
      return {
        ratio: cp.ratio,
        targetAspectRatio: null,
        effects: effectsState,
        platformConfig: null,
      };
    }
    return null;
  }, [
    activeSelection,
    platformPresets,
    customPresets,
    effectsState,
    orientation,
  ]);

  useEffect(() => {
    if (!previewLayoutRequest || !orientation) {
      setPreviewLayout(null);
      return;
    }
    invoke<PreviewRenderLayout>("compute_preview_layout", {
      request: previewLayoutRequest,
      orientation,
    })
      .then(setPreviewLayout)
      .catch(() => setPreviewLayout(null));
  }, [orientation, previewLayoutRequest]);

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
    invoke<AppDepsState>("get_dependency_state")
      .then((state) => {
        setDepsState(state);
      })
      .catch(() => {})
      .finally(() => {
        setDepsStateLoaded(true);
      });
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

      // Register asset protocol scope for all previously saved paths
      const savedPaths = [
        config.lastInputDir,
        config.lastOutputDir,
        config.logoPath,
      ].filter((p): p is string => typeof p === "string" && p.length > 0);

      for (const p of savedPaths) {
        await invoke("allow_path_scope", { path: p }).catch(() => {});
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
        addLog(`Failed to save config: ${errorMessage(e)}`, "error");
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
      clearNotification();
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

      const u4 = await listen<AppDepsState>("deps://state", (e) => {
        setDepsState(e.payload);
      });
      if (disposed) u4();
      else unsubscribers.push(u4);

      const u5 = await listen<DependencyInstallEvent>(
        "deps://install-progress",
        (e) => {
          const lifecycle = e.payload.lifecycle;
          if (lifecycle === "downloading") {
            setDepsInstalling(true);
            const progress = e.payload.progressPercent ?? 0;
            setDepsProgressById((prev) => ({
              ...prev,
              [e.payload.id]: progress,
            }));
            setDepsInstallMessage(
              `Downloading ${e.payload.id}... ${formatDependencyProgressPercent(progress)}%`,
            );
          } else if (lifecycle === "verifying") {
            setDepsInstallMessage(`Verifying ${e.payload.id}...`);
          } else if (lifecycle === "extracting") {
            setDepsInstallMessage(`Extracting ${e.payload.id}...`);
          } else if (lifecycle === "installed") {
            setDepsInstallMessage("Ready");
            setDepsInstalling(false);
          } else if (lifecycle === "failed") {
            setDepsInstallMessage(
              e.payload.message ?? "Dependency install failed",
            );
            setDepsInstalling(false);
          }
        },
      );
      if (disposed) u5();
      else unsubscribers.push(u5);
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

    // Reset orientation immediately so preview layout is recomputed only
    // after the real source geometry arrives.
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
        await invoke("allow_path_scope", { path: files[0] }).catch(() => {});
        addLog(`File selected: ${basename(files[0])}`, "info");
      } else {
        setInputFile(files[0]);
        setPreviewFile(files[0]);
        setBatchFiles(files);
        setFolderPreviewFiles([]);
        for (const f of files) {
          await invoke("allow_path_scope", { path: f }).catch(() => {});
        }
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
        await invoke("allow_path_scope", { path: sel }).catch(() => {});
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
          addLog(`Folder scan failed: ${errorMessage(scanErr)}`, "error");
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
        clearNotification();
        await invoke("allow_path_scope", { path: sel }).catch(() => {});
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
    async (e: React.DragEvent) => {
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
        await invoke("allow_path_scope", { path: files[0] }).catch(() => {});
      } else {
        setInputFile(files[0]);
        setPreviewFile(files[0]);
        setBatchFiles(files);
        setFolderPreviewFiles([]);
        for (const f of files) {
          await invoke("allow_path_scope", { path: f }).catch(() => {});
        }
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
        await invoke("allow_path_scope", { path: sel }).catch(() => {});
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
    const files =
      batchFiles.length > 0 ? batchFiles : inputFile ? [inputFile] : [];
    if (files.length === 0) {
      showNotification({
        message: "Please select at least one video.",
        tone: "warning",
      });
      addLog("Please select at least one video", "warn");
      return;
    }
    if (!outputDir) {
      showNotification({
        message: "Please select an output destination.",
        tone: "warning",
      });
      addLog("Please select an output destination", "warn");
      return;
    }
    if (selectedRatios.length === 0 && selectedPresetIds.length === 0) {
      showNotification({
        message: "Select at least one preset or ratio.",
        tone: "warning",
      });
      addLog("Select at least one preset or ratio", "warn");
      return;
    }
    if (!subtitleCoreReady && effectsState.exportSubtitles) {
      openDependencyPrompt("subtitle_export", "exportSubtitles");
      showNotification({
        message: "Subtitle export is waiting on dependency installation.",
        tone: "warning",
      });
      addLog("Subtitle export is waiting on dependency installation.", "warn");
      return;
    }
    if (!subtitleCoreReady && effectsState.burnSubtitles) {
      openDependencyPrompt("subtitle_burn", "burnSubtitles");
      showNotification({
        message: "Subtitle burn-in is waiting on dependency installation.",
        tone: "warning",
      });
      addLog("Subtitle burn-in is waiting on dependency installation.", "warn");
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
      showNotification({
        message: `Batch start failed: ${errorMessage(e)}`,
        tone: "error",
      });
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

  const headerStatusBadge =
    batchProgress &&
    batchProgress.percentage >= 100 &&
    batchProgress.status === "completed"
      ? { tone: "success" as const, label: "Complete" }
      : batchProgress && batchProgress.status === "failed"
        ? { tone: "error" as const, label: "Errors" }
        : null;
  const updateBanner =
    hasUpdateBanner
      ? {
          source: "update" as const,
          tone: updateFlow.tone,
          message: updateFlow.progressLabel ?? updateFlow.message,
          progressPercent: updateFlow.progressPercent,
          progressStage:
            updateFlow.stage === "downloading" ||
            updateFlow.stage === "installing",
          exiting: isUpdateBannerExiting,
        }
      : null;
  const activeBanner =
    updateBanner ??
    (activeNotification
        ? {
            source: "notification" as const,
            tone: activeNotification.tone,
            message:
              activeNotification.progressLabel ?? activeNotification.message,
            progressPercent: activeNotification.progressPercent,
            progressStage: activeNotification.progressPercent !== null,
            exiting: isNotificationExiting,
          }
        : null);

  return (
    <AppShellProvider
      value={{
        authState,
        isLicensed,
        licenseIndicatorState,
        depsState,
        subtitleCoreReady,
        missingSubtitleDependencies,
      }}
    >
      <div
        className="app-shell"
        data-auth-status={authState?.status ?? "initializing"}
      >
        <Header
          theme={theme}
          onToggleTheme={() =>
            setTheme((current) => (current === "day" ? "night" : "day"))
          }
          onOpenSettings={() => setSettingsOverlayOpen(true)}
          onOpenLicensePanel={() => setLicensePanelOpen(true)}
          onOpenAbout={() => setAboutDialogOpen(true)}
          onRefresh={handleRefreshApp}
          onCheckForUpdates={handleCheckForUpdates}
          isCheckingUpdates={isCheckingUpdates}
          isLicensed={isLicensed}
          statusBadge={headerStatusBadge}
        />
        {activeBanner && (
          <div
            className={`update-notice update-notice-${activeBanner.tone}${
              activeBanner.exiting ? " update-notice--exiting" : ""
            }`}
          >
            <span className="update-notice-dot" aria-hidden="true" />
            <span className="update-notice-text">
              {activeBanner.message}
              {activeBanner.progressPercent !== null &&
              activeBanner.progressStage
                ? ` (${activeBanner.progressPercent}%)`
                : ""}
            </span>
            <button
              type="button"
              className="update-notice-dismiss"
              onClick={
                activeBanner.source === "update"
                  ? dismissUpdateBanner
                  : clearNotification
              }
              aria-label="Dismiss notification"
            >
              x
            </button>
          </div>
        )}
        <UpdateModal
          open={updateDialogOpen}
          stage={updateFlow.stage}
          currentVersion={updateFlow.currentVersion}
          latestVersion={updateFlow.latestVersion}
          releaseNotes={updateFlow.releaseNotes}
          progressPercent={updateFlow.progressPercent}
          progressLabel={updateFlow.progressLabel}
          errorMessage={updateFlow.errorMessage}
          onDownloadAndInstall={handleDownloadAndInstallUpdate}
          onRestartNow={handleRestartNow}
          onLater={handleDismissUpdateDialog}
        />

        <div
          className={`app-content-stage${settingsOverlayOpen ? " view-transitioning" : ""}`}
        >
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
                            handleSubtitleFeatureToggle("exportSubtitles", v)
                          }
                        />
                      </div>
                      <div className="toggle-row">
                        <span className="toggle-label">Burn Subtitles</span>
                        <Toggle
                          checked={!!effectsState.burnSubtitles}
                          onChange={(v) =>
                            handleSubtitleFeatureToggle("burnSubtitles", v)
                          }
                        />
                      </div>
                    </div>

                    <div className="settings-group">
                      <div className="settings-group-title">Skip Existing</div>
                      <div className="toggle-row">
                        <span className="toggle-label">
                          Skip if output exists
                          <span className="label-desc">
                            Saves time on re-runs
                          </span>
                        </span>
                        <Toggle
                          checked={!!effectsState.skipExisting}
                          onChange={(v) =>
                            setEffectsState({
                              ...effectsState,
                              skipExisting: v,
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
                                      effectsState.logo?.position ??
                                      "bottom_right",
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
                  previewLayout={previewLayout}
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
                      onChange={(e) =>
                        handleVolumeChange(Number(e.target.value))
                      }
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
                      setPreviewVolume((v) =>
                        v > 0 ? 0 : DEFAULT_PREVIEW_VOLUME,
                      );
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
                    (selectedRatios.length === 0 &&
                      selectedPresetIds.length === 0)
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
                        ? `Start Batch (${jobCount})`
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
                  marginTop: batchProgress
                    ? "var(--space-md)"
                    : "var(--space-lg)",
                }}
              >
                <button
                  className={`tab${rightTab === "queue" ? " active" : ""}`}
                  onClick={() => setRightTab("queue")}
                >
                  Queue
                  {batchProgress && batchProgress.totalJobs > 0 && (
                    <span
                      className="badge badge-muted"
                      style={{ marginLeft: 6 }}
                    >
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

          <SettingsOverlay
            open={settingsOverlayOpen}
            depsState={depsState}
            depsInstalling={depsInstalling}
            depsInstallMessage={depsInstallMessage}
            aboutMetadata={aboutMetadata}
            onClose={() => setSettingsOverlayOpen(false)}
            onInstallMissing={() => handleInstallDependencies(false)}
            onForceReinstall={() => handleInstallDependencies(true)}
            onRescan={handleRescanDependencies}
            missingSubtitleDependencies={missingSubtitleDependencies}
          />
        </div>

        <FirstRunOnboardingOverlay
          open={firstRunOverlayOpen}
          onExited={finishFirstRunFlow}
        >
          {firstRunPanel === "license" && (
            <OnboardingModal
              embedded
              open={firstRunPanelOpen}
              licenseKey={licenseKeyInput}
              onLicenseKeyChange={setLicenseKeyInput}
              onVerify={handleActivateLicense}
              onBypass={handleFirstRunBypass}
              isVerifying={isActivatingLicense}
              verificationError={authErrorMessage}
              verificationSuccess={onboardingSuccess}
              onExited={handleFirstRunPanelExited}
            />
          )}

          {firstRunPanel === "dependency" && (
            <DependencyModal
              embedded
              open={firstRunPanelOpen}
              mode={dependencyPromptMode}
              missingDependencies={missingSubtitleDependencies}
              progressById={depsProgressById}
              isInstalling={depsInstalling}
              onInstall={handleFirstRunDependencyInstall}
              onClose={() => {
                setFirstRunPanelOpen(false);
                setFirstRunPendingNext("finish");
                setFirstRunGapReady(false);
              }}
              onExited={handleFirstRunPanelExited}
            />
          )}
        </FirstRunOnboardingOverlay>

        <DependencyModal
          open={dependencyModalOpen}
          mode={dependencyPromptMode}
          missingDependencies={missingSubtitleDependencies}
          progressById={depsProgressById}
          isInstalling={depsInstalling}
          onInstall={() => {
            void handleInstallDependencies(false);
          }}
          onClose={closeDependencyPrompt}
        />

        <LicensePanelModal
          open={licensePanelOpen}
          authState={authState}
          licenseKey={licenseKeyInput}
          errorMessage={authErrorMessage}
          isActivating={isActivatingLicense}
          onLicenseKeyChange={setLicenseKeyInput}
          onActivate={handleActivateLicense}
          onRefresh={handleRefreshLicense}
          onClear={handleClearLicense}
          onClose={() => setLicensePanelOpen(false)}
        />

        <AboutDialog
          open={aboutDialogOpen}
          onClose={() => setAboutDialogOpen(false)}
          metadata={aboutMetadata}
        />
      </div>
    </AppShellProvider>
  );
}
