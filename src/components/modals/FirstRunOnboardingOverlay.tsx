import { type ReactNode } from "react";
import {
  ONBOARDING_MOTION_EASING,
  ONBOARDING_OVERLAY_TRANSITION_MS,
} from "./onboardingMotion";
import { usePresenceTransition } from "./usePresenceTransition";

interface FirstRunOnboardingOverlayProps {
  open: boolean;
  onExited?: () => void;
  children: ReactNode;
}

export function FirstRunOnboardingOverlay({
  open,
  onExited,
  children,
}: FirstRunOnboardingOverlayProps) {
  const { isRendered, isClosing } = usePresenceTransition(
    open,
    ONBOARDING_OVERLAY_TRANSITION_MS,
    onExited,
  );

  if (!isRendered) return null;

  return (
    <div
      className="fr-overlay"
      data-state={isClosing ? "closing" : "open"}
      aria-hidden={!open}
    >
      <div className="fr-overlay-scrim" />
      <div className="fr-overlay-stage">{children}</div>

      <style>{`
        .fr-overlay {
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
          opacity: 1;
          will-change: opacity;
        }

        .fr-overlay[data-state="closing"] {
          animation: frOverlayOut ${ONBOARDING_OVERLAY_TRANSITION_MS}ms ${ONBOARDING_MOTION_EASING} both;
        }

        .fr-overlay-scrim {
          position: absolute;
          inset: 0;
          background: radial-gradient(
            circle at top,
            rgba(255, 255, 255, 0.03),
            rgba(0, 0, 0, 0)
          );
          pointer-events: none;
        }

        .fr-overlay-stage {
          position: relative;
          z-index: 1;
          display: flex;
          align-items: center;
          justify-content: center;
          width: 100%;
          min-height: 100%;
        }

        @keyframes frOverlayOut {
          from {
            opacity: 1;
          }
          to {
            opacity: 0;
          }
        }

        @media (prefers-reduced-motion: reduce) {
          .fr-overlay {
            animation: none !important;
            opacity: 1;
          }
          .fr-overlay[data-state="closing"] {
            opacity: 0;
          }
        }
      `}</style>
    </div>
  );
}
