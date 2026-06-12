import type {
  AppDepsState,
  DependencyId,
  DependencyReport,
} from "../types/backend";

export type DependencyPromptMode =
  | "startup"
  | "subtitle_export"
  | "subtitle_burn";

export type ManagedDependencyId = Extract<
  DependencyId,
  "whisper_binary" | "whisper_model"
>;

export const MANAGED_DEPENDENCY_IDS: ManagedDependencyId[] = [
  "whisper_binary",
  "whisper_model",
];

export const SUBTITLE_CORE_DEPENDENCY_IDS = MANAGED_DEPENDENCY_IDS;

export function isManagedDependencyId(
  dependencyId: DependencyId,
): dependencyId is ManagedDependencyId {
  return (MANAGED_DEPENDENCY_IDS as DependencyId[]).includes(dependencyId);
}

export function getManagedDependencyReports(depsState: AppDepsState | null) {
  return MANAGED_DEPENDENCY_IDS.map(
    (dependencyId) => depsState?.dependencies[dependencyId] ?? null,
  ).filter((report): report is DependencyReport => !!report);
}

export function getDependencyStatusLabel(report?: DependencyReport | null) {
  if (!report) return "missing";
  return report.status.status;
}

export function isDependencyReady(report?: DependencyReport | null) {
  return report?.status.status === "ready";
}

export function getMissingDependencies(
  depsState: AppDepsState | null,
  dependencyIds: ManagedDependencyId[] = MANAGED_DEPENDENCY_IDS,
) {
  if (!depsState) return dependencyIds;
  return dependencyIds.filter(
    (dependencyId) => !isDependencyReady(depsState.dependencies[dependencyId]),
  );
}

export function getMissingDependencyReports(
  depsState: AppDepsState | null,
  dependencyIds: ManagedDependencyId[] = MANAGED_DEPENDENCY_IDS,
) {
  if (!depsState) return [];
  return dependencyIds
    .map((dependencyId) => depsState.dependencies[dependencyId] ?? null)
    .filter((report): report is DependencyReport => !!report)
    .filter((report) => !isDependencyReady(report));
}

export function hasRequiredDependencies(
  depsState: AppDepsState | null,
  dependencyIds: ManagedDependencyId[] = MANAGED_DEPENDENCY_IDS,
) {
  return getMissingDependencies(depsState, dependencyIds).length === 0;
}

export function getDependencyHealthSummary(depsState: AppDepsState | null) {
  const reports = getManagedDependencyReports(depsState);
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
        "This export path relies on the subtitle runtime modules. Download them once to unlock subtitle extraction and future subtitle workflows.",
      ctaLabel: "Download Dependencies",
    };
  }

  if (mode === "subtitle_burn") {
    return {
      eyebrow: "Subtitle Burn-In",
      title: "Subtitle burn-in needs the subtitle runtime modules",
      description:
        "Burning subtitles into video requires the desktop subtitle toolchain to be present. Download the missing modules to continue immediately.",
      ctaLabel: "Download Dependencies",
    };
  }

  return {
    eyebrow: "Dependency Setup",
    title: "Subtitle tools are not downloaded yet",
    description:
      "AspectShift can download the subtitle runtime modules in the background so subtitle export and burn-in are ready when you need them.",
    ctaLabel: "Download Now",
  };
}

export function formatDependencyProgressPercent(progress: number) {
  const clamped = normalizeDependencyProgress(progress);

  const truncated = Math.floor(clamped * 100) / 100;

  return truncated.toFixed(2);
}

export function normalizeDependencyProgress(progress: unknown) {
  const value =
    typeof progress === "number"
      ? progress
      : typeof progress === "string"
        ? Number(progress)
        : 0;

  if (!Number.isFinite(value)) return 0;
  return Math.max(0, Math.min(100, value));
}
