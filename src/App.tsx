import { useState, useEffect, useMemo, useRef } from "react";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import "./App.css";

type AspectRatio =
  | "ratio9x16"
  | "ratio1x1"
  | "ratio4x5"
  | "ratio2x3"
  | "ratio16x9";

type QualityPreset = "draft" | "standard" | "high";
type OutputFormat = "mp4" | "mov" | "webm";
type LogoPosition = "top_left" | "top_right" | "bottom_left" | "bottom_right";

interface PlatformConfig {
  target_width: number;
  target_height: number;
  enforce_dimensions: boolean;
}

interface LogoOptions {
  enabled: boolean;
  position: LogoPosition;
  opacity: number;
  gap: number;
  scale: number;
  path: string | null;
}

interface VideoTransform {
  rotate: number; // 0, 90, 180, 270
  flip_h: boolean;
  flip_v: boolean;
}

interface ConversionOptions {
  blur_background: boolean;
  blur_sigma: number;
  remove_audio: boolean;
  generate_subtitles: boolean;
  burn_subtitles: boolean;
  skip_existing: boolean;
  quality: QualityPreset;
  output_format: OutputFormat;
  logo: LogoOptions | null;
  custom_encoding_enabled: boolean;
  crf: number | null;
  preset: string | null;
  audio_bitrate: string | null;
  transform?: VideoTransform | null;
}

interface VideoPreset {
  id: string;
  name: string;
  description: string | null;
  ratio: AspectRatio;
  options: ConversionOptions;
  logo_path: string | null;
  platform_config: PlatformConfig | null;
  is_builtin: boolean;
}

interface OrientationInfo {
  width: number;
  height: number;
  rotation: number;
  is_vertical: boolean;
  display_width: number;
  display_height: number;
}

const DEFAULT_OPTIONS: ConversionOptions = {
  blur_background: false,
  blur_sigma: 20.0,
  remove_audio: false,
  generate_subtitles: false,
  burn_subtitles: false,
  skip_existing: true,
  quality: "standard",
  output_format: "mp4",
  logo: null,
  custom_encoding_enabled: false,
  crf: 18,
  preset: "medium",
  audio_bitrate: "128k",
  transform: {
    rotate: 0,
    flip_h: false,
    flip_v: false,
  },
};

