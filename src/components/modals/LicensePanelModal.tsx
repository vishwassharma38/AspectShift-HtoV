import type { AuthState } from "../../types/backend";
import { getLicenseIndicatorState } from "../../utils/licenseIndicatorMapping";
import {
  PRESENCE_TRANSITION_MS,
  usePresenceTransition,
} from "./usePresenceTransition";

interface LicensePanelModalProps {
  open: boolean;
  authState: AuthState | null;
  licenseKey: string;
  errorMessage: string | null;
  isActivating: boolean;
  onLicenseKeyChange: (value: string) => void;
  onActivate: () => void;
  onRefresh: () => void;
  onClear: () => void;
  onClose: () => void;
}

const TIER_LABELS: Record<string, string> = {
  community: "Community",
  pro: "Pro",
  enterprise: "Enterprise",
};

const STATUS_COLOR_VAR = {
  green: "var(--success)",
  yellow: "var(--warning)",
  red: "var(--error)",
  gray: "var(--text-muted)",
} as const;

const STATUS_BG = {
  green: "color-mix(in srgb, var(--success) 10%, transparent)",
  yellow: "color-mix(in srgb, var(--warning) 10%, transparent)",
  red: "color-mix(in srgb, var(--error) 10%, transparent)",
  gray: "var(--bg-input)",
} as const;

