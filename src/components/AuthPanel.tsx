import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { AuthState, ActivationResult } from "../types/backend";

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
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  const handleAuthState = useCallback(
    (state: AuthState) => {
      setAuthState(state);
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

  const status = authState?.status ?? "not_activated";

  return (
    <div className="settings-group">
      <div className="settings-group-title">License</div>

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
            {authState?.tier === "licensed" && " . Pro"}
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
          <div className="banner banner-warning">
            Offline grace period active - reconnect to verify license
          </div>
          <button className="btn btn-sm btn-full mt-2" onClick={handleRefresh}>
            Reconnect &amp; Refresh
          </button>
        </>
      )}

      {status === "expired" && (
        <>
          <div className="banner banner-error">License expired</div>
          <input
            className="input mt-2"
            placeholder="Enter new license key"
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

      {errorMessage && (
        <div className="banner banner-error mt-2" style={{ fontSize: 11 }}>
          {errorMessage}
        </div>
      )}
    </div>
  );
}
