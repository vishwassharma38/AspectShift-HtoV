import { RATIO_DISPLAY } from "../App";

interface Props {
  presets: any[];
  selectedPresetIds: string[];
  onToggle: (id: string) => void;
  icons: Record<string, string>;
}

export function PresetsPanel({ presets, selectedPresetIds, onToggle, icons }: Props) {
  return (
    <div className="presets-panel">
      <div className="presets-header">
        <span className="section-title">Platform Presets</span>
        <span className="text-xs text-muted">Select up to 5</span>
      </div>
      <div className="presets-grid">
        {presets.map((p) => {
          const isSelected = selectedPresetIds.includes(p.id);
          return (
            <div
              key={p.id}
              className={`preset-card${isSelected ? " selected" : ""}`}
              onClick={() => onToggle(p.id)}
            >
              <input type="checkbox" checked={isSelected} readOnly />
              <div className="preset-card-name">
                {icons[p.id] || "◈"} {p.name}
              </div>
              <div className="preset-card-ratio">
                {(RATIO_DISPLAY as any)[p.ratio]}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
