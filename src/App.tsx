import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

type AspectRatio = "ratio9x16" | "ratio1x1" | "ratio4x5" | "ratio2x3";
type PlatformTarget = "youtube" | "instagram_reels" | "tiktok";
type QualityPreset = "draft" | "standard" | "high";
type OutputFormat = "mp4" | "mov" | "webm";

interface ConversionOptions {
  blur_background: boolean;
  blur_sigma: number;
  remove_audio: boolean;
  skip_existing: boolean;
  quality: QualityPreset;
  output_format: OutputFormat;
  logo: null;
  custom_encoding_enabled: boolean;
  crf: number | null;
  preset: string | null;
  audio_bitrate: string | null;
  platform_target: PlatformTarget | null;
}

function App() {
  const [input, setInput] = useState("");
  const [outputDir, setOutputDir] = useState("");
  const [ratio, setRatio] = useState<AspectRatio>("ratio9x16");
  const [options, setOptions] = useState<ConversionOptions>({
    blur_background: false,
    blur_sigma: 20.0,
    remove_audio: false,
    skip_existing: true,
    quality: "standard",
    output_format: "mp4",
    logo: null,
    custom_encoding_enabled: false,
    crf: 18,
    preset: "medium",
    audio_bitrate: "128k",
    platform_target: null,
  });

  const [status, setStatus] = useState("");

  const handleConvert = async () => {
    try {
      setStatus("Processing...");
      const result = await invoke("convert_to_ratio", {
        input,
        outputDir,
        ratio,
        options,
      });
      setStatus(`Success: ${JSON.stringify(result)}`);
    } catch (error) {
      setStatus(`Error: ${error}`);
    }
  };

  return (
    <div className="container">
      <h1>AspectShift HTOV</h1>

      <div className="row">
        <input
          type="text"
          placeholder="Input file path"
          value={input}
          onChange={(e) => setInput(e.target.value)}
        />
        <input
          type="text"
          placeholder="Output directory"
          value={outputDir}
          onChange={(e) => setOutputDir(e.target.value)}
        />
      </div>

      <div className="row">
        <label>Ratio:</label>
        <select value={ratio} onChange={(e) => setRatio(e.target.value as AspectRatio)}>
          <option value="ratio9x16">9:16</option>
          <option value="ratio1x1">1:1</option>
          <option value="ratio4x5">4:5</option>
          <option value="ratio2x3">2:3</option>
        </select>
      </div>

      <div className="row">
        <label>Platform Target:</label>
        <select 
          value={options.platform_target || ""} 
          onChange={(e) => setOptions({ ...options, platform_target: (e.target.value as PlatformTarget) || null })}
        >
          <option value="">None</option>
          <option value="youtube">YouTube</option>
          <option value="instagram_reels">Instagram Reels</option>
          <option value="tiktok">TikTok</option>
        </select>
      </div>

      <div className="row">
        <label>
          <input 
            type="checkbox" 
            checked={options.custom_encoding_enabled} 
            onChange={(e) => setOptions({ ...options, custom_encoding_enabled: e.target.checked })}
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
              onChange={(e) => setOptions({ ...options, crf: parseInt(e.target.value) })} 
            />
            <span>{options.crf}</span>
          </div>

          <div className="row">
            <label>Preset (Speed):</label>
            <select 
              value={options.preset || "medium"} 
              onChange={(e) => setOptions({ ...options, preset: e.target.value })}
            >
              <option value="slow">Slow</option>
              <option value="medium">Medium</option>
              <option value="fast">Fast</option>
            </select>
          </div>

          <div className="row">
            <label>Audio Bitrate:</label>
            <select 
              value={options.audio_bitrate || "128k"} 
              onChange={(e) => setOptions({ ...options, audio_bitrate: e.target.value })}
            >
              <option value="128k">128k</option>
              <option value="192k">192k</option>
              <option value="320k">320k</option>
            </select>
          </div>
        </>
      )}

      <div className="row">
        <button onClick={handleConvert}>Convert</button>
      </div>

      <div className="status">
        <p>{status}</p>
      </div>
    </div>
  );
}

export default App;
