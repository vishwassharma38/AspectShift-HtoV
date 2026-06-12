import {
  PRESENCE_TRANSITION_MS,
  usePresenceTransition,
} from "./usePresenceTransition";

export type UpdateFlowStage =
  | "idle"
  | "checking_entitlement"
  | "entitlement_denied"
  | "already_latest"
  | "checking_updater"
  | "update_available"
  | "downloading"
  | "installing"
  | "installed_restart_required"
  | "failed";

interface UpdateModalProps {
  open: boolean;
  stage: UpdateFlowStage;
  currentVersion: string | null;
  latestVersion: string | null;
  releaseNotes: string | null;
  progressPercent: number | null;
  progressLabel: string | null;
  errorMessage: string | null;
  onDownloadAndInstall: () => void;
  onRestartNow: () => void;
  onLater: () => void;
}

type StageCopy = {
  eyebrow: string;
  title: string;
  accent: "success" | "warning" | "error" | "info";
  summary: string;
};

function getStageCopy(stage: UpdateFlowStage): StageCopy {
  switch (stage) {
    case "checking_entitlement":
      return {
        eyebrow: "Update Check",
        title: "Verifying entitlement",
        accent: "info",
        summary: "Checking whether this license can receive an update.",
      };
    case "entitlement_denied":
      return {
        eyebrow: "Update Check",
        title: "Update entitlement denied",
        accent: "warning",
        summary: "This license is not currently entitled to an update.",
      };
    case "already_latest":
      return {
        eyebrow: "Update Check",
        title: "Already on the latest version",
        accent: "success",
        summary: "The installed build already matches the latest approved version.",
      };
    case "checking_updater":
      return {
        eyebrow: "Update Check",
        title: "Checking updater manifest",
        accent: "info",
        summary: "The license check passed. Verifying the signed release manifest now.",
      };
    case "update_available":
      return {
        eyebrow: "Update Available",
        title: "A signed update is ready",
        accent: "success",
        summary: "You can download and install the update when you are ready.",
      };
    case "downloading":
      return {
        eyebrow: "Downloading",
        title: "Downloading update",
        accent: "info",
        summary: "The signed updater bundle is being downloaded and verified.",
      };
    case "installing":
      return {
        eyebrow: "Installing",
        title: "Installing update",
        accent: "info",
        summary: "The signed update is being installed locally.",
      };
    case "installed_restart_required":
      return {
        eyebrow: "Restart Required",
        title: "Update installed",
        accent: "success",
        summary: "The new version is ready. Restart the app to finish loading it.",
      };
    case "failed":
      return {
        eyebrow: "Update Failed",
        title: "Update flow failed",
        accent: "error",
        summary: "The update could not be completed. Your license state was not changed.",
      };
    case "idle":
    default:
      return {
        eyebrow: "Update",
        title: "Update",
        accent: "info",
        summary: "",
      };
  }
}

function formatVersion(version: string | null): string {
  if (!version) return "n/a";
  return version.startsWith("v") ? version : `v${version}`;
}

