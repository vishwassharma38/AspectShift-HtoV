import { formatBuildModeLabel } from "../../utils/buildMetadata";
import type { ProductEdition } from "../../utils/productEdition";
import {
  PRESENCE_TRANSITION_MS,
  usePresenceTransition,
} from "./usePresenceTransition";

interface AboutDialogProps {
  open: boolean;
  onClose: () => void;
  metadata: {
    appName: string;
    appVersion: string;
    tauriVersion: string;
    identifier: string;
    buildMode: string;
  };
  edition: ProductEdition;
}

export function AboutDialog({
  open,
  onClose,
  metadata,
  edition,
}: AboutDialogProps) {
  const { isRendered, isClosing } = usePresenceTransition(
    open,
    PRESENCE_TRANSITION_MS,
  );

  if (!isRendered) return null;

  return (
    <div
      className="modal-backdrop modal-summon-backdrop abt-backdrop"
      data-state={isClosing ? "closing" : "open"}
      onClick={onClose}
    >
      <div
        className="modal-panel modal-summon-panel abt-panel"
        data-state={isClosing ? "closing" : "open"}
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
        aria-label="About"
      >
        <div className="abt-header">
          <div className="abt-logo-wrap">
            <img
              src="logo.png"
              alt=""
              className="abt-logo"
              aria-hidden="true"
            />
          </div>

          <div className="abt-identity">
            <p className="abt-eyebrow">About</p>
            <h2 className="abt-name">{metadata.appName}</h2>
            <code className="abt-version">v{metadata.appVersion}</code>
          </div>

          <button
            className="modal-close abt-close"
            onClick={onClose}
            aria-label="Close about dialog"
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

        <div className="abt-rule" />

        <dl className="abt-meta">
          {[
            { label: "Edition", value: edition, mono: false },
            {
              label: "Build",
              value: formatBuildModeLabel(metadata.buildMode),
              mono: false,
            },
            {
              label: "Tauri Runtime",
              value: metadata.tauriVersion,
              mono: true,
            },
            { label: "Bundle ID", value: metadata.identifier, mono: true },
          ].map(({ label, value, mono }) => (
            <div key={label} className="abt-meta-row">
              <dt className="abt-meta-label">{label}</dt>
              <dd className="abt-meta-value">
                {mono ? <code className="abt-meta-code">{value}</code> : value}
              </dd>
            </div>
          ))}
        </dl>

        <div className="abt-footer">
          <span className="abt-copyright">
            Copyright © 2026 Vishwas Sharma
          </span>
        </div>
      </div>

      <style>{`
        .abt-backdrop {
          background: var(--glass-backdrop-bg);
          backdrop-filter: blur(var(--glass-backdrop-blur)) saturate(var(--glass-backdrop-saturation));
          -webkit-backdrop-filter: blur(var(--glass-backdrop-blur)) saturate(var(--glass-backdrop-saturation));
        }

        .abt-panel {
          width: min(400px, 100%);
          padding: 0;
          border-radius: 16px;
          overflow: hidden;
          display: flex;
          flex-direction: column;
        }

        .abt-header {
          display: flex;
          align-items: flex-start;
          gap: 14px;
          padding: 24px 24px 20px;
        }
        .abt-logo-wrap {
          width: 44px;
          height: 44px;
          border-radius: 11px;
          border: 1px solid var(--border);
          background: var(--bg-input);
          display: flex;
          align-items: center;
          justify-content: center;
          flex-shrink: 0;
          overflow: hidden;
        }
        .abt-logo {
          width: 28px;
          height: 28px;
          object-fit: contain;
        }
        .abt-identity {
          flex: 1;
          min-width: 0;
          display: flex;
          flex-direction: column;
          gap: 5px;
        }
        .abt-eyebrow {
          font-size: 10px;
          font-weight: 700;
          letter-spacing: 0.1em;
          text-transform: uppercase;
          color: var(--text-muted);
          line-height: 1;
        }
        .abt-name {
          font-size: 18px;
          font-weight: 700;
          letter-spacing: -0.03em;
          color: var(--text-primary);
          line-height: 1.1;
          white-space: nowrap;
          overflow: hidden;
          text-overflow: ellipsis;
        }
        .abt-version {
          font-family: var(--font-mono);
          font-size: 11px;
          font-weight: 600;
          color: var(--text-secondary);
        }
        .abt-close {
          width: 30px;
          height: 30px;
          border-radius: 8px;
          flex-shrink: 0;
          display: flex;
          align-items: center;
          justify-content: center;
          margin-top: 0;
        }

        .abt-rule {
          height: 1px;
          background: var(--border);
          margin: 0 24px;
        }

        .abt-meta {
          display: flex;
          flex-direction: column;
          gap: 0;
          padding: 6px 0;
          margin: 0;
        }
        .abt-meta-row {
          display: flex;
          align-items: center;
          justify-content: space-between;
          gap: 16px;
          padding: 10px 24px;
          border-bottom: 1px solid var(--border);
        }
        .abt-meta-row:last-child {
          border-bottom: none;
        }
        .abt-meta-label {
          font-size: 11px;
          font-weight: 500;
          color: var(--text-muted);
          flex-shrink: 0;
        }
        .abt-meta-value {
          font-size: 11px;
          color: var(--text-primary);
          text-align: right;
          min-width: 0;
          overflow: hidden;
          text-overflow: ellipsis;
          white-space: nowrap;
        }
        .abt-meta-code {
          font-family: var(--font-mono);
          font-size: 10.5px;
          font-weight: 500;
          color: var(--text-secondary);
        }

        .abt-footer {
          padding: 14px 24px;
          border-top: 1px solid var(--border);
          background: var(--bg-secondary);
        }
        .abt-copyright {
          font-size: 10px;
          color: var(--text-muted);
          font-weight: 500;
          letter-spacing: 0.01em;
        }
      `}</style>
    </div>
  );
}
