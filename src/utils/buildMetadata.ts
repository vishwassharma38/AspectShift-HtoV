export function formatBuildModeLabel(buildMode: string): string {
  const normalized = buildMode.trim();
  if (!normalized) return "Unknown";
  return normalized.charAt(0).toUpperCase() + normalized.slice(1).toLowerCase();
}
