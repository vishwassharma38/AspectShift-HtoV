import type { DependencyId } from "../../types/backend";
import {
  getDependencyPromptCopy,
  type DependencyPromptMode,
} from "../../services/dependencyManager";
import {
  PRESENCE_TRANSITION_MS,
  usePresenceTransition,
} from "./usePresenceTransition";

interface DependencyModalProps {
  open: boolean;
  mode: DependencyPromptMode;
  missingDependencies: DependencyId[];
  installMessage: string | null;
  progressById: Partial<Record<DependencyId, number>>;
  isInstalling: boolean;
  canDefer: boolean;
  onInstall: () => void;
  onDefer: () => void;
  onClose: () => void;
}

// Icon that maps to each known dependency type
function DepIcon({ id }: { id: string }) {
  const lower = id.toLowerCase();
  if (lower.includes("ffmpeg"))
    return (
      <svg width="13" height="13" viewBox="0 0 13 13" fill="none">
        <rect
          x="1"
          y="3"
          width="7"
          height="7"
          rx="1.5"
          stroke="currentColor"
          strokeWidth="1.3"
        />
        <path
          d="M8 5.5l4-2v6l-4-2V5.5z"
          stroke="currentColor"
          strokeWidth="1.3"
          strokeLinejoin="round"
        />
      </svg>
    );
  if (lower.includes("python") || lower.includes("py"))
    return (
      <svg width="13" height="13" viewBox="0 0 13 13" fill="none">
        <path
          d="M4 1.5C4 1.5 3 1.5 3 2.5v3c0 1 1 1.5 1.5 1.5h4c.5 0 1.5.5 1.5 1.5v3c0 1-1 1-1 1"
          stroke="currentColor"
          strokeWidth="1.3"
          strokeLinecap="round"
        />
        <path
          d="M9 11.5c0 0 1 0 1-1V7.5c0-1-1-1.5-1.5-1.5h-4c-.5 0-1.5-.5-1.5-1.5v-3c0-1 1-1 1-1"
          stroke="currentColor"
          strokeWidth="1.3"
          strokeLinecap="round"
        />
        <circle cx="4.5" cy="3" r=".6" fill="currentColor" />
        <circle cx="8.5" cy="10" r=".6" fill="currentColor" />
      </svg>
    );
  if (lower.includes("whisper") || lower.includes("model"))
    return (
      <svg width="13" height="13" viewBox="0 0 13 13" fill="none">
        <path
          d="M6.5 1.5v10M4 3.5v6M9 3.5v6M1.5 6.5h10M2.5 5v3M10.5 5v3"
          stroke="currentColor"
          strokeWidth="1.3"
          strokeLinecap="round"
        />
      </svg>
    );
  // Generic package icon
  return (
    <svg width="13" height="13" viewBox="0 0 13 13" fill="none">
      <path
        d="M6.5 1.5L11.5 4v5L6.5 11.5 1.5 9V4L6.5 1.5z"
        stroke="currentColor"
        strokeWidth="1.3"
        strokeLinejoin="round"
      />
      <path
        d="M1.5 4l5 3 5-3M6.5 7v4.5"
        stroke="currentColor"
        strokeWidth="1.3"
        strokeLinecap="round"
      />
    </svg>
  );
}

