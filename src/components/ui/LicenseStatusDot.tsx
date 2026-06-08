import { useAppShell } from "../../context/AppShellContext";

const TOOLTIPS = {
  active: "License is valid and active.",
  refresh_required:
    "License requires refresh. The application will automatically attempt to refresh the license when an internet connection is available.",
  unlicensed:
    "License is expired, revoked, refunded, invalid, or unavailable. The application is currently running in unregistered mode. Please re-enter a valid license key. If the issue persists, contact the developer.",
} as const;

export function LicenseStatusDot() {
  const { licenseIndicatorStatus } = useAppShell();

  return (
    <span
      className="license-status-dot"
      data-status={licenseIndicatorStatus}
      title={TOOLTIPS[licenseIndicatorStatus]}
      aria-label={TOOLTIPS[licenseIndicatorStatus]}
      role="status"
    >
      <span className="license-status-dot-core" />
    </span>
  );
}
