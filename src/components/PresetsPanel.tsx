import type { PlatformPreset, CustomPreset } from "../types/backend";

// Normalised view of any preset for display purposes
export interface DisplayPreset {
  id: string;
  name: string;
  ratioLabel: string;
  isBuiltin: boolean;
  isCustom: boolean;
  /** The underlying typed preset, used when building OutputJob */
  source: PlatformPreset | CustomPreset;
}

interface Props {
  presets: DisplayPreset[];
  selectedPresetIds: string[];
  onToggle: (id: string) => void;
  icons: Record<string, string>;
}

export function PresetsPanel({
  presets,
  selectedPresetIds,
  onToggle,
  icons,
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
              Object.keys(icons).find(
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
                <input type="checkbox" checked={isSelected} readOnly />
                <div className="preset-card-name">
                  {icons[iconKey] || "◈"} {p.name}
                </div>
                <div className="preset-card-ratio">{p.ratioLabel}</div>
              </div>
            );
          })}
        </div>
      )}

      {customPresets.length > 0 && (
        <>
          <div
            className="settings-group-title"
            style={{ marginTop: 10, marginBottom: 4 }}
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
                  <input type="checkbox" checked={isSelected} readOnly />
                  <div className="preset-card-name">✦ {p.name}</div>
                  <div className="preset-card-ratio">{p.ratioLabel}</div>
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