function App() {
  const [input, setInput] = useState("");
  const [outputDir, setOutputDir] = useState("");
  const [ratio, setRatio] = useState<AspectRatio>("ratio9x16");
  const [options, setOptions] = useState<ConversionOptions>(DEFAULT_OPTIONS);
  const [presets, setPresets] = useState<VideoPreset[]>([]);
  const [selectedPresetId, setSelectedPresetId] = useState<string>("");
  const [newPresetName, setNewPresetName] = useState("");
  const [orientation, setOrientation] = useState<OrientationInfo | null>(null);

  const [status, setStatus] = useState("");
  const videoRef = useRef<HTMLVideoElement>(null);

  useEffect(() => {
    loadPresets();
  }, []);

  useEffect(() => {
    if (input) {
      handleDetectOrientation();
    } else {
      setOrientation(null);
    }
  }, [input]);

  const loadPresets = async () => {
    try {
      const allPresets = await invoke<VideoPreset[]>("get_all_presets");
      setPresets(allPresets);
    } catch (error) {
      console.error("Failed to load presets:", error);
    }
  };

  const handleDetectOrientation = async () => {
    try {
      const info = await invoke<OrientationInfo>("detect_orientation", {
        filePath: input,
      });
      setOrientation(info);
    } catch (error) {
      console.error("Failed to detect orientation:", error);
    }
  };

  const selectedPreset = useMemo(
    () => presets.find((p) => p.id === selectedPresetId) || null,
    [presets, selectedPresetId],
  );

  const isDirty = useMemo(() => {
    if (!selectedPreset) return false;

    // Check ratio
    if (ratio !== selectedPreset.ratio) return true;

    // Check logo changes
    const logoChanged = (() => {
      const currentLogoEnabled = options.logo?.enabled || false;
      const presetLogoEnabled = !!selectedPreset.logo_path;

      if (currentLogoEnabled !== presetLogoEnabled) return true;
      if (currentLogoEnabled) {
        if (options.logo?.path !== selectedPreset.logo_path) return true;
      }
      return false;
    })();

    if (logoChanged) return true;

    // Check transform changes
    const transformChanged = JSON.stringify(options.transform) !== JSON.stringify(selectedPreset.options.transform || DEFAULT_OPTIONS.transform);
    if (transformChanged) return true;

    // Check key options
    const keysToCompare: (keyof ConversionOptions)[] = [
      "blur_background",
      "blur_sigma",
      "remove_audio",
      "generate_subtitles",
      "burn_subtitles",
      "skip_existing",
      "quality",
      "output_format",
      "custom_encoding_enabled",
      "crf",
      "preset",
      "audio_bitrate",
    ];

    for (const key of keysToCompare) {
      if (options[key] !== selectedPreset.options[key]) return true;
    }

    return false;
  }, [selectedPreset, ratio, options]);

  const isLocked = selectedPreset?.platform_config?.enforce_dimensions || false;

  const handlePickFile = async () => {
    try {
      const selected = await open({
        multiple: false,
        filters: [
          { name: "Video", extensions: ["mp4", "mov", "mkv", "avi", "webm"] },
        ],
      });
      if (selected && typeof selected === "string") {
        setInput(selected);
      }
    } catch (error) {
      setStatus(`Error picking file: ${error}`);
    }
  };

  const handlePickOutputDir = async () => {
    try {
      const selected = await open({
        multiple: false,
        directory: true,
      });
      if (selected && typeof selected === "string") {
        setOutputDir(selected);
      }
    } catch (error) {
      setStatus(`Error picking directory: ${error}`);
    }
  };

  const handlePresetChange = (presetId: string) => {
    setSelectedPresetId(presetId);
    if (!presetId) {
      setOptions(DEFAULT_OPTIONS);
      setRatio("ratio9x16");
      return;
    }

    const preset = presets.find((p) => p.id === presetId);
    if (preset) {
      setRatio(preset.ratio);

      const newOptions = { ...preset.options };
      if (preset.logo_path) {
        newOptions.logo = {
          enabled: true,
          position: "bottom_right",
          opacity: 1.0,
          gap: 20,
          scale: 0.15,
          path: preset.logo_path,
        };
      } else {
        newOptions.logo = null;
      }
      setOptions(newOptions);
    }
  };

  const handleSavePreset = async () => {
    if (!newPresetName) return;

    const preset: VideoPreset = {
      id: Date.now().toString(),
      name: newPresetName,
      description: "Custom preset",
      ratio,
      options,
      logo_path: options.logo?.enabled ? options.logo.path : null,
      platform_config: selectedPreset?.platform_config || null,
      is_builtin: false,
    };

    try {
      await invoke("save_preset", { preset });
      setNewPresetName("");
      loadPresets();
      setStatus("Preset saved!");
    } catch (error) {
      setStatus(`Error saving preset: ${error}`);
    }
  };

  const handleDeletePreset = async (id: string) => {
    try {
      await invoke("delete_preset", { id });
      loadPresets();
      if (selectedPresetId === id) {
        setSelectedPresetId("");
      }
    } catch (error) {
      setStatus(`Error deleting preset: ${error}`);
    }
  };

  const handleConvert = async () => {
    try {
      setStatus("Processing...");
      const result = await invoke("convert_to_ratio", {
        input,
        outputDir,
        ratio,
        options,
        platform_config: selectedPreset?.platform_config || null,
      });
      setStatus(`Success: ${JSON.stringify(result)}`);
    } catch (error) {
      setStatus(`Error: ${error}`);
    }
  };

  const getPreviewTransformStyle = () => {
    if (!options.transform) return {};
    const { rotate, flip_h, flip_v } = options.transform;
    let transform = `rotate(${rotate}deg)`;
    if (flip_h) transform += " scaleX(-1)";
    if (flip_v) transform += " scaleY(-1)";
    return { transform };
  };

  const getAspectRatioValue = (r: AspectRatio) => {
    switch (r) {
      case "ratio9x16": return 9 / 16;
      case "ratio1x1": return 1 / 1;
      case "ratio4x5": return 4 / 5;
      case "ratio2x3": return 2 / 3;
      case "ratio16x9": return 16 / 9;
      default: return 9 / 16;
    }
  };

  return (
    <div className="container">
      <h1>AspectShift HTOV</h1>

      <div className="main-layout">
        <div className="controls-panel">
          <div className="row">
            <div className="input-with-button">
              <input
                type="text"
                placeholder="Input file path"
                value={input}
                readOnly
              />
              <button onClick={handlePickFile}>Browse File</button>
            </div>
          </div>
          <div className="row">
            <div className="input-with-button">
              <input
                type="text"
                placeholder="Output directory"
                value={outputDir}
                readOnly
              />
              <button onClick={handlePickOutputDir}>Browse Folder</button>
            </div>
          </div>

          <div className="row">
            <label>Select Preset:</label>
            <select
              value={selectedPresetId}
              onChange={(e) => handlePresetChange(e.target.value)}
              className={isDirty ? "dirty" : ""}
            >
              <option value="">Manual / Default</option>
              <optgroup label="Built-in">
                {presets
                  .filter((p) => p.is_builtin)
                  .map((p) => (
                    <option key={p.id} value={p.id}>
                      {p.name}
                      {selectedPresetId === p.id && isDirty ? " (modified)" : ""}
                    </option>
                  ))}
              </optgroup>
              <optgroup label="Custom">
                {presets
                  .filter((p) => !p.is_builtin)
                  .map((p) => (
                    <option key={p.id} value={p.id}>
                      {p.name}
                      {selectedPresetId === p.id && isDirty ? " (modified)" : ""}
                    </option>
                  ))}
              </optgroup>
            </select>
            {selectedPresetId &&
              !presets.find((p) => p.id === selectedPresetId)?.is_builtin && (
                <button onClick={() => handleDeletePreset(selectedPresetId)}>
                  Delete
                </button>
              )}
          </div>

          {isDirty && (
            <div className="warning">
              ⚠️ Settings differ from preset.{" "}
              <button onClick={() => handlePresetChange(selectedPresetId)}>
                Reset
              </button>{" "}
              or{" "}
              <button onClick={() => setSelectedPresetId("")}>
                Switch to Custom
              </button>
            </div>
          )}

          <hr />

          <div className={`row ${isLocked ? "locked" : ""}`}>
            <label>Ratio:</label>
            <select
              value={ratio}
              onChange={(e) => setRatio(e.target.value as AspectRatio)}
              disabled={isLocked}
            >
              <option value="ratio9x16">9:16</option>
              <option value="ratio1x1">1:1</option>
              <option value="ratio4x5">4:5</option>
              <option value="ratio2x3">2:3</option>
              <option value="ratio16x9">16:9</option>
            </select>
          </div>

          <div className="transform-controls">
            <h3>Transform</h3>
            <div className="row">
              <button onClick={() => setOptions({
                ...options,
                transform: {
                  ...options.transform!,
                  rotate: (options.transform!.rotate + 90) % 360
                }
              })}>Rotate 90°</button>
              
              <label>
                <input
                  type="checkbox"
                  checked={options.transform?.flip_h || false}
                  onChange={(e) => setOptions({
                    ...options,
                    transform: {
                      ...options.transform!,
                      flip_h: e.target.checked
                    }
                  })}
                />
                Flip H
              </label>

              <label>
                <input
                  type="checkbox"
                  checked={options.transform?.flip_v || false}
                  onChange={(e) => setOptions({
                    ...options,
                    transform: {
                      ...options.transform!,
                      flip_v: e.target.checked
                    }
                  })}
                />
                Flip V
              </label>

              <button onClick={() => setOptions({
                ...options,
                transform: { rotate: 0, flip_h: false, flip_v: false }
              })}>Reset Transform</button>
            </div>
          </div>

          <hr />

          <div className="row">
            <label>
              <input
                type="checkbox"
                checked={options.blur_background}
                onChange={(e) =>
                  setOptions({
                    ...options,
                    blur_background: e.target.checked,
                  })
                }
              />
              Blur Background
            </label>
          </div>

          <div className="row">
            <label>
              <input
                type="checkbox"
                checked={options.generate_subtitles}
                onChange={(e) =>
                  setOptions({
                    ...options,
                    generate_subtitles: e.target.checked,
                  })
                }
              />
              Generate Subtitles
            </label>
          </div>

          <div className="row">
            <label>
              <input
                type="checkbox"
                checked={options.burn_subtitles}
                onChange={(e) =>
                  setOptions({
                    ...options,
                    burn_subtitles: e.target.checked,
                    generate_subtitles: e.target.checked
                      ? true
                      : options.generate_subtitles,
                  })
                }
              />
              Burn Subtitles
            </label>
          </div>

          <div className="row">
            <label>
              <input
                type="checkbox"
                checked={options.custom_encoding_enabled}
                onChange={(e) =>
                  setOptions({
                    ...options,
                    custom_encoding_enabled: e.target.checked,
                  })
                }
              />
              Enable Custom Encoding
            </label>
          </div>

          {options.custom_encoding_enabled && (
            <>
              <div className="row">
                <label>CRF (Quality):</label>
                <input
                  type="range"
                  min="0"
                  max="51"
                  value={options.crf || 18}
                  onChange={(e) =>
                    setOptions({ ...options, crf: parseInt(e.target.value) })
                  }
                />
                <span>{options.crf}</span>
              </div>

              <div className="row">
                <label>Preset (Speed):</label>
                <select
                  value={options.preset || "medium"}
                  onChange={(e) =>
                    setOptions({ ...options, preset: e.target.value })
                  }
                >
                  <option value="slow">Slow</option>
                  <option value="medium">Medium</option>
                  <option value="fast">Fast</option>
                </select>
              </div>
            </>
          )}
        </div>

        <div className="preview-panel">
          <h3>Preview</h3>
          <div 
            className="preview-container"
            style={{ 
              aspectRatio: getAspectRatioValue(ratio),
              backgroundColor: '#000',
              position: 'relative',
              overflow: 'hidden',
              width: '100%',
              maxWidth: '300px',
              margin: 'auto',
              border: '2px solid #444'
            }}
          >
            {input ? (
              <video
                key={input}
                ref={videoRef}
                src={convertFileSrc(input)}
                style={{
                  width: '100%',
                  height: '100%',
                  objectFit: options.blur_background ? 'cover' : 'contain',
                  ...getPreviewTransformStyle()
                }}
                autoPlay
                muted
                loop
                playsInline
              />
            ) : (
              <div className="preview-placeholder">No video selected</div>
            )}
            
            {options.blur_background && input && (
               <video
                 key={`${input}-blur`}
                 src={convertFileSrc(input)}
                 style={{
                   position: 'absolute',
                   top: 0,
                   left: 0,
                   width: '100%',
                   height: '100%',
                   objectFit: 'cover',
                   zIndex: -1,
                   filter: `blur(${options.blur_sigma}px)`,
                   opacity: 0.5,
                   ...getPreviewTransformStyle()
                 }}
                 autoPlay
                 muted
                 loop
                 playsInline
               />
            )}
          </div>
          <div className="info-text">
            {orientation && (
              <p>{orientation.display_width}x{orientation.display_height} ({orientation.is_vertical ? 'Vertical' : 'Horizontal'})</p>
            )}
          </div>
        </div>
      </div>

      <hr />

      <div className="row">
        <input
          type="text"
          placeholder="New preset name"
          value={newPresetName}
          onChange={(e) => setNewPresetName(e.target.value)}
        />
        <button onClick={handleSavePreset}>Save as Preset</button>
      </div>

      <div className="row">
        <button
          onClick={handleConvert}
          className={status === "Processing..." ? "loading" : ""}
        >
          {status === "Processing..." ? "Converting..." : "Convert"}
        </button>
      </div>

      <div className="status">
        <p>{status}</p>
      </div>
    </div>
  );
}

export default App;
