import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  ActivationResult,
  AuthState,
  UpdateEntitlementCheckResult,
} from "../types/backend";
import { getLicenseIndicatorState } from "../utils/licenseIndicatorMapping";

interface AuthStatusChangedPayload {
  authState: AuthState;
}

interface AuthActivationFailedPayload {
  reason: string;
  errorCode: string;
}

interface Props {
  onAuthStateChange: (state: AuthState) => void;
}

export function AuthPanel({ onAuthStateChange }: Props) {
  const [authState, setAuthState] = useState<AuthState | null>(null);
  const [licenseKey, setLicenseKey] = useState("");
  const [isActivating, setIsActivating] = useState(false);
  const [isCheckingUpdates, setIsCheckingUpdates] = useState(false);
  const [updateCheckResult, setUpdateCheckResult] =
    useState<UpdateEntitlementCheckResult | null>(null);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  const handleAuthState = useCallback(
    (state: AuthState) => {
      setAuthState(state);
      setUpdateCheckResult(null);
      onAuthStateChange(state);
      if (state.status === "activating") {
        setIsActivating(true);
      } else {
        setIsActivating(false);
      }
    },
    [onAuthStateChange],
  );

  useEffect(() => {
    const unsubs: Array<() => void> = [];
    let disposed = false;

    const setup = async () => {
      try {
        const state = await invoke<AuthState>("get_auth_state");
        if (!disposed) handleAuthState(state);
      } catch (e) {
        console.error("Failed to load auth state:", e);
      }

      const u1 = await listen<AuthStatusChangedPayload>(
        "auth://status-changed",
        (e) => {
          if (!disposed) handleAuthState(e.payload.authState);
        },
      );
      if (disposed) {
        u1();
        return;
      }
      unsubs.push(u1);

      const u2 = await listen<AuthStatusChangedPayload>(
        "auth://activation-success",
        (e) => {
          if (!disposed) {
            handleAuthState(e.payload.authState);
            setErrorMessage(null);
          }
        },
      );
      if (disposed) {
        u2();
        return;
      }
      unsubs.push(u2);

      const u3 = await listen<AuthActivationFailedPayload>(
        "auth://activation-failed",
        (e) => {
          if (!disposed) {
            setIsActivating(false);
            setErrorMessage(e.payload.reason);
          }
        },
      );
      if (disposed) {
        u3();
        return;
      }
      unsubs.push(u3);

      const u4 = await listen<AuthStatusChangedPayload>(
        "auth://refresh-required",
        (e) => {
          if (!disposed) handleAuthState(e.payload.authState);
        },
      );
      if (disposed) {
        u4();
        return;
      }
      unsubs.push(u4);

      const u5 = await listen<AuthStatusChangedPayload>(
        "auth://license-invalid",
        (e) => {
          if (!disposed) handleAuthState(e.payload.authState);
        },
      );
      if (disposed) {
        u5();
        return;
      }
      unsubs.push(u5);
    };

    setup();
    return () => {
      disposed = true;
      unsubs.forEach((u) => u());
    };
  }, [handleAuthState]);

  const handleActivate = async () => {
    if (!licenseKey.trim() || isActivating) return;
    setIsActivating(true);
    setErrorMessage(null);
    try {
      const result = await invoke<ActivationResult>("activate_license", {
        licenseKey: licenseKey.trim(),
      });
      if (result.success) {
        handleAuthState(result.authState);
        setLicenseKey("");
      } else {
        setErrorMessage("Activation failed. Please check your license key.");
        setIsActivating(false);
      }
    } catch (e: unknown) {
      const msg =
        e && typeof e === "object" && "message" in e
          ? (e as { message: string }).message
          : String(e);
      setErrorMessage(msg);
      setIsActivating(false);
    }
  };

  const handleRefresh = async () => {
    try {
      const state = await invoke<AuthState>("refresh_license");
      handleAuthState(state);
    } catch (e: unknown) {
      const msg =
        e && typeof e === "object" && "message" in e
          ? (e as { message: string }).message
          : String(e);
      setErrorMessage(msg);
    }
  };

  const handleClear = async () => {
    try {
      await invoke("clear_license");
    } catch (e: unknown) {
      const msg =
        e && typeof e === "object" && "message" in e
          ? (e as { message: string }).message
          : String(e);
      setErrorMessage(msg);
    }
  };

  const status = authState?.status ?? "initializing";
  const indicatorState = getLicenseIndicatorState(status);
  const canCheckUpdates = indicatorState.isAccessAllowed;

  const handleUpdateCheck = async () => {
    if (!canCheckUpdates || isCheckingUpdates) return;

    setIsCheckingUpdates(true);
    setUpdateCheckResult(null);

    try {
      const result = await invoke<UpdateEntitlementCheckResult>(
        "check_update_entitlement",
      );
      setUpdateCheckResult(result);
    } catch (e: unknown) {
      const msg =
        e && typeof e === "object" && "message" in e
          ? (e as { message: string }).message
          : String(e);
      setUpdateCheckResult({ status: "server_error" });
      setErrorMessage(msg);
    } finally {
      setIsCheckingUpdates(false);
    }
  };

  const updateStatusBanner = (() => {
    if (!updateCheckResult) return null;

    switch (updateCheckResult.status) {
      case "update_available":
        return (
          <div className="banner banner-success mt-2">
            Update available: {updateCheckResult.data?.latestVersion}
          </div>
        );
      case "no_update":
        return (
          <div className="banner banner-success mt-2">No update available</div>
        );
      case "not_entitled":
        return (
          <div className="banner banner-warning mt-2">
            Official updates are unavailable for this license.
          </div>
        );
      case "channel_not_allowed":
        return (
          <div className="banner banner-warning mt-2">
            Official updates are unavailable for this build channel.
          </div>
        );
      case "auth_required":
        return (
          <div className="banner banner-warning mt-2">
            Refresh your license before checking for updates.
          </div>
        );
      case "offline":
        return (
          <div className="banner banner-warning mt-2">
            Offline: unable to verify update entitlement.
          </div>
        );
      case "server_error":
        return (
          <div className="banner banner-error mt-2">
            Update check failed. Please try again.
          </div>
        );
      default:
        return null;
    }
  })();

  return (
    <div className="settings-group">
      <div className="settings-group-title">License</div>

      {(status === "initializing" ||
        status === "credentials_found" ||
        status === "validating") && (
        <div className="flex items-center gap-6">
          <span className="spinner" />
          <span className="text-sm text-muted">Checking license...</span>
        </div>
      )}

      {(status === "not_activated" || status === "invalid") && (
        <>
          <input
            className="input"
            placeholder="Enter license key (ASPECTSHIFT-...)"
            value={licenseKey}
            onChange={(e) => setLicenseKey(e.target.value)}
            disabled={isActivating}
            onKeyDown={(e) => {
              if (e.key === "Enter") handleActivate();
            }}
          />
          <button
            className="btn btn-primary btn-sm btn-full mt-2"
            onClick={handleActivate}
            disabled={isActivating || !licenseKey.trim()}
          >
            {isActivating ? (
              <>
                <span className="spinner" /> Activating...
              </>
            ) : (
              "Activate License"
            )}
          </button>
        </>
      )}

      {status === "activating" && (
        <div className="flex items-center gap-6">
          <span className="spinner" />
          <span className="text-sm text-muted">Activating license...</span>
        </div>
      )}

      {(status === "valid" || status === "offline_valid") && (
        <>
          <div className="banner banner-success">
            Licensed
            {status === "offline_valid" && " (Offline)"}
            {authState?.tier === "pro" && " . Pro"}
          </div>
          {authState?.machineId && (
            <div className="text-xs text-muted mt-2">
              Machine: {authState.machineId}
            </div>
          )}
          {authState?.jwtExpiresAt && (
            <div className="text-xs text-muted">
              Expires: {new Date(authState.jwtExpiresAt).toLocaleDateString()}
            </div>
          )}
          <button className="btn btn-ghost btn-sm mt-4" onClick={handleClear}>
            Deactivate
          </button>
        </>
      )}

      {canCheckUpdates && (
        <>
          <div className="settings-group-title mt-4">Updates</div>
          <button
            className="btn btn-sm btn-full"
            onClick={handleUpdateCheck}
            disabled={isCheckingUpdates}
          >
            {isCheckingUpdates ? (
              <>
                <span className="spinner" /> Checking updates...
              </>
            ) : (
              "Check for Updates"
            )}
          </button>
          {updateStatusBanner}
        </>
      )}

      {status === "refresh_required" && (
        <>
          <div className="banner banner-warning">License refresh recommended</div>
          <button className="btn btn-sm btn-full mt-2" onClick={handleRefresh}>
            Refresh License
          </button>
        </>
      )}

      {status === "grace_period" && (
        <>
          <div className="banner banner-warning" style={{ fontSize: 12 }}>
            {authState?.graceExpiresAt && (new Date(authState.graceExpiresAt).getTime() - Date.now()) < 48 * 3600 * 1000 ? (
              "Unable to verify your license. Reconnect to the internet soon to avoid interruption."
            ) : (
              "Connection issue detected. Your license couldn't be refreshed, but everything will continue working. We'll retry automatically."
            )}
          </div>
          <button className="btn btn-sm btn-full mt-2" onClick={handleRefresh}>
            Reconnect &amp; Refresh
          </button>
        </>
      )}

      {status === "expired" && (
        <>
          <div className="banner banner-error">
            License verification required. Please reconnect to continue.
          </div>
          <input
            className="input mt-2"
            placeholder="Enter license key (if new)"
            value={licenseKey}
            onChange={(e) => setLicenseKey(e.target.value)}
            disabled={isActivating}
          />
          <button
            className="btn btn-primary btn-sm btn-full mt-2"
            onClick={handleActivate}
            disabled={isActivating || !licenseKey.trim()}
          >
            Re-activate
          </button>
        </>
      )}

      {status === "machine_mismatch" && (
        <div className="banner banner-error">License bound to another machine</div>
      )}

      {status === "corrupted" && (
        <>
          <div className="banner banner-error">
            License data corrupted - please re-activate
          </div>
          <button className="btn btn-danger btn-sm mt-2" onClick={handleClear}>
            Clear &amp; Re-activate
          </button>
        </>
      )}

      {status === "recoverable_error" && (
        <>
          <div className="banner banner-error">
            License status could not be loaded. Try refreshing.
          </div>
          <button className="btn btn-sm btn-full mt-2" onClick={handleRefresh}>
            Refresh License
          </button>
        </>
      )}

      {errorMessage && (
        <div className="banner banner-error mt-2" style={{ fontSize: 11 }}>
          {errorMessage}
        </div>
      )}
    </div>
  );
}
