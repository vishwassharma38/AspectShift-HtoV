import type { PlatformPreset, CustomPreset } from "../types/backend";
import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type JSX,
  type WheelEvent,
} from "react";

const PRESETS_PER_PAGE = 10;
const WHEEL_PAGE_COOLDOWN_MS = 320;

// Normalised view of any preset for display purposes
export interface DisplayPreset {
  id: string;
  name: string;
  ratioLabel: string;
  resolution?: string;
  isBuiltin: boolean;
  isCustom: boolean;
  /** The underlying typed preset, used when building OutputJob */
  source: PlatformPreset | CustomPreset;
}

const PLATFORM_ICONS: Record<string, JSX.Element> = {
  youtube: (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      width="14"
      height="14"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="M2.5 17a24.12 24.12 0 0 1 0-10 2 2 0 0 1 1.4-1.4 49.56 49.56 0 0 1 16.2 0A2 2 0 0 1 21.5 7a24.12 24.12 0 0 1 0 10 2 2 0 0 1-1.4 1.4 49.55 49.55 0 0 1-16.2 0A2 2 0 0 1 2.5 17" />
      <path d="m10 15 5-3-5-3z" />
    </svg>
  ),
  shorts: (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      width="14"
      height="14"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="M12 2v20" />
      <path d="m17 5-9.5 7 9.5 7" />
    </svg>
  ),
  instagram: (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      width="14"
      height="14"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <rect width="20" height="20" x="2" y="2" rx="5" ry="5" />
      <path d="M16 11.37A4 4 0 1 1 12.63 8 4 4 0 0 1 16 11.37z" />
      <line x1="17.5" x2="17.51" y1="6.5" y2="6.5" />
    </svg>
  ),
  reels: (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      width="14"
      height="14"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="M2 12a10 10 0 1 0 18.38 5.46" />
      <path d="M12 2a10 10 0 0 1 8.38 15.46" />
      <path d="M2 12h20" />
      <path d="M12 2v20" />
      <path d="m16 8-4 4 4 4" />
      <path d="m8 16 4-4-4-4" />
    </svg>
  ),
  tiktok: (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      width="14"
      height="14"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="M12.525.02c1.31-.02 2.61-.01 3.91-.02.08 1.53.63 3.09 1.75 4.17 1.12 1.11 2.7 1.62 4.24 1.79v4.03c-1.44-.05-2.89-.35-4.2-.97-.57-.26-1.1-.59-1.62-.93-.01 2.92.01 5.84-.02 8.75-.08 1.4-.54 2.79-1.35 3.94-1.31 1.92-3.58 3.17-5.91 3.21-1.43.08-2.86-.31-4.08-1.03-2.02-1.19-3.44-3.37-3.65-5.71-.02-.5-.03-1-.01-1.49.18-1.9 1.12-3.72 2.58-4.96 1.66-1.44 3.98-2.13 6.15-1.72.02 1.48-.04 2.96-.04 4.44-.99-.32-2.15-.23-3.02.37-.63.41-1.11 1.04-1.36 1.75-.21.51-.15 1.07-.14 1.61.24 1.64 1.82 3.02 3.5 2.87 1.12-.01 2.19-.66 2.77-1.61.19-.33.4-.67.41-1.06.1-1.79.06-3.57.07-5.36.01-4.03-.01-8.05.02-12.07z" />
    </svg>
  ),
  twitter: (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      width="14"
      height="14"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="M22 4s-.7 2.1-2 3.4c1.6 10-9.4 17.3-18 11.6 2.2.1 4.4-.6 6-2C3 15.5.5 9.6 3 5c2.2 2.6 5.6 4.1 9 4-.9-4.2 4-6.6 7-3.8 1.1 0 3-1.2 3-1.2z" />
    </svg>
  ),
  reddit: (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      width="14"
      height="14"
      viewBox="0 0 24 24"
      fill="currentColor"
    >
      <path d="M12 0C5.373 0 0 5.373 0 12c0 3.314 1.343 6.314 3.515 8.485l-2.286 2.286C.775 23.225 1.097 24 1.738 24H12c6.627 0 12-5.373 12-12S18.627 0 12 0Zm4.388 3.199c1.104 0 1.999.895 1.999 1.999 0 1.105-.895 2-1.999 2-.946 0-1.739-.657-1.947-1.539v.002c-1.147.162-2.032 1.15-2.032 2.341v.007c1.776.067 3.4.567 4.686 1.363.473-.363 1.064-.58 1.707-.58 1.547 0 2.802 1.254 2.802 2.802 0 1.117-.655 2.081-1.601 2.531-.088 3.256-3.637 5.876-7.997 5.876-4.361 0-7.905-2.617-7.998-5.87-.954-.447-1.614-1.415-1.614-2.538 0-1.548 1.255-2.802 2.803-2.802.645 0 1.239.218 1.712.585 1.275-.79 2.881-1.291 4.64-1.365v-.01c0-1.663 1.263-3.034 2.88-3.207.188-.911.993-1.595 1.959-1.595Zm-8.085 8.376c-.784 0-1.459.78-1.506 1.797-.047 1.016.64 1.429 1.426 1.429.786 0 1.371-.369 1.418-1.385.047-1.017-.553-1.841-1.338-1.841Zm7.406 0c-.786 0-1.385.824-1.338 1.841.047 1.017.634 1.385 1.418 1.385.785 0 1.473-.413 1.426-1.429-.046-1.017-.721-1.797-1.506-1.797Zm-3.703 4.013c-.974 0-1.907.048-2.77.135-.147.015-.241.168-.183.305.483 1.154 1.622 1.964 2.953 1.964 1.33 0 2.47-.81 2.953-1.964.057-.137-.037-.29-.184-.305-.863-.087-1.795-.135-2.769-.135Z" />
    </svg>
  ),
  x: (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      width="14"
      height="14"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="M4 4l16 16M4 20L20 4" />
    </svg>
  ),
};

