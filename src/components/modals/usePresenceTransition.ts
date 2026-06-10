import { useEffect, useRef, useState } from "react";

export const PRESENCE_TRANSITION_MS = 380;

/**
 * Keeps a modal mounted long enough to play its exit animation before unmounting.
 */
export function usePresenceTransition(
  open: boolean,
  exitMs: number,
  onExited?: () => void,
) {
  const [isRendered, setIsRendered] = useState(open);
  const [isClosing, setIsClosing] = useState(false);
  const timerRef = useRef<number | null>(null);
  const onExitedRef = useRef(onExited);

  useEffect(() => {
    onExitedRef.current = onExited;
  }, [onExited]);

  useEffect(() => {
    if (timerRef.current !== null) {
      window.clearTimeout(timerRef.current);
      timerRef.current = null;
    }

    if (open) {
      setIsRendered(true);
      setIsClosing(false);
      return;
    }

    if (!isRendered) return;

    setIsClosing(true);
    timerRef.current = window.setTimeout(() => {
      setIsRendered(false);
      setIsClosing(false);
      onExitedRef.current?.();
      timerRef.current = null;
    }, exitMs);

    return () => {
      if (timerRef.current !== null) {
        window.clearTimeout(timerRef.current);
        timerRef.current = null;
      }
    };
  }, [exitMs, isRendered, open]);

  return {
    isRendered,
    isClosing,
  };
}