export function UpdateModal({
  open,
  stage,
  currentVersion,
  latestVersion,
  releaseNotes,
  progressPercent,
  progressLabel,
  errorMessage,
  onDownloadAndInstall,
  onRestartNow,
  onLater,
}: UpdateModalProps) {
  const { isRendered, isClosing } = usePresenceTransition(
    open,
    PRESENCE_TRANSITION_MS,
  );

  if (!isRendered || stage === "idle") return null;

  const copy = getStageCopy(stage);
  const isBusy =
    stage === "checking_entitlement" ||
    stage === "checking_updater" ||
    stage === "downloading" ||
    stage === "installing";
  const showActionButtons =
    stage === "update_available" ||
    stage === "installed_restart_required" ||
    stage === "failed";

  return (
    <div
      className="modal-backdrop modal-summon-backdrop up-backdrop"
      data-state={isClosing ? "closing" : "open"}
      onClick={isBusy ? undefined : onLater}
    >
      <div
        className="modal-panel modal-summon-panel up-panel"
        data-state={isClosing ? "closing" : "open"}
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
        aria-label="Application update"
      >
        <div className="up-header">
          <div className="up-header-icon" data-accent={copy.accent}>
            {copy.accent === "success" && (
              <svg width="18" height="18" viewBox="0 0 18 18" fill="none">
                <path
                  d="M3 9.5l3 3 9-9"
                  stroke="currentColor"
                  strokeWidth="1.7"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                />
              </svg>
            )}
            {copy.accent === "warning" && (
              <svg width="18" height="18" viewBox="0 0 18 18" fill="none">
                <path
                  d="M9 2L16 14H2L9 2z"
                  stroke="currentColor"
                  strokeWidth="1.5"
                  strokeLinejoin="round"
                />
                <path
                  d="M9 6v4M9 12.5v.5"
                  stroke="currentColor"
                  strokeWidth="1.5"
                  strokeLinecap="round"
                />
              </svg>
            )}
            {copy.accent === "error" && (
              <svg width="18" height="18" viewBox="0 0 18 18" fill="none">
                <path
                  d="M9 2.2c3.76 0 6.8 3.04 6.8 6.8s-3.04 6.8-6.8 6.8-6.8-3.04-6.8-6.8 3.04-6.8 6.8-6.8z"
                  stroke="currentColor"
                  strokeWidth="1.5"
                />
                <path
                  d="M9 5.6v4.1M9 11.8v.6"
                  stroke="currentColor"
                  strokeWidth="1.6"
                  strokeLinecap="round"
                />
              </svg>
            )}
            {copy.accent === "info" && (
              <svg width="18" height="18" viewBox="0 0 18 18" fill="none">
                <path
                  d="M9 16A7 7 0 1 0 9 2a7 7 0 0 0 0 14z"
                  stroke="currentColor"
                  strokeWidth="1.5"
                />
                <path
                  d="M9 8.2v4.2M9 6.1v.5"
                  stroke="currentColor"
                  strokeWidth="1.6"
                  strokeLinecap="round"
                />
              </svg>
            )}
          </div>

          <div className="up-header-text">
            <p className="up-eyebrow">{copy.eyebrow}</p>
            <h2 className="up-title">{copy.title}</h2>
            <p className="up-summary">{copy.summary}</p>
          </div>

          {showActionButtons && (
            <button
              className="modal-close up-close"
              onClick={onLater}
              aria-label="Close update dialog"
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
          )}
        </div>

        <div className="up-meta-grid">
          <div className="up-meta-card">
            <span className="up-meta-label">Current</span>
            <code className="up-meta-value">{formatVersion(currentVersion)}</code>
          </div>
          <div className="up-meta-card">
            <span className="up-meta-label">Latest</span>
            <code className="up-meta-value">{formatVersion(latestVersion)}</code>
          </div>
        </div>

        {(stage === "downloading" ||
          stage === "installing" ||
          stage === "checking_entitlement" ||
          stage === "checking_updater") && (
          <div className="up-progress-card">
            <div className="up-progress-row">
              <span className="up-progress-label">
                {progressLabel ?? copy.title}
              </span>
              {progressPercent !== null && (
                <span className="up-progress-pct">{progressPercent}%</span>
              )}
            </div>
            <div className="up-progress-track" aria-hidden="true">
              <div
                className="up-progress-fill"
                style={{
                  width:
                    progressPercent !== null
                      ? `${Math.max(0, Math.min(100, progressPercent))}%`
                      : stage === "installing"
                        ? "100%"
                        : "22%",
                }}
              />
            </div>
          </div>
        )}

        {releaseNotes && stage === "update_available" && (
          <div className="up-notes">
            <div className="up-section-label">Release notes</div>
            <div className="up-notes-body">{releaseNotes}</div>
          </div>
        )}

        {errorMessage && (
          <div className="up-error">
            <svg width="13" height="13" viewBox="0 0 13 13" fill="none">
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
            <span>{errorMessage}</span>
          </div>
        )}

        <div className="up-footer">
          {stage === "update_available" && (
            <>
              <button
                type="button"
                className="update-modal-btn update-modal-btn--primary"
                onClick={onDownloadAndInstall}
              >
                Download and Install
              </button>
              <button
                type="button"
                className="update-modal-btn"
                onClick={onLater}
              >
                Later
              </button>
            </>
          )}

          {stage === "installed_restart_required" && (
            <>
              <button
                type="button"
                className="update-modal-btn update-modal-btn--primary"
                onClick={onRestartNow}
              >
                Restart Now
              </button>
              <button
                type="button"
                className="update-modal-btn"
                onClick={onLater}
              >
                Later
              </button>
            </>
          )}

          {stage === "failed" && (
            <>
              <button
                type="button"
                className="update-modal-btn update-modal-btn--primary"
                onClick={onLater}
              >
                Close
              </button>
            </>
          )}

          {!showActionButtons && (
            <button
              type="button"
              className="update-modal-btn update-modal-btn--primary"
              onClick={onLater}
              disabled={isBusy}
            >
              {isBusy ? "Working..." : "Close"}
            </button>
          )}
        </div>

        <style>{`
          .up-backdrop {
            background: var(--glass-backdrop-bg);
            backdrop-filter: blur(var(--glass-backdrop-blur)) saturate(var(--glass-backdrop-saturation));
            -webkit-backdrop-filter: blur(var(--glass-backdrop-blur)) saturate(var(--glass-backdrop-saturation));
          }

          .up-panel {
            width: min(640px, 100%);
            padding: 22px;
          }

          .up-header {
            display: flex;
            align-items: flex-start;
            gap: 14px;
          }

          .up-header-icon {
            width: 40px;
            height: 40px;
            border-radius: 12px;
            display: flex;
            align-items: center;
            justify-content: center;
            flex-shrink: 0;
            border: 1px solid var(--border);
          }

          .up-header-icon[data-accent="success"] {
            color: var(--success);
            background: color-mix(in srgb, var(--success) 10%, var(--bg-input));
          }

          .up-header-icon[data-accent="warning"] {
            color: var(--warning);
            background: color-mix(in srgb, var(--warning) 12%, var(--bg-input));
          }

          .up-header-icon[data-accent="error"] {
            color: var(--error);
            background: color-mix(in srgb, var(--error) 10%, var(--bg-input));
          }

          .up-header-icon[data-accent="info"] {
            color: var(--info);
            background: color-mix(in srgb, var(--info) 10%, var(--bg-input));
          }

          .up-header-text {
            flex: 1;
            min-width: 0;
            display: flex;
            flex-direction: column;
            gap: 6px;
          }

          .up-eyebrow {
            color: var(--text-muted);
            font-size: 10px;
            font-weight: 700;
            letter-spacing: 0.12em;
            text-transform: uppercase;
          }

          .up-title {
            color: var(--text-primary);
            font-size: 26px;
            line-height: 1.05;
            letter-spacing: -0.03em;
          }

          .up-summary {
            color: var(--text-secondary);
            font-size: 13px;
            line-height: 1.6;
          }

          .up-close {
            flex-shrink: 0;
          }

          .up-meta-grid {
            display: grid;
            grid-template-columns: repeat(2, minmax(0, 1fr));
            gap: 12px;
            margin-top: 18px;
          }

          .up-meta-card {
            padding: 14px 16px;
            border-radius: 14px;
            border: 1px solid var(--border);
            background: var(--bg-input);
            display: flex;
            flex-direction: column;
            gap: 6px;
          }

          .up-meta-label {
            color: var(--text-muted);
            font-size: 10px;
            font-weight: 700;
            letter-spacing: 0.08em;
            text-transform: uppercase;
          }

          .up-meta-value {
            color: var(--text-primary);
            font-size: 12px;
            font-weight: 600;
            font-family: var(--font-mono);
          }

          .up-progress-card {
            margin-top: 18px;
            padding: 14px 16px;
            border-radius: 14px;
            border: 1px solid var(--border);
            background: color-mix(in srgb, var(--bg-input) 82%, var(--bg-card));
          }

          .up-progress-row {
            display: flex;
            align-items: center;
            justify-content: space-between;
            gap: 12px;
            margin-bottom: 10px;
          }

          .up-progress-label {
            color: var(--text-secondary);
            font-size: 12px;
            font-weight: 600;
          }

          .up-progress-pct {
            color: var(--text-primary);
            font-size: 12px;
            font-weight: 700;
            font-family: var(--font-mono);
          }

          .up-progress-track {
            height: 10px;
            border-radius: 999px;
            background: var(--progress-bg);
            overflow: hidden;
          }

          .up-progress-fill {
            height: 100%;
            border-radius: inherit;
            background: linear-gradient(90deg, var(--accent), color-mix(in srgb, var(--accent) 55%, var(--info)));
            box-shadow: 0 0 18px color-mix(in srgb, var(--accent) 20%, transparent);
            transition: width 0.25s ease;
          }

          .up-notes {
            margin-top: 18px;
          }

          .up-section-label {
            margin-bottom: 10px;
            color: var(--text-muted);
            font-size: 10px;
            font-weight: 700;
            letter-spacing: 0.1em;
            text-transform: uppercase;
          }

          .up-notes-body {
            max-height: 220px;
            overflow: auto;
            padding: 14px 16px;
            border-radius: 14px;
            border: 1px solid var(--border);
            background: var(--bg-input);
            color: var(--text-secondary);
            font-size: 12px;
            line-height: 1.7;
            white-space: pre-wrap;
          }

          .up-error {
            margin-top: 18px;
            padding: 12px 14px;
            border-radius: 12px;
            border: 1px solid color-mix(in srgb, var(--error) 18%, var(--border));
            background: color-mix(in srgb, var(--error) 12%, var(--bg-input));
            color: var(--text-primary);
            font-size: 12px;
            line-height: 1.5;
            display: flex;
            align-items: flex-start;
            gap: 10px;
          }

          .up-footer {
            display: flex;
            flex-wrap: wrap;
            justify-content: flex-end;
            gap: 10px;
            margin-top: 20px;
          }

          .update-modal-btn {
            min-width: 120px;
            height: 38px;
            padding: 0 14px;
            border-radius: 10px;
            border: 1px solid var(--border);
            background: var(--bg-input);
            color: var(--text-primary);
            font: inherit;
            font-size: 12px;
            font-weight: 600;
            cursor: pointer;
            transition:
              background 0.15s ease,
              border-color 0.15s ease,
              color 0.15s ease,
              transform 0.15s ease;
          }

          .update-modal-btn:hover:not(:disabled) {
            background: var(--bg-hover);
            border-color: var(--border-strong);
            transform: translateY(-1px);
          }

          .update-modal-btn:disabled {
            opacity: 0.6;
            cursor: progress;
          }

          .update-modal-btn--primary {
            background: var(--accent);
            border-color: var(--accent);
            color: var(--text-on-accent);
          }

          .update-modal-btn--primary:hover:not(:disabled) {
            background: var(--accent-hover);
            border-color: var(--accent-hover);
          }

          @media (max-width: 640px) {
            .up-panel {
              width: min(100%, calc(100vw - 24px));
              padding: 18px;
            }

            .up-meta-grid {
              grid-template-columns: 1fr;
            }

            .up-footer {
              justify-content: stretch;
            }

            .update-modal-btn {
              flex: 1;
              min-width: 0;
            }
          }
        `}</style>
      </div>
    </div>
  );
}
