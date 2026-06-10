import { useEffect, useRef } from "react";
import {
  ONBOARDING_MOTION_EASING,
  ONBOARDING_PANEL_TRANSITION_MS,
} from "./onboardingMotion";
import { usePresenceTransition } from "./usePresenceTransition";

interface OnboardingModalProps {
  open: boolean;
  licenseKey: string;
  onLicenseKeyChange: (value: string) => void;
  onVerify: () => void;
  onBypass: () => void;
  isVerifying: boolean;
  verificationError: string | null;
  verificationSuccess: boolean;
  embedded?: boolean;
  onExited?: () => void;
}

export function OnboardingModal({
  open,
  licenseKey,
  onLicenseKeyChange,
  onVerify,
  onBypass,
  isVerifying,
  verificationError,
  verificationSuccess,
  embedded = false,
  onExited,
}: OnboardingModalProps) {
  const { isRendered, isClosing } = usePresenceTransition(
    open,
    ONBOARDING_PANEL_TRANSITION_MS,
    onExited,
  );
  const inputRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    if (!isRendered || isClosing || verificationSuccess) return;
    const timer = window.setTimeout(() => {
      inputRef.current?.focus({ preventScroll: true });
    }, 0);
    return () => window.clearTimeout(timer);
  }, [isClosing, isRendered, verificationSuccess]);

  if (!isRendered) return null;

  const locked = isVerifying || verificationSuccess;

  const panel = (
    <div
      className={`ob-shell${verificationSuccess ? " ob-shell--success" : ""}`}
      data-state={isClosing ? "closing" : "open"}
    >
      <aside className="ob-left">
        <div className="ob-grid" aria-hidden="true">
          {Array.from({ length: 6 }).map((_, i) => (
            <div
              key={i}
              className="ob-grid-line ob-grid-line--v"
              style={{ left: `${(i + 1) * (100 / 7)}%` }}
            />
          ))}
          {Array.from({ length: 5 }).map((_, i) => (
            <div
              key={i}
              className="ob-grid-line ob-grid-line--h"
              style={{ top: `${(i + 1) * (100 / 6)}%` }}
            />
          ))}
        </div>

        <div className="ob-frames" aria-hidden="true">
          <div className="ob-frame ob-frame--h">
            <span className="ob-frame-label">16:9</span>
          </div>
          <div className="ob-arrow" aria-hidden="true">
            <svg width="20" height="20" viewBox="0 0 20 20" fill="none">
              <path
                d="M4 10h12M12 6l4 4-4 4"
                stroke="currentColor"
                strokeWidth="1.5"
                strokeLinecap="round"
                strokeLinejoin="round"
              />
            </svg>
          </div>
          <div className="ob-frame ob-frame--v">
            <span className="ob-frame-label">9:16</span>
          </div>
        </div>

        <div className="ob-wordmark">
          <img
            src="logo.png"
            alt=""
            className="ob-wordmark-logo"
            aria-hidden="true"
          />
          <div className="ob-wordmark-text">
            <span className="ob-wordmark-name">AspectShift</span>
            <span className="ob-wordmark-sub">HtoV</span>
          </div>
        </div>

        <p className="ob-tagline">
          Horizontal to vertical.
          <br />
          Frame&nbsp;perfect, every&nbsp;time.
        </p>
      </aside>

      <main className="ob-right">
        <div className="ob-right-inner">
          {!verificationSuccess ? (
            <>
              <div className="ob-right-header">
                <p className="ob-eyebrow">Welcome</p>
                <h1 className="ob-heading">Activate your copy</h1>
                <p className="ob-sub">
                  Enter your license key to unlock Pro features on this device.
                </p>
              </div>

              <div className="ob-field">
                <label className="ob-field-label" htmlFor="ob-key-input">
                  License Key
                </label>
                <input
                  id="ob-key-input"
                  ref={inputRef}
                  className={`ob-input${verificationError ? " ob-input--error" : ""}`}
                  placeholder="ASPECTSHIFT-XXXX-XXXX-XXXX"
                  value={licenseKey}
                  disabled={locked}
                  spellCheck={false}
                  autoComplete="off"
                  onChange={(e) => onLicenseKeyChange(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" && licenseKey.trim()) onVerify();
                  }}
                />
                {verificationError && (
                  <div className="ob-field-error">
                    <svg
                      width="11"
                      height="11"
                      viewBox="0 0 11 11"
                      fill="none"
                      style={{ flexShrink: 0, marginTop: 1 }}
                    >
                      <circle
                        cx="5.5"
                        cy="5.5"
                        r="4.5"
                        stroke="currentColor"
                        strokeWidth="1.2"
                      />
                      <path
                        d="M5.5 3.5v2.5M5.5 7.5v.3"
                        stroke="currentColor"
                        strokeWidth="1.3"
                        strokeLinecap="round"
                      />
                    </svg>
                    {verificationError}
                  </div>
                )}
              </div>

              <button
                className="ob-btn-primary"
                onClick={onVerify}
                disabled={locked || !licenseKey.trim()}
              >
                {isVerifying ? (
                  <>
                    <span className="ob-spinner" />
                    Verifying device...
                  </>
                ) : (
                  <>
                    Activate License
                    <svg width="13" height="13" viewBox="0 0 13 13" fill="none">
                      <path
                        d="M3 6.5h7M7 3.5l3 3-3 3"
                        stroke="currentColor"
                        strokeWidth="1.5"
                        strokeLinecap="round"
                        strokeLinejoin="round"
                      />
                    </svg>
                  </>
                )}
              </button>

              <div className="ob-divider">
                <span>or</span>
              </div>

              <button
                className="ob-btn-ghost"
                onClick={onBypass}
                disabled={locked}
              >
                Continue without a license
              </button>

              <p className="ob-footnote">
                You can register at any time from the License panel in Settings.
              </p>
            </>
          ) : (
            <div className="ob-success-state">
              <div className="ob-success-icon">
                <svg width="28" height="28" viewBox="0 0 28 28" fill="none">
                  <path
                    d="M7 14.5l5 5 9-10"
                    stroke="currentColor"
                    strokeWidth="2"
                    strokeLinecap="round"
                    strokeLinejoin="round"
                  />
                </svg>
              </div>
              <h2 className="ob-success-heading">You're all set</h2>
              <p className="ob-success-sub">
                License verified and bound to this device. Preparing your workspace...
              </p>
              <div className="ob-success-loader">
                <div className="ob-success-loader-bar" />
              </div>
            </div>
          )}
        </div>
      </main>
    </div>
  );

  return (
    <div
      className={embedded ? "ob-embedded-stage" : "ob-backdrop"}
      data-state={embedded ? undefined : isClosing ? "closing" : "open"}
    >
      {panel}

      <style>{`
        .ob-backdrop {
          position: fixed;
          inset: 0;
          z-index: 1000;
          display: flex;
          align-items: center;
          justify-content: center;
          padding: 16px;
          background: var(--glass-backdrop-bg);
          backdrop-filter: blur(var(--glass-backdrop-blur)) saturate(var(--glass-backdrop-saturation));
          -webkit-backdrop-filter: blur(var(--glass-backdrop-blur)) saturate(var(--glass-backdrop-saturation));
          animation: obFadeIn ${ONBOARDING_PANEL_TRANSITION_MS}ms ${ONBOARDING_MOTION_EASING} both;
          will-change: opacity;
        }

        .ob-backdrop[data-state="closing"] {
          animation-name: obFadeOut;
        }

        .ob-embedded-stage {
          display: flex;
          align-items: center;
          justify-content: center;
          width: 100%;
          min-height: 100%;
        }

        .ob-shell {
          display: flex;
          width: min(780px, calc(100vw - 32px));
          height: min(520px, calc(100vh - 32px));
          border-radius: 20px;
          border: 1px solid var(--border-strong);
          background: var(--bg-card);
          overflow: hidden;
          opacity: 0;
          transform: translateY(18px) scale(0.985);
          will-change: opacity, transform;
          animation: obShellIn ${ONBOARDING_PANEL_TRANSITION_MS}ms ${ONBOARDING_MOTION_EASING} both;
        }

        .ob-shell[data-state="closing"] {
          animation-name: obShellOut;
        }

        .ob-left {
          position: relative;
          width: 280px;
          flex-shrink: 0;
          background: var(--bg-secondary);
          border-right: 1px solid var(--border);
          display: flex;
          flex-direction: column;
          padding: 28px 24px;
          overflow: hidden;
        }

        .ob-grid {
          position: absolute;
          inset: 0;
          pointer-events: none;
        }

        .ob-grid-line {
          position: absolute;
          background: var(--border);
        }

        .ob-grid-line--v {
          top: 0;
          bottom: 0;
          width: 1px;
        }

        .ob-grid-line--h {
          left: 0;
          right: 0;
          height: 1px;
        }

        .ob-frames {
          position: relative;
          z-index: 1;
          display: flex;
          align-items: center;
          justify-content: center;
          gap: 10px;
          flex: 1;
        }

        .ob-frame {
          border: 1.5px solid var(--accent);
          border-radius: 5px;
          display: flex;
          align-items: flex-end;
          justify-content: center;
          padding-bottom: 6px;
          position: relative;
        }

        .ob-frame--h {
          width: 72px;
          height: 40px;
          animation: obFramePulse 3.5s ease-in-out infinite;
        }

        .ob-frame--v {
          width: 36px;
          height: 64px;
          animation: obFramePulse 3.5s ease-in-out infinite 0.4s;
        }

        @keyframes obFramePulse {
          0%,
          100% {
            opacity: 0.55;
          }
          50% {
            opacity: 1;
          }
        }

        .ob-frame-label {
          font-family: "JetBrains Mono", monospace;
          font-size: 9px;
          font-weight: 600;
          color: var(--accent);
          letter-spacing: 0.05em;
          opacity: 0.8;
        }

        .ob-arrow {
          color: var(--text-muted);
          display: flex;
          align-items: center;
          animation: obArrowSlide 2s ease-in-out infinite;
        }

        @keyframes obArrowSlide {
          0%,
          100% {
            transform: translateX(0);
            opacity: 0.5;
          }
          50% {
            transform: translateX(3px);
            opacity: 1;
          }
        }

        .ob-wordmark {
          position: relative;
          z-index: 1;
          display: flex;
          align-items: center;
          gap: 10px;
          margin-bottom: 14px;
        }

        .ob-wordmark-logo {
          width: 28px;
          height: 28px;
          border-radius: 7px;
          object-fit: contain;
        }

        .ob-wordmark-text {
          display: flex;
          flex-direction: column;
          gap: 1px;
        }

        .ob-wordmark-name {
          font-size: 13px;
          font-weight: 700;
          letter-spacing: -0.03em;
          color: var(--text-primary);
          line-height: 1;
        }

        .ob-wordmark-sub {
          font-family: "JetBrains Mono", monospace;
          font-size: 10px;
          font-weight: 500;
          color: var(--text-muted);
          letter-spacing: 0.04em;
        }

        .ob-tagline {
          position: relative;
          z-index: 1;
          font-size: 11px;
          line-height: 1.65;
          color: var(--text-muted);
          font-weight: 400;
        }

        .ob-right {
          flex: 1;
          min-width: 0;
          display: flex;
          align-items: center;
          justify-content: center;
          padding: 36px 40px;
          overflow-y: auto;
        }

        .ob-right-inner {
          width: 100%;
          max-width: 340px;
          display: flex;
          flex-direction: column;
          gap: 0;
        }

        .ob-right-header {
          margin-bottom: 28px;
        }

        .ob-eyebrow {
          font-size: 10px;
          font-weight: 700;
          letter-spacing: 0.12em;
          text-transform: uppercase;
          color: var(--accent);
          margin-bottom: 8px;
          line-height: 1;
        }

        .ob-heading {
          font-size: 22px;
          font-weight: 700;
          letter-spacing: -0.035em;
          color: var(--text-primary);
          line-height: 1.1;
          margin-bottom: 8px;
        }

        .ob-sub {
          font-size: 12px;
          color: var(--text-secondary);
          line-height: 1.6;
        }

        .ob-field {
          display: flex;
          flex-direction: column;
          gap: 6px;
          margin-bottom: 14px;
        }

        .ob-field-label {
          font-size: 10px;
          font-weight: 700;
          letter-spacing: 0.08em;
          text-transform: uppercase;
          color: var(--text-muted);
        }

        .ob-input {
          width: 100%;
          padding: 11px 14px;
          border-radius: 10px;
          border: 1px solid var(--border-strong);
          background: var(--bg-input);
          color: var(--text-primary);
          font-family: "JetBrains Mono", monospace;
          font-size: 12px;
          font-weight: 500;
          letter-spacing: 0.05em;
          outline: none;
          transition: border-color 0.15s ease, background 0.15s ease;
          -webkit-app-region: no-drag;
        }

        .ob-input::placeholder {
          color: var(--text-muted);
          letter-spacing: 0.04em;
        }

        .ob-input:focus {
          border-color: var(--accent);
          background: var(--bg-card);
        }

        .ob-input--error {
          border-color: color-mix(in srgb, var(--error) 60%, transparent);
        }

        .ob-input:disabled {
          opacity: 0.5;
          cursor: not-allowed;
        }

        .ob-field-error {
          display: flex;
          align-items: flex-start;
          gap: 6px;
          font-size: 11px;
          color: var(--error);
          line-height: 1.4;
          font-weight: 500;
        }

        .ob-btn-primary {
          width: 100%;
          display: flex;
          align-items: center;
          justify-content: center;
          gap: 8px;
          padding: 12px 20px;
          border-radius: 10px;
          border: none;
          background: var(--accent);
          color: #fff;
          font: inherit;
          font-size: 13px;
          font-weight: 600;
          letter-spacing: -0.01em;
          cursor: pointer;
          transition: background 0.15s ease, opacity 0.15s ease, transform 0.1s ease;
          margin-bottom: 16px;
          -webkit-app-region: no-drag;
        }

        .ob-btn-primary:hover:not(:disabled) {
          background: var(--accent-hover);
        }

        .ob-btn-primary:active:not(:disabled) {
          transform: scale(0.98);
        }

        .ob-btn-primary:disabled {
          opacity: 0.4;
          cursor: not-allowed;
        }

        .ob-spinner {
          width: 13px;
          height: 13px;
          border: 2px solid rgba(255, 255, 255, 0.3);
          border-top-color: #fff;
          border-radius: 50%;
          animation: obSpin 0.65s linear infinite;
          flex-shrink: 0;
        }

        @keyframes obSpin {
          to {
            transform: rotate(360deg);
          }
        }

        .ob-divider {
          display: flex;
          align-items: center;
          gap: 10px;
          margin-bottom: 16px;
        }

        .ob-divider::before,
        .ob-divider::after {
          content: "";
          flex: 1;
          height: 1px;
          background: var(--border);
        }

        .ob-divider span {
          font-size: 10px;
          font-weight: 600;
          color: var(--text-muted);
          text-transform: uppercase;
          letter-spacing: 0.08em;
        }

        .ob-btn-ghost {
          width: 100%;
          padding: 10px 16px;
          border-radius: 10px;
          border: 1px solid var(--border);
          background: transparent;
          color: var(--text-secondary);
          font: inherit;
          font-size: 12px;
          font-weight: 500;
          cursor: pointer;
          transition: background 0.15s ease, border-color 0.15s ease, color 0.15s ease;
          margin-bottom: 16px;
          -webkit-app-region: no-drag;
        }

        .ob-btn-ghost:hover:not(:disabled) {
          background: var(--bg-hover);
          border-color: var(--border-strong);
          color: var(--text-primary);
        }

        .ob-btn-ghost:disabled {
          opacity: 0.4;
          cursor: not-allowed;
        }

        .ob-footnote {
          font-size: 10px;
          color: var(--text-muted);
          text-align: center;
          line-height: 1.5;
        }

        .ob-success-state {
          display: flex;
          flex-direction: column;
          align-items: center;
          text-align: center;
          gap: 12px;
        }

        .ob-success-icon {
          width: 60px;
          height: 60px;
          border-radius: 50%;
          border: 1.5px solid color-mix(in srgb, var(--success) 40%, transparent);
          background: color-mix(in srgb, var(--success) 10%, transparent);
          display: flex;
          align-items: center;
          justify-content: center;
          color: var(--success);
          animation: obSuccessPop 0.4s cubic-bezier(0.34, 1.56, 0.64, 1) both 0.1s;
        }

        @keyframes obSuccessPop {
          from {
            transform: scale(0.6);
            opacity: 0;
          }
          to {
            transform: scale(1);
            opacity: 1;
          }
        }

        .ob-success-heading {
          font-size: 20px;
          font-weight: 700;
          letter-spacing: -0.03em;
          color: var(--text-primary);
        }

        .ob-success-sub {
          font-size: 12px;
          color: var(--text-secondary);
          line-height: 1.6;
          max-width: 260px;
        }

        .ob-success-loader {
          width: 120px;
          height: 2px;
          background: var(--border);
          border-radius: 2px;
          overflow: hidden;
          margin-top: 4px;
        }

        .ob-success-loader-bar {
          height: 100%;
          background: var(--success);
          border-radius: 2px;
          animation: obLoad 1.8s ease-in-out infinite;
        }

        @keyframes obLoad {
          0% {
            width: 0%;
            margin-left: 0%;
          }
          50% {
            width: 60%;
            margin-left: 20%;
          }
          100% {
            width: 0%;
            margin-left: 100%;
          }
        }

        @keyframes obShellIn {
          from {
            opacity: 0;
            transform: translateY(18px) scale(0.985);
          }
          to {
            opacity: 1;
            transform: translateY(0) scale(1);
          }
        }

        @keyframes obShellOut {
          from {
            opacity: 1;
            transform: translateY(0) scale(1);
          }
          to {
            opacity: 0;
            transform: translateY(10px) scale(0.99);
          }
        }

        @keyframes obFadeIn {
          from {
            opacity: 0;
          }
          to {
            opacity: 1;
          }
        }

        @keyframes obFadeOut {
          from {
            opacity: 1;
          }
          to {
            opacity: 0;
          }
        }

        @media (max-width: 600px) {
          .ob-left {
            display: none;
          }

          .ob-right {
            padding: 28px 24px;
          }

          .ob-shell {
            width: min(100vw - 24px, 540px);
            height: auto;
            min-height: min(520px, calc(100vh - 24px));
          }
        }

        @media (prefers-reduced-motion: reduce) {
          .ob-backdrop,
          .ob-shell,
          .ob-success-icon,
          .ob-success-loader-bar,
          .ob-frame--h,
          .ob-frame--v,
          .ob-arrow {
            animation: none !important;
          }

          .ob-backdrop {
            opacity: 1;
          }

          .ob-shell {
            opacity: 1;
            transform: none;
          }

          .ob-shell[data-state="closing"],
          .ob-backdrop[data-state="closing"] {
            opacity: 0;
          }
        }
      `}</style>
    </div>
  );
}
