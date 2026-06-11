import type { AuthStatus } from "../types/backend";

export type LicenseIndicatorColor = "green" | "yellow" | "red" | "gray";
export type LicenseIndicatorInput = AuthStatus | "loading" | "checking";

export type LicenseIndicatorState = {
  color: LicenseIndicatorColor;
  cssStatus: LicenseIndicatorColor;
  badgeText: string;
  tooltip: string;
  isAccessAllowed: boolean;
};

function assertNever(value: never): never {
  throw new Error(`Unhandled license status: ${value}`);
}

export function getLicenseIndicatorState(
  status: LicenseIndicatorInput | null | undefined,
): LicenseIndicatorState {
  if (!status) {
    return {
      color: "gray",
      cssStatus: "gray",
      badgeText: "Checking License",
      tooltip: "Checking License Status",
      isAccessAllowed: false,
    };
  }

  switch (status) {
    case "valid":
      return {
        color: "green",
        cssStatus: "green",
        badgeText: "License Active",
        tooltip: "License Valid",
        isAccessAllowed: true,
      };
    case "offline_valid":
      return {
        color: "green",
        cssStatus: "green",
        badgeText: "License Active",
        tooltip: "License Valid",
        isAccessAllowed: true,
      };
    case "grace_period":
      return {
        color: "yellow",
        cssStatus: "yellow",
        badgeText: "Grace Period Active",
        tooltip: "Grace Period Active",
        isAccessAllowed: true,
      };
    case "refresh_required":
      return {
        color: "yellow",
        cssStatus: "yellow",
        badgeText: "License Refresh Required",
        tooltip: "License Refresh Required",
        isAccessAllowed: true,
      };
    case "expired":
      return {
        color: "red",
        cssStatus: "red",
        badgeText: "License Expired",
        tooltip: "License Expired",
        isAccessAllowed: false,
      };
    case "invalid":
      return {
        color: "red",
        cssStatus: "red",
        badgeText: "License Invalid",
        tooltip: "License Invalid",
        isAccessAllowed: false,
      };
    case "corrupted":
      return {
        color: "red",
        cssStatus: "red",
        badgeText: "License Invalid",
        tooltip: "License Verification Failed",
        isAccessAllowed: false,
      };
    case "not_activated":
      return {
        color: "red",
        cssStatus: "red",
        badgeText: "License Missing",
        tooltip: "License Missing",
        isAccessAllowed: false,
      };
    case "machine_mismatch":
      return {
        color: "red",
        cssStatus: "red",
        badgeText: "Registration Invalid",
        tooltip: "License Verification Failed",
        isAccessAllowed: false,
      };
    case "recoverable_error":
      return {
        color: "red",
        cssStatus: "red",
        badgeText: "License Verification Failed",
        tooltip: "License Verification Failed",
        isAccessAllowed: false,
      };
    case "loading":
    case "checking":
    case "initializing":
    case "credentials_found":
    case "validating":
      return {
        color: "gray",
        cssStatus: "gray",
        badgeText: "Checking License",
        tooltip: "Checking License Status",
        isAccessAllowed: false,
      };
    case "activating":
      return {
        color: "gray",
        cssStatus: "gray",
        badgeText: "Validating License",
        tooltip: "Validating License",
        isAccessAllowed: false,
      };
    default:
      return assertNever(status);
  }
}
