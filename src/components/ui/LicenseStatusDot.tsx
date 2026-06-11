import { useAppShell } from "../../context/AppShellContext";

export function LicenseStatusDot() {
  const { licenseIndicatorState } = useAppShell();

  return (
    <span
      className="license-status-dot"
      data-status={licenseIndicatorState.cssStatus}
      title={licenseIndicatorState.tooltip}
      aria-label={licenseIndicatorState.tooltip}
      role="status"
    >
      <span className="license-status-dot-core" />
    </span>
  );
}
