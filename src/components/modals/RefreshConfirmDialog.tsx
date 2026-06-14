import {
  PRESENCE_TRANSITION_MS,
  usePresenceTransition,
} from "./usePresenceTransition";

interface RefreshConfirmDialogProps {
  open: boolean;
  activeProcessingCount: number;
  onConfirm: () => void;
  onCancel: () => void;
}

export function RefreshConfirmDialog({
  open,
  onConfirm,
  onCancel,
}: RefreshConfirmDialogProps) {
  const { isRendered, isClosing } = usePresenceTransition(
    open,
    PRESENCE_TRANSITION_MS,
  );

  if (!isRendered) return null;

  return (
    <div
      className="modal-backdrop modal-summon-backdrop refresh-confirm-backdrop"
      data-state={isClosing ? "closing" : "open"}
      onClick={onCancel}
    >
      <div
        className="modal-panel modal-summon-panel refresh-confirm-panel"
        data-state={isClosing ? "closing" : "open"}
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
        aria-labelledby="refresh-confirm-title"
      >
        <div className="refresh-confirm-header">
          <div className="refresh-confirm-icon" aria-hidden="true">
            <svg width="20" height="20" viewBox="0 0 20 20" fill="none">
              <path
                d="M10 2.6l7.2 12.8H2.8L10 2.6z"
                stroke="currentColor"
                strokeWidth="1.5"
                strokeLinejoin="round"
              />
              <path
                d="M10 7.2v4.2M10 13.7v.5"
                stroke="currentColor"
                strokeWidth="1.6"
                strokeLinecap="round"
              />
            </svg>
          </div>
          <div className="refresh-confirm-copy">
            <p className="refresh-confirm-eyebrow">Refresh Application</p>
            <h2 id="refresh-confirm-title" className="refresh-confirm-title">
              Are you sure you want to refresh the application?
            </h2>
            <p className="refresh-confirm-warning">
              Refreshing the application will cancel all active processing jobs
              and clear the queue.
            </p>
          </div>
        </div>

        <div className="refresh-confirm-footer">
          <button type="button" className="btn btn-danger" onClick={onConfirm}>
            Yes, Refresh
          </button>
          <button type="button" className="btn" onClick={onCancel}>
            No
          </button>
        </div>

        <style>{`
          .refresh-confirm-backdrop {
            background: var(--glass-backdrop-bg);
            backdrop-filter: blur(var(--glass-backdrop-blur)) saturate(var(--glass-backdrop-saturation));
            -webkit-backdrop-filter: blur(var(--glass-backdrop-blur)) saturate(var(--glass-backdrop-saturation));
          }

          .refresh-confirm-panel {
            width: min(500px, 100%);
            padding: 22px;
          }

          .refresh-confirm-header {
            display: flex;
            align-items: flex-start;
            gap: 14px;
          }

          .refresh-confirm-icon {
            width: 40px;
            height: 40px;
            border-radius: 12px;
            border: 1px solid var(--border);
            background: color-mix(in srgb, var(--warning) 12%, var(--bg-input));
            color: var(--warning);
            display: flex;
            align-items: center;
            justify-content: center;
            flex-shrink: 0;
          }

          .refresh-confirm-copy {
            flex: 1;
            min-width: 0;
            display: flex;
            flex-direction: column;
            gap: 7px;
          }

          .refresh-confirm-eyebrow {
            color: var(--text-muted);
            font-size: 10px;
            font-weight: 700;
            letter-spacing: 0.12em;
            text-transform: uppercase;
          }

          .refresh-confirm-title {
            color: var(--text-primary);
            font-size: 21px;
            line-height: 1.15;
            letter-spacing: 0;
          }

          .refresh-confirm-summary,
          .refresh-confirm-warning {
            color: var(--text-secondary);
            font-size: 13px;
            line-height: 1.55;
          }

          .refresh-confirm-warning {
            color: var(--text-primary);
          }

          .refresh-confirm-footer {
            display: flex;
            flex-wrap: wrap;
            justify-content: flex-end;
            gap: 10px;
            margin-top: 20px;
          }

          .refresh-confirm-footer .btn {
            min-width: 112px;
            height: 38px;
            border-radius: 10px;
            font-weight: 600;
          }

          @media (max-width: 560px) {
            .refresh-confirm-panel {
              width: min(100%, calc(100vw - 24px));
              padding: 18px;
            }

            .refresh-confirm-footer {
              justify-content: stretch;
            }

            .refresh-confirm-footer .btn {
              flex: 1;
              min-width: 0;
            }
          }
        `}</style>
      </div>
    </div>
  );
}