export function LicensePanelModal({
  open,
  authState,
  licenseKey,
  errorMessage,
  isActivating,
  onLicenseKeyChange,
  onActivate,
  onRefresh,
  onClear,
  onClose,
}: LicensePanelModalProps) {
  const { isRendered, isClosing } = usePresenceTransition(
    open,
    PRESENCE_TRANSITION_MS,
  );

  if (!isRendered) return null;

  const rawStatus = authState?.status ?? "initializing";
  const indicatorState = getLicenseIndicatorState(rawStatus);
  const statusColor = STATUS_COLOR_VAR[indicatorState.color];
  const statusBg = STATUS_BG[indicatorState.color];
  const tier = authState?.tier ?? "community";

  const expiresAt = authState?.jwtExpiresAt
    ? new Date(authState.jwtExpiresAt)
    : null;
  const expiresLabel = expiresAt
    ? expiresAt.toLocaleDateString(undefined, {
        year: "numeric",
        month: "short",
        day: "numeric",
      })
    : "-";
  const expiresTime = expiresAt
    ? expiresAt.toLocaleTimeString(undefined, {
        hour: "2-digit",
        minute: "2-digit",
      })
    : null;

  return (
    <div
      className="modal-backdrop modal-summon-backdrop lp-backdrop"
      data-state={isClosing ? "closing" : "open"}
      onClick={onClose}
    >
      <div
        className="modal-panel modal-summon-panel lp-panel"
        data-state={isClosing ? "closing" : "open"}
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
        aria-label="License Panel"
      >
        <div className="lp-header">
          <div className="lp-header-text">
            <p className="lp-eyebrow">License</p>
            <h2 className="lp-title">Registration</h2>
          </div>
          <button
            className="modal-close lp-close"
            onClick={onClose}
            aria-label="Close license panel"
          >
            <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
              <path
                d="M1 1l10 10M11 1L1 11"
                stroke="currentColor"
                strokeWidth="1.5"
                strokeLinecap="round"
              />
            </svg>
          </button>
        </div>

        <div
          className="lp-status-banner"
          style={{
            background: statusBg,
            borderColor: `color-mix(in srgb, ${statusColor} 25%, transparent)`,
          }}
        >
          <span
            className="lp-status-dot"
            style={{
              background: statusColor,
              boxShadow: indicatorState.color !== "gray"
                ? `0 0 0 3px color-mix(in srgb, ${statusColor} 20%, transparent)`
                : "none",
            }}
          />
          <span className="lp-status-label" style={{ color: statusColor }}>
            {indicatorState.badgeText}
          </span>
          <span className="lp-status-tier">{TIER_LABELS[tier] ?? tier}</span>
        </div>

        <dl className="lp-meta">
          <div className="lp-meta-row">
            <dt className="lp-meta-label">Machine ID</dt>
            <dd className="lp-meta-value">
              <code className="lp-meta-code">
                {authState?.machineId ?? "Unavailable"}
              </code>
            </dd>
          </div>
          <div className="lp-meta-row">
            <dt className="lp-meta-label">Token Expires</dt>
            <dd className="lp-meta-value">
              {expiresAt ? (
                <>
                  <code className="lp-meta-code">{expiresLabel}</code>
                  {expiresTime && (
                    <span className="lp-meta-time">{expiresTime}</span>
                  )}
                </>
              ) : (
                <code className="lp-meta-code">-</code>
              )}
            </dd>
          </div>
        </dl>

        <div className="lp-rule" />

        <div className="lp-input-section">
          <label className="lp-input-label" htmlFor="lp-key-input">
            License Key
          </label>
          <div className="lp-input-row">
            <input
              id="lp-key-input"
              className="input lp-key-input"
              value={licenseKey}
              placeholder="ASPECTSHIFT-XXXX-XXXX-XXXX"
              spellCheck={false}
              autoComplete="off"
              onChange={(e) => onLicenseKeyChange(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") onActivate();
              }}
            />
            <button
              className="lp-activate-btn"
              onClick={onActivate}
              disabled={isActivating || !licenseKey.trim()}
            >
              {isActivating ? (
                <>
                  <span className="lp-spinner" />
                  Verifying
                </>
              ) : (
                "Register"
              )}
            </button>
          </div>
          <p className="lp-input-hint">
            Enter your key and press Register, or hit Enter.
          </p>
        </div>

        {errorMessage && (
          <div className="lp-error">
            <svg
              width="13"
              height="13"
              viewBox="0 0 13 13"
              fill="none"
              style={{ flexShrink: 0 }}
            >
              <circle
                cx="6.5"
                cy="6.5"
                r="5.5"
                stroke="currentColor"
                strokeWidth="1.3"
              />
              <path
                d="M6.5 4v3M6.5 9v.5"
                stroke="currentColor"
                strokeWidth="1.4"
                strokeLinecap="round"
              />
            </svg>
            {errorMessage}
          </div>
        )}

        <div className="lp-footer">
          <button className="lp-footer-btn" onClick={onRefresh}>
            <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
              <path
                d="M10.5 2A5 5 0 1 0 11 5.5"
                stroke="currentColor"
                strokeWidth="1.4"
                strokeLinecap="round"
              />
              <path
                d="M10.5 2V0M10.5 2H8.5"
                stroke="currentColor"
                strokeWidth="1.4"
                strokeLinecap="round"
                strokeLinejoin="round"
              />
            </svg>
            Refresh Status
          </button>
          <button
            className="lp-footer-btn lp-footer-btn--danger"
            onClick={onClear}
          >
            <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
              <path
                d="M1 1l10 10M11 1L1 11"
                stroke="currentColor"
                strokeWidth="1.4"
                strokeLinecap="round"
              />
            </svg>
            Clear License
          </button>
        </div>
      </div>

      <style>{`
        .lp-backdrop {
          background: var(--glass-backdrop-bg);
          backdrop-filter: blur(var(--glass-backdrop-blur)) saturate(var(--glass-backdrop-saturation));
          -webkit-backdrop-filter: blur(var(--glass-backdrop-blur)) saturate(var(--glass-backdrop-saturation));
        }

        .lp-panel {
          width: min(440px, 100%);
          padding: 0;
          border-radius: 16px;
          overflow: hidden;
          display: flex;
          flex-direction: column;
        }

        .lp-header {
          display: flex;
          align-items: flex-start;
          justify-content: space-between;
          gap: 12px;
          padding: 24px 24px 20px;
        }
        .lp-eyebrow {
          font-size: 10px;
          font-weight: 700;
          letter-spacing: 0.1em;
          text-transform: uppercase;
          color: var(--text-muted);
          margin-bottom: 4px;
          line-height: 1;
        }
        .lp-title {
          font-size: 20px;
          font-weight: 700;
          letter-spacing: -0.03em;
          color: var(--text-primary);
          line-height: 1.1;
        }
        .lp-close {
          width: 30px;
          height: 30px;
          border-radius: 8px;
          flex-shrink: 0;
          display: flex;
          align-items: center;
          justify-content: center;
        }

        .lp-status-banner {
          display: flex;
          align-items: center;
          gap: 9px;
          margin: 0 24px 20px;
          padding: 11px 14px;
          border-radius: 10px;
          border: 1px solid;
        }
        .lp-status-dot {
          width: 8px;
          height: 8px;
          border-radius: 50%;
          flex-shrink: 0;
        }
        .lp-status-label {
          font-size: 12px;
          font-weight: 600;
          flex: 1;
        }
        .lp-status-tier {
          font-size: 10px;
          font-weight: 700;
          letter-spacing: 0.07em;
          text-transform: uppercase;
          color: var(--text-muted);
          background: var(--bg-card);
          border: 1px solid var(--border);
          padding: 2px 8px;
          border-radius: 20px;
        }

        .lp-meta {
          display: flex;
          flex-direction: column;
          margin: 0;
          padding: 0;
        }
        .lp-meta-row {
          display: flex;
          align-items: center;
          justify-content: space-between;
          gap: 16px;
          padding: 10px 24px;
          border-top: 1px solid var(--border);
        }
        .lp-meta-label {
          font-size: 11px;
          font-weight: 500;
          color: var(--text-muted);
          flex-shrink: 0;
        }
        .lp-meta-value {
          display: flex;
          align-items: center;
          gap: 8px;
          text-align: right;
          min-width: 0;
          overflow: hidden;
        }
        .lp-meta-code {
          font-family: "JetBrains Mono", monospace;
          font-size: 10.5px;
          font-weight: 500;
          color: var(--text-secondary);
          overflow: hidden;
          text-overflow: ellipsis;
          white-space: nowrap;
        }
        .lp-meta-time {
          font-family: "JetBrains Mono", monospace;
          font-size: 10px;
          color: var(--text-muted);
          flex-shrink: 0;
        }

        .lp-rule {
          height: 1px;
          background: var(--border);
          margin: 8px 0 0;
        }

        .lp-input-section {
          padding: 20px 24px 4px;
          display: flex;
          flex-direction: column;
          gap: 8px;
        }
        .lp-input-label {
          font-size: 10px;
          font-weight: 700;
          letter-spacing: 0.08em;
          text-transform: uppercase;
          color: var(--text-muted);
        }
        .lp-input-row {
          display: flex;
          gap: 8px;
          align-items: center;
        }
        .lp-key-input {
          flex: 1;
          font-family: "JetBrains Mono", monospace;
          font-size: 12px;
          letter-spacing: 0.04em;
          padding: 9px 12px;
          border-radius: 9px;
        }
        .lp-key-input::placeholder {
          letter-spacing: 0.04em;
        }
        .lp-activate-btn {
          display: inline-flex;
          align-items: center;
          gap: 7px;
          padding: 9px 18px;
          border-radius: 9px;
          border: 1px solid var(--accent);
          background: var(--accent);
          color: #fff;
          font: inherit;
          font-size: 12px;
          font-weight: 600;
          cursor: pointer;
          white-space: nowrap;
          transition: background 0.15s ease, border-color 0.15s ease, opacity 0.15s ease;
          flex-shrink: 0;
        }
        .lp-activate-btn:hover {
          background: var(--accent-hover);
          border-color: var(--accent-hover);
        }
        .lp-activate-btn:active {
          transform: scale(0.97);
        }
        .lp-activate-btn:disabled {
          opacity: 0.4;
          cursor: not-allowed;
          pointer-events: none;
        }
        .lp-input-hint {
          font-size: 10px;
          color: var(--text-muted);
          line-height: 1.4;
        }

        .lp-spinner {
          width: 11px;
          height: 11px;
          border: 2px solid rgba(255, 255, 255, 0.3);
          border-top-color: #fff;
          border-radius: 50%;
          animation: lpSpin 0.65s linear infinite;
          flex-shrink: 0;
        }
        @keyframes lpSpin {
          to {
            transform: rotate(360deg);
          }
        }

        .lp-error {
          display: flex;
          align-items: flex-start;
          gap: 8px;
          margin: 12px 24px 0;
          padding: 10px 14px;
          border-radius: 9px;
          border: 1px solid color-mix(in srgb, var(--error) 30%, transparent);
          background: color-mix(in srgb, var(--error) 8%, transparent);
          color: var(--error);
          font-size: 11px;
          font-weight: 500;
          line-height: 1.5;
        }

        .lp-footer {
          display: flex;
          align-items: center;
          gap: 6px;
          padding: 16px 24px;
          margin-top: 16px;
          border-top: 1px solid var(--border);
          background: var(--bg-secondary);
        }
        .lp-footer-btn {
          display: inline-flex;
          align-items: center;
          gap: 6px;
          padding: 7px 13px;
          border-radius: 8px;
          border: 1px solid var(--border);
          background: transparent;
          color: var(--text-secondary);
          font: inherit;
          font-size: 11px;
          font-weight: 500;
          cursor: pointer;
          transition: background 0.15s ease, border-color 0.15s ease, color 0.15s ease;
        }
        .lp-footer-btn:hover {
          background: var(--bg-hover);
          border-color: var(--border-strong);
          color: var(--text-primary);
        }
        .lp-footer-btn:active {
          transform: scale(0.97);
        }
        .lp-footer-btn--danger {
          color: var(--error);
          border-color: color-mix(in srgb, var(--error) 25%, transparent);
        }
        .lp-footer-btn--danger:hover {
          background: color-mix(in srgb, var(--error) 8%, transparent);
          border-color: color-mix(in srgb, var(--error) 40%, transparent);
          color: var(--error);
        }
      `}</style>
    </div>
  );
}
