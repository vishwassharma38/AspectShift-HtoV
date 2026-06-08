import type {
  AppDepsState,
  DependencyId,
  DependencyReport,
} from "../types/backend";

export type DependencyPromptMode =
  | "startup"
  | "subtitle_export"
  | "subtitle_burn";

export const SUBTITLE_CORE_DEPENDENCY_IDS: DependencyId[] = [
  "whisper_binary",
  "whisper_model",
  "ffmpeg",
  "ffprobe",
];

export function getDependencyStatusLabel(report?: DependencyReport | null) {
  if (!report) return "missing";
  return report.status.status;
}

export function isDependencyReady(report?: DependencyReport | null) {
  return report?.status.status === "ready";
}

export function getMissingDependencies(
  depsState: AppDepsState | null,
  dependencyIds: DependencyId[] = SUBTITLE_CORE_DEPENDENCY_IDS,
) {
  if (!depsState) return dependencyIds;
  return dependencyIds.filter(
    (dependencyId) => !isDependencyReady(depsState.dependencies[dependencyId]),
  );
}

export function getMissingDependencyReports(
  depsState: AppDepsState | null,
  dependencyIds: DependencyId[] = SUBTITLE_CORE_DEPENDENCY_IDS,
) {
  if (!depsState) return [];
  return dependencyIds
    .map((dependencyId) => depsState.dependencies[dependencyId] ?? null)
    .filter((report): report is DependencyReport => !!report)
    .filter((report) => !isDependencyReady(report));
}

export function hasRequiredDependencies(
  depsState: AppDepsState | null,
  dependencyIds: DependencyId[] = SUBTITLE_CORE_DEPENDENCY_IDS,
) {
  return getMissingDependencies(depsState, dependencyIds).length === 0;
}

export function getDependencyHealthSummary(depsState: AppDepsState | null) {
  const reports = Object.values(depsState?.dependencies ?? {});
  const readyCount = reports.filter((report) => report.status.status === "ready");
  const issueCount = reports.filter((report) => report.status.status !== "ready");

  return {
    totalCount: reports.length,
    readyCount: readyCount.length,
    issueCount: issueCount.length,
    scanStatus: depsState?.scanStatus ?? "not_scanned",
    scanSource: depsState?.scanSource ?? null,
    healthStatus: depsState?.healthStatus ?? "unknown",
    lastUpdated: depsState?.lastUpdated ?? null,
    lastFullScanAt: depsState?.lastFullScanAt ?? null,
    lastManualScanAt: depsState?.lastManualScanAt ?? null,
    lastWeeklyScanAt: depsState?.lastWeeklyScanAt ?? null,
  };
}

export function getDependencyPromptCopy(mode: DependencyPromptMode) {
  if (mode === "subtitle_export") {
    return {
      eyebrow: "Subtitle Export",
      title: "Subtitle export needs external subtitle tools",
      description:
        "This export path relies on the subtitle runtime modules. Install them once to unlock subtitle extraction and future subtitle workflows.",
      ctaLabel: "Download and Install Instantly",
    };
  }

  if (mode === "subtitle_burn") {
    return {
      eyebrow: "Subtitle Burn-In",
      title: "Subtitle burn-in needs the subtitle runtime modules",
      description:
        "Burning subtitles into video requires the desktop subtitle toolchain to be present. Install the missing modules to continue immediately.",
      ctaLabel: "Download and Install Instantly",
    };
  }

  return {
    eyebrow: "Dependency Setup",
    title: "Subtitle tools are not installed yet",
    description:
      "AspectShift can install the subtitle runtime modules in the background so subtitle export and burn-in are ready when you need them.",
    ctaLabel: "Download Now",
  };
}