const CUSTOM_PRESET_ICON = (
  <svg
    xmlns="http://www.w3.org/2000/svg"
    width="14"
    height="14"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth="2"
    strokeLinecap="round"
    strokeLinejoin="round"
    className="lucide lucide-save-icon lucide-save"
  >
    <path d="M15.2 3a2 2 0 0 1 1.4.6l3.8 3.8a2 2 0 0 1 .6 1.4V19a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z" />
    <path d="M17 21v-7a1 1 0 0 0-1-1H8a1 1 0 0 0-1 1v7" />
    <path d="M7 3v4a1 1 0 0 0 1 1h7" />
  </svg>
);

function getPresetIcon(preset: DisplayPreset): JSX.Element | undefined {
  if (preset.isCustom) return CUSTOM_PRESET_ICON;

  const iconKey =
    Object.keys(PLATFORM_ICONS).find(
      (k) =>
        preset.id.toLowerCase().includes(k) ||
        preset.name.toLowerCase().includes(k),
    ) ?? "";

  return PLATFORM_ICONS[iconKey];
}

function getPresetTitle(preset: DisplayPreset): string {
  if (preset.isCustom) return `${preset.name} (Custom preset)`;
  return (preset.source as PlatformPreset).description ?? preset.name;
}

interface Props {
  presets: DisplayPreset[];
  selectedPresetIds: string[];
  onToggle: (id: string) => void;
}

export function PresetsPanel({ presets, selectedPresetIds, onToggle }: Props) {
  const [pageIndex, setPageIndex] = useState(0);
  const previousPresetCountRef = useRef(presets.length);
  const wheelLockUntilRef = useRef(0);

  const totalPages = Math.max(1, Math.ceil(presets.length / PRESETS_PER_PAGE));
  const clampedPageIndex = Math.min(pageIndex, totalPages - 1);
  const visiblePresets = useMemo(() => {
    const start = clampedPageIndex * PRESETS_PER_PAGE;
    return presets.slice(start, start + PRESETS_PER_PAGE);
  }, [clampedPageIndex, presets]);

  useEffect(() => {
    setPageIndex((currentPage) => {
      const previousPresetCount = previousPresetCountRef.current;
      const lastPage = totalPages - 1;

      previousPresetCountRef.current = presets.length;

      if (
        presets.length > previousPresetCount &&
        presets.length > PRESETS_PER_PAGE
      ) {
        return lastPage;
      }

      return Math.min(currentPage, lastPage);
    });
  }, [presets.length, totalPages]);

  const handleWheel = (event: WheelEvent<HTMLDivElement>) => {
    if (totalPages <= 1 || event.deltaY === 0) return;

    event.preventDefault();

    const now = Date.now();
    if (now < wheelLockUntilRef.current) return;

    const direction = event.deltaY > 0 ? 1 : -1;
    const nextPage = Math.max(
      0,
      Math.min(totalPages - 1, clampedPageIndex + direction),
    );

    if (nextPage === clampedPageIndex) return;

    wheelLockUntilRef.current = now + WHEEL_PAGE_COOLDOWN_MS;
    setPageIndex(nextPage);
  };

  return (
    <div className="presets-panel" onWheel={handleWheel}>
      <div className="presets-header">
        <span className="section-title">Presets</span>
        <span className="text-xs text-muted">
          Select up to 5
          {totalPages > 1 && (
            <span className="presets-page-indicator">
              {clampedPageIndex + 1} / {totalPages}
            </span>
          )}
        </span>
      </div>

      {presets.length > 0 && (
        <div className="presets-grid">
          {visiblePresets.map((p) => {
            const isSelected = selectedPresetIds.includes(p.id);
            const icon = getPresetIcon(p);

            return (
              <div
                key={p.id}
                className={`preset-card${isSelected ? " selected" : ""}`}
                onClick={() => onToggle(p.id)}
                title={getPresetTitle(p)}
              >
                <div className="preset-card-name">
                  <span className="preset-icon">{icon}</span> {p.name}
                </div>
                <div className="preset-card-ratio">
                  {p.ratioLabel}{" "}
                  {p.resolution && (
                    <span className="text-muted">({p.resolution})</span>
                  )}
                </div>
              </div>
            );
          })}
        </div>
      )}

      {presets.length === 0 && (
        <div className="queue-empty" style={{ padding: "16px 0" }}>
          <div className="text-muted text-sm">No presets loaded</div>
        </div>
      )}
    </div>
  );
}