export function DependencyModal({
  open,
  mode,
  missingDependencies,
  installMessage,
  progressById,
  isInstalling,
  canDefer,
  onInstall,
  onDefer,
  onClose,
}: DependencyModalProps) {
  const { isRendered, isClosing } = usePresenceTransition(
    open,
    PRESENCE_TRANSITION_MS,
  );

  if (!isRendered) return null;

  const copy = getDependencyPromptCopy(mode);

  const totalProgress =
    missingDependencies.length > 0
      ? Math.round(
          missingDependencies.reduce(
            (sum, id) => sum + (progressById[id] ?? 0),
            0,
          ) / missingDependencies.length,
        )
      : 0;

  const isModeWarn = mode !== "startup";

  return (
    <div
      className="modal-backdrop modal-summon-backdrop dm-backdrop"
      data-state={isClosing ? "closing" : "open"}
      onClick={onClose}
    >
      <div
        className="modal-panel modal-summon-panel dm-panel"
        data-state={isClosing ? "closing" : "open"}
        role="dialog"
        aria-modal="true"
        aria-label={copy.title}
        onClick={(e) => e.stopPropagation()}
      >
        {/* ── Header ──────────────────────────────────────────── */}
        <div className="dm-header">
          <div className="dm-header-icon" data-warn={isModeWarn}>
            <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
              {isModeWarn ? (
                <>
                  <path
                    d="M8 2L14.5 13.5H1.5L8 2z"
                    stroke="currentColor"
                    strokeWidth="1.4"
                    strokeLinejoin="round"
                  />
                  <path
                    d="M8 7v3M8 11.5v.5"
                    stroke="currentColor"
                    strokeWidth="1.4"
                    strokeLinecap="round"
                  />
                </>
              ) : (
                <>
                  <path
                    d="M8 2C4.7 2 2 4.7 2 8s2.7 6 6 6 6-2.7 6-6-2.7-6-6-6z"
                    stroke="currentColor"
                    strokeWidth="1.4"
                  />
                  <path
                    d="M8 7v4M8 5.5v.5"
                    stroke="currentColor"
                    strokeWidth="1.4"
                    strokeLinecap="round"
                  />
                </>
              )}
            </svg>
          </div>
          <div className="dm-header-text">
            <p className="dm-eyebrow">{copy.eyebrow}</p>
            <h2 className="dm-title">{copy.title}</h2>
          </div>
        </div>

        {/* ── Description ─────────────────────────────────────── */}
        <p className="dm-description">{copy.description}</p>

        {/* ── Dependency list ──────────────────────────────────── */}
        <div className="dm-section-label">
          {missingDependencies.length} module
          {missingDependencies.length !== 1 ? "s" : ""} required
        </div>
        <div className="dm-dep-list">
          {missingDependencies.map((id, i) => {
            const pct = progressById[id] ?? null;
            const started = pct !== null;
            const done = pct === 100;

            return (
              <div
                key={id}
                className={`dm-dep-row${done ? " dm-dep-row--done" : ""}`}
                style={{ animationDelay: `${i * 50}ms` }}
              >
                {/* Icon */}
                <div
                  className={`dm-dep-icon${done ? " dm-dep-icon--done" : ""}`}
                >
                  {done ? (
                    <svg width="11" height="11" viewBox="0 0 11 11" fill="none">
                      <path
                        d="M2 5.5l2.5 2.5 4.5-5"
                        stroke="currentColor"
                        strokeWidth="1.5"
                        strokeLinecap="round"
                        strokeLinejoin="round"
                      />
                    </svg>
                  ) : (
                    <DepIcon id={id} />
                  )}
                </div>

                {/* Name + progress */}
                <div className="dm-dep-body">
                  <div className="dm-dep-name-row">
                    <span className="dm-dep-name">{id}</span>
                    {started && !done && (
                      <span className="dm-dep-pct">{pct}%</span>
                    )}
                    {done && <span className="dm-dep-done-label">Done</span>}
                  </div>
                  {/* Per-dep progress track */}
                  <div className="dm-dep-track">
                    <div
                      className={`dm-dep-fill${done ? " dm-dep-fill--done" : ""}`}
                      style={{
                        width:
                          isInstalling || started
                            ? `${pct ?? (isInstalling ? 4 : 0)}%`
                            : "0%",
                        transition: "width 0.4s cubic-bezier(0.4, 0, 0.2, 1)",
                      }}
                    />
                  </div>
                </div>
              </div>
            );
          })}
        </div>

        {/* ── Overall progress strip ───────────────────────────── */}
        {(isInstalling || totalProgress > 0) && (
          <div className="dm-overall">
            <div className="dm-overall-header">
              <span className="dm-overall-label">
                {installMessage ?? (isInstalling ? "Installing…" : "Waiting")}
              </span>
              <span className="dm-overall-pct">{totalProgress}%</span>
            </div>
            <div className="dm-overall-track">
              <div
                className="dm-overall-fill"
                style={{
                  width: `${totalProgress || (isInstalling ? 3 : 0)}%`,
                  transition: "width 0.5s cubic-bezier(0.4, 0, 0.2, 1)",
                }}
              />
            </div>
          </div>
        )}

        {/* ── Actions ─────────────────────────────────────────── */}
        <div className="dm-actions">
          <button
            className="dm-btn-primary"
            onClick={onInstall}
            disabled={isInstalling}
          >
            {isInstalling ? (
              <>
                <span className="dm-spinner" />
                Installing…
              </>
            ) : (
              <>
                {copy.ctaLabel}
                <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
                  <path
                    d="M6 1v7M3 5.5L6 8.5l3-3"
                    stroke="currentColor"
                    strokeWidth="1.5"
                    strokeLinecap="round"
                    strokeLinejoin="round"
                  />
                  <path
                    d="M1 10.5h10"
                    stroke="currentColor"
                    strokeWidth="1.5"
                    strokeLinecap="round"
                  />
                </svg>
              </>
            )}
          </button>

          {canDefer ? (
            <button className="dm-btn-ghost" onClick={onDefer}>
              Defer for now
            </button>
          ) : (
            <button
              className="dm-btn-ghost"
              onClick={onClose}
              disabled={isInstalling}
            >
              Close
            </button>
          )}
        </div>
      </div>

      <style>{`
        .dm-backdrop {
          backdrop-filter: blur(6px);
          -webkit-backdrop-filter: blur(6px);
        }

        /* ── Panel ─────────────────────────────────────────────── */
        .dm-panel {
          width: min(460px, calc(100vw - 32px));
          padding: 0;
          border-radius: 16px;
          overflow: hidden;
          display: flex;
          flex-direction: column;
          animation: dmIn 0.25s cubic-bezier(0.4, 0, 0.2, 1) both;
        }
        @keyframes dmIn {
          from { opacity: 0; transform: translateY(10px) scale(0.98); }
          to   { opacity: 1; transform: translateY(0)    scale(1);    }
        }

        /* ── Header ────────────────────────────────────────────── */
        .dm-header {
          display: flex;
          align-items: flex-start;
          gap: 14px;
          padding: 24px 24px 0;
        }
        .dm-header-icon {
          width: 38px;
          height: 38px;
          border-radius: 10px;
          border: 1px solid color-mix(in srgb, var(--warning) 35%, transparent);
          background: color-mix(in srgb, var(--warning) 10%, transparent);
          color: var(--warning);
          display: flex;
          align-items: center;
          justify-content: center;
          flex-shrink: 0;
        }
        .dm-header-icon[data-warn="false"] {
          border-color: color-mix(in srgb, var(--info) 35%, transparent);
          background: color-mix(in srgb, var(--info) 10%, transparent);
          color: var(--info);
        }
        .dm-header-text {
          flex: 1;
          padding-top: 2px;
        }
        .dm-eyebrow {
          font-size: 10px;
          font-weight: 700;
          letter-spacing: 0.1em;
          text-transform: uppercase;
          color: var(--text-muted);
          margin-bottom: 4px;
          line-height: 1;
        }
        .dm-title {
          font-size: 18px;
          font-weight: 700;
          letter-spacing: -0.03em;
          color: var(--text-primary);
          line-height: 1.15;
        }

        /* ── Description ───────────────────────────────────────── */
        .dm-description {
          font-size: 12px;
          color: var(--text-secondary);
          line-height: 1.65;
          padding: 10px 24px 0;
        }

        /* ── Section label ─────────────────────────────────────── */
        .dm-section-label {
          font-size: 9px;
          font-weight: 700;
          letter-spacing: 0.1em;
          text-transform: uppercase;
          color: var(--text-muted);
          padding: 20px 24px 8px;
        }

        /* ── Dep list ──────────────────────────────────────────── */
        .dm-dep-list {
          display: flex;
          flex-direction: column;
          gap: 5px;
          padding: 0 24px;
        }
        .dm-dep-row {
          display: flex;
          align-items: center;
          gap: 12px;
          padding: 10px 13px;
          border-radius: 9px;
          border: 1px solid var(--border);
          background: var(--bg-input);
          animation: dmDepIn 0.2s ease both;
          transition: border-color 0.2s ease;
        }
        .dm-dep-row--done {
          border-color: color-mix(in srgb, var(--success) 30%, transparent);
          background: color-mix(in srgb, var(--success) 5%, var(--bg-input));
        }
        @keyframes dmDepIn {
          from { opacity: 0; transform: translateX(-6px); }
          to   { opacity: 1; transform: translateX(0);    }
        }

        .dm-dep-icon {
          width: 28px;
          height: 28px;
          border-radius: 7px;
          border: 1px solid var(--border);
          background: var(--bg-card);
          color: var(--text-muted);
          display: flex;
          align-items: center;
          justify-content: center;
          flex-shrink: 0;
          transition: background 0.2s ease, border-color 0.2s ease, color 0.2s ease;
        }
        .dm-dep-icon--done {
          color: var(--success);
          border-color: color-mix(in srgb, var(--success) 30%, transparent);
          background: color-mix(in srgb, var(--success) 10%, transparent);
        }

        .dm-dep-body {
          flex: 1;
          min-width: 0;
          display: flex;
          flex-direction: column;
          gap: 5px;
        }
        .dm-dep-name-row {
          display: flex;
          align-items: center;
          justify-content: space-between;
          gap: 8px;
        }
        .dm-dep-name {
          font-family: "JetBrains Mono", monospace;
          font-size: 11px;
          font-weight: 600;
          color: var(--text-primary);
          letter-spacing: 0.01em;
        }
        .dm-dep-pct {
          font-family: "JetBrains Mono", monospace;
          font-size: 10px;
          font-weight: 600;
          color: var(--accent);
        }
        .dm-dep-done-label {
          font-size: 10px;
          font-weight: 600;
          color: var(--success);
          letter-spacing: 0.03em;
        }

        /* Per-dep progress track */
        .dm-dep-track {
          height: 3px;
          border-radius: 2px;
          background: var(--border);
          overflow: hidden;
        }
        .dm-dep-fill {
          height: 100%;
          border-radius: 2px;
          background: var(--accent);
        }
        .dm-dep-fill--done {
          background: var(--success);
        }

        /* ── Overall progress ──────────────────────────────────── */
        .dm-overall {
          margin: 16px 24px 0;
          padding: 12px 14px;
          border-radius: 9px;
          border: 1px solid var(--border);
          background: var(--bg-input);
          display: flex;
          flex-direction: column;
          gap: 7px;
        }
        .dm-overall-header {
          display: flex;
          align-items: center;
          justify-content: space-between;
        }
        .dm-overall-label {
          font-family: "JetBrains Mono", monospace;
          font-size: 10px;
          font-weight: 500;
          color: var(--text-muted);
        }
        .dm-overall-pct {
          font-family: "JetBrains Mono", monospace;
          font-size: 10px;
          font-weight: 700;
          color: var(--text-secondary);
        }
        .dm-overall-track {
          height: 4px;
          border-radius: 2px;
          background: var(--border);
          overflow: hidden;
        }
        .dm-overall-fill {
          height: 100%;
          border-radius: 2px;
          background: var(--accent);
        }

        /* ── Actions ───────────────────────────────────────────── */
        .dm-actions {
          display: flex;
          align-items: center;
          gap: 8px;
          padding: 20px 24px 22px;
        }
        .dm-btn-primary {
          flex: 1;
          display: flex;
          align-items: center;
          justify-content: center;
          gap: 7px;
          padding: 10px 18px;
          border-radius: 9px;
          border: none;
          background: var(--accent);
          color: #fff;
          font: inherit;
          font-size: 12px;
          font-weight: 600;
          cursor: pointer;
          transition: background 0.15s ease, opacity 0.15s ease, transform 0.1s ease;
        }
        .dm-btn-primary:hover:not(:disabled) {
          background: var(--accent-hover);
        }
        .dm-btn-primary:active:not(:disabled) { transform: scale(0.98); }
        .dm-btn-primary:disabled {
          opacity: 0.45;
          cursor: not-allowed;
        }
        .dm-btn-ghost {
          display: inline-flex;
          align-items: center;
          padding: 10px 16px;
          border-radius: 9px;
          border: 1px solid var(--border);
          background: transparent;
          color: var(--text-secondary);
          font: inherit;
          font-size: 12px;
          font-weight: 500;
          cursor: pointer;
          transition: background 0.15s ease, border-color 0.15s ease, color 0.15s ease;
          white-space: nowrap;
        }
        .dm-btn-ghost:hover:not(:disabled) {
          background: var(--bg-hover);
          border-color: var(--border-strong);
          color: var(--text-primary);
        }
        .dm-btn-ghost:disabled {
          opacity: 0.4;
          cursor: not-allowed;
        }

        /* ── Spinner ───────────────────────────────────────────── */
        .dm-spinner {
          width: 12px;
          height: 12px;
          border: 2px solid rgba(255,255,255,0.3);
          border-top-color: #fff;
          border-radius: 50%;
          animation: dmSpin 0.65s linear infinite;
          flex-shrink: 0;
        }
        @keyframes dmSpin { to { transform: rotate(360deg); } }
      `}</style>
    </div>
  );
}
