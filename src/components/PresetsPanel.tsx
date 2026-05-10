import type { PlatformPreset, CustomPreset } from "../types/backend";
import type { JSX } from "react";

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
  youtube: <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M2.5 17a24.12 24.12 0 0 1 0-10 2 2 0 0 1 1.4-1.4 49.56 49.56 0 0 1 16.2 0A2 2 0 0 1 21.5 7a24.12 24.12 0 0 1 0 10 2 2 0 0 1-1.4 1.4 49.55 49.55 0 0 1-16.2 0A2 2 0 0 1 2.5 17"/><path d="m10 15 5-3-5-3z"/></svg>,
  shorts: <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M12 2v20"/><path d="m17 5-9.5 7 9.5 7"/></svg>,
  instagram: <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><rect width="20" height="20" x="2" y="2" rx="5" ry="5"/><path d="M16 11.37A4 4 0 1 1 12.63 8 4 4 0 0 1 16 11.37z"/><line x1="17.5" x2="17.51" y1="6.5" y2="6.5"/></svg>,
  reels: <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M2 12a10 10 0 1 0 18.38 5.46"/><path d="M12 2a10 10 0 0 1 8.38 15.46"/><path d="M2 12h20"/><path d="M12 2v20"/><path d="m16 8-4 4 4 4"/><path d="m8 16 4-4-4-4"/></svg>,
  tiktok: <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M9 12a4 4 0 1 0 4 4V4c0-2 2-2 2-2"/><path d="M12 12h1"/></svg>,
  twitter: <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M22 4s-.7 2.1-2 3.4c1.6 10-9.4 17.3-18 11.6 2.2.1 4.4-.6 6-2C3 15.5.5 9.6 3 5c2.2 2.6 5.6 4.1 9 4-.9-4.2 4-6.6 7-3.8 1.1 0 3-1.2 3-1.2z"/></svg>,
  reddit: <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M6 3h8a4 4 0 0 1 0 8H6"/><path d="M6 3v18"/><path d="M6 12h8l4 9"/></svg>,
  x: <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M4 4l16 16M4 20L20 4"/></svg>
};

interface Props {
  presets: DisplayPreset[];
  selectedPresetIds: string[];
  onToggle: (id: string) => void;
}

export function PresetsPanel({
  presets,
  selectedPresetIds,
  onToggle,
}: Props) {
  const builtinPresets = presets.filter((p) => p.isBuiltin);
  const customPresets = presets.filter((p) => p.isCustom);

  return (
    <div className="presets-panel">
      <div className="presets-header">
        <span className="section-title">Platform Presets</span>
        <span className="text-xs text-muted">Select up to 5</span>
      </div>

      {builtinPresets.length > 0 && (
        <div className="presets-grid">
          {builtinPresets.map((p) => {
          const isSelected = selectedPresetIds.includes(p.id);
          // Icon lookup: try exact id, then lowercase, then first word lowercase
          const iconKey =
            Object.keys(PLATFORM_ICONS).find(
              (k) =>
                p.id.toLowerCase().includes(k) ||
                p.name.toLowerCase().includes(k),
            ) ?? "";
          return (
            <div
              key={p.id}
              className={`preset-card${isSelected ? " selected" : ""}`}
              onClick={() => onToggle(p.id)}
              title={(p.source as PlatformPreset).description ?? p.name}
            >
              <div className="preset-card-name">
                <span className="preset-icon">{PLATFORM_ICONS[iconKey]}</span> {p.name}
              </div>
              <div className="preset-card-ratio">
                {p.ratioLabel} {p.resolution && <span className="text-muted">({p.resolution})</span>}
              </div>
            </div>
          );
          })}        </div>
      )}

      {customPresets.length > 0 && (
        <>
          <div
            className="settings-group-title"
            style={{
              marginTop: "var(--space-md)",
              marginBottom: "var(--space-2xs)",
            }}
          >
            Custom
          </div>
          <div className="presets-grid">
            {customPresets.map((p) => {
              const isSelected = selectedPresetIds.includes(p.id);
              return (
                <div
                  key={p.id}
                  className={`preset-card${isSelected ? " selected" : ""}`}
                  onClick={() => onToggle(p.id)}
                >
                  <div className="preset-card-name">✦ {p.name}</div>
                  <div className="preset-card-ratio">
                    {p.ratioLabel} {p.resolution && <span className="text-muted">({p.resolution})</span>}
                  </div>
                </div>
              );
            })}
          </div>
        </>
      )}

      {presets.length === 0 && (
        <div className="queue-empty" style={{ padding: "16px 0" }}>
          <div className="text-muted text-sm">No presets loaded</div>
        </div>
      )}
    </div>
  );
}
