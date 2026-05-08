import React, { useMemo, useRef, useEffect, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import type {
  OrientationInfo,
  VideoEffectsSettings,
} from "../types/backend";

interface VideoCanvasProps {
  videoSrc: string;
  ratio: number; // Numerical ratio (w/h)
  effects: VideoEffectsSettings;
  orientation: OrientationInfo | null;
  showGuides?: boolean;
  showSafeFrames?: boolean;
}

const RATIO_LABELS: Record<string, string> = {
  "0.5625": "9:16",
  "1": "1:1",
  "0.8": "4:5",
  "0.6666666666666666": "2:3",
  "1.7777777777777777": "16:9",
};

export const VideoCanvas: React.FC<VideoCanvasProps> = ({
  videoSrc,
  ratio,
  effects,
  showGuides = true,
  showSafeFrames = true,
}) => {
  const containerRef = useRef<HTMLDivElement>(null);
  const [containerDims, setContainerDims] = useState({ width: 0, height: 0 });

  // Update container dims on resize
  useEffect(() => {
    if (!containerRef.current) return;
    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) {
        setContainerDims({
          width: entry.contentRect.width,
          height: entry.contentRect.height,
        });
      }
    });
    observer.observe(containerRef.current);
    return () => observer.disconnect();
  }, []);

  // Calculate the actual size of the canvas box within the container
  const canvasSize = useMemo(() => {
    const { width, height } = containerDims;
    if (width === 0 || height === 0) return { width: 0, height: 0 };

    const containerRatio = width / height;
    if (containerRatio > ratio) {
      // Container is wider than the target ratio -> height is the limiting factor
      return { width: height * ratio, height };
    } else {
      // Container is taller than the target ratio -> width is the limiting factor
      return { width, height: width / ratio };
    }
  }, [containerDims, ratio]);

  const transformStyle = useMemo(() => {
    if (!effects.transform) return {};
    const { rotate, flip_h, flip_v } = effects.transform;
    let t = `rotate(${rotate}deg)`;
    if (flip_h) t += " scaleX(-1)";
    if (flip_v) t += " scaleY(-1)";
    return { transform: t };
  }, [effects.transform]);

  const logoStyle = useMemo(() => {
    if (!effects.logo || !effects.logo.enabled || !effects.logo.path) return null;
    const { position, opacity, gap, scale } = effects.logo;

    // Logo scale is relative to the canvas width (mirroring backend)
    const logoWidth = canvasSize.width * scale;

    // Gap should also be scaled relative to the export size.
    // Assuming a standard 1080p height as reference if not specified,
    // but better yet, use the current th from backend logic.
    // For preview, let's just make it look proportional.
    // If the gap is 20px in a 1920x1080 (16:9), it's about 1% of width.
    // Let's just use the gap as-is but scale it by the ratio of canvasSize.width / 1080 (arbitrary base)
    // Actually, backend uses literal pixels.
    const scaledGap = (gap * canvasSize.width) / 1080;

    const style: React.CSSProperties = {
      position: "absolute",
      width: logoWidth,
      opacity,
      transition: "all 0.2s ease",
      zIndex: 10,
    };

    switch (position) {
      case "top_left":
        style.top = scaledGap;
        style.left = scaledGap;
        break;
      case "top_right":
        style.top = scaledGap;
        style.right = scaledGap;
        break;
      case "bottom_left":
        style.bottom = scaledGap;
        style.left = scaledGap;
        break;
      case "bottom_right":
        style.bottom = scaledGap;
        style.right = scaledGap;
        break;
    }

    return style;
  }, [effects.logo, canvasSize]);

  const showBlur = !!effects.blur;

  return (
    <div
      ref={containerRef}
      className="video-canvas-container"
      style={{
        width: "100%",
        height: "100%",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        position: "relative",
      }}
    >
      {videoSrc ? (
        <div
          className="video-canvas-box"
          style={{
            width: canvasSize.width,
            height: canvasSize.height,
            position: "relative",
            overflow: "hidden",
            backgroundColor: "#000",
            borderRadius: "4px",
            boxShadow: "0 10px 30px rgba(0,0,0,0.5)",
            transition: "width 0.3s ease, height 0.3s ease",
          }}
        >
          {/* Main Video Layer */}
          {showBlur ? (
            <>
              <video
                src={convertFileSrc(videoSrc)}
                className="canvas-video-blur"
                style={{
                  position: "absolute",
                  inset: 0,
                  width: "100%",
                  height: "100%",
                  objectFit: "cover",
                  filter: `blur(${effects.blurSigma ?? 20}px)`,
                  opacity: 0.6,
                  ...transformStyle,
                }}
                autoPlay
                muted
                loop
                playsInline
              />
              <video
                src={convertFileSrc(videoSrc)}
                className="canvas-video-fg"
                style={{
                  position: "absolute",
                  inset: 0,
                  width: "100%",
                  height: "100%",
                  objectFit: "contain",
                  zIndex: 2,
                  ...transformStyle,
                }}
                autoPlay
                muted
                loop
                playsInline
              />
            </>
          ) : (
            <video
              src={convertFileSrc(videoSrc)}
              className="canvas-video-main"
              style={{
                width: "100%",
                height: "100%",
                objectFit: "cover",
                ...transformStyle,
              }}
              autoPlay
              muted
              loop
              playsInline
            />
          )}

          {/* Logo Layer */}
          {effects.logo?.enabled && effects.logo.path && (
            <img
              src={convertFileSrc(effects.logo.path)}
              style={logoStyle!}
              alt="Logo"
            />
          )}

          {/* Subtitles Layer */}
          {(effects.generateSubtitles || effects.burnSubtitles) && (
            <div
              className="canvas-subtitles"
              style={{
                position: "absolute",
                bottom: "10%",
                left: "10%",
                right: "10%",
                textAlign: "center",
                color: "white",
                textShadow: "0 2px 4px rgba(0,0,0,0.8)",
                fontSize: Math.max(12, canvasSize.width / 20),
                fontWeight: 600,
                zIndex: 20,
                pointerEvents: "none",
              }}
            >
              [ Subtitles Preview ]
            </div>
          )}

          {/* Composition Guides */}
          {showGuides && (
            <div
              className="canvas-guides"
              style={{
                position: "absolute",
                inset: 0,
                pointerEvents: "none",
                zIndex: 30,
                opacity: 0.3,
              }}
            >
              <div
                style={{
                  position: "absolute",
                  left: "33.33%",
                  top: 0,
                  bottom: 0,
                  width: "1px",
                  borderLeft: "1px dashed white",
                }}
              />
              <div
                style={{
                  position: "absolute",
                  left: "66.66%",
                  top: 0,
                  bottom: 0,
                  width: "1px",
                  borderLeft: "1px dashed white",
                }}
              />
              <div
                style={{
                  position: "absolute",
                  top: "33.33%",
                  left: 0,
                  right: 0,
                  height: "1px",
                  borderTop: "1px dashed white",
                }}
              />
              <div
                style={{
                  position: "absolute",
                  top: "66.66%",
                  left: 0,
                  right: 0,
                  height: "1px",
                  borderTop: "1px dashed white",
                }}
              />
            </div>
          )}

          {/* Safe Frame Guides */}
          {showSafeFrames && (
            <div
              className="canvas-safe-frames"
              style={{
                position: "absolute",
                inset: 0,
                pointerEvents: "none",
                zIndex: 31,
                opacity: 0.2,
              }}
            >
              {/* 90% Safe Area */}
              <div
                style={{
                  position: "absolute",
                  inset: "5%",
                  border: "1px solid white",
                  borderRadius: "2px",
                }}
              />
              {/* 80% Safe Area */}
              <div
                style={{
                  position: "absolute",
                  inset: "10%",
                  border: "1px solid rgba(255,255,255,0.5)",
                  borderRadius: "2px",
                }}
              />
            </div>
          )}
          
          {/* Label Overlay */}
          <div style={{
            position: 'absolute',
            top: 10,
            left: 10,
            padding: '2px 6px',
            background: 'rgba(0,0,0,0.6)',
            color: 'white',
            fontSize: 10,
            borderRadius: 3,
            fontFamily: 'monospace',
            zIndex: 40
          }}>
            {RATIO_LABELS[ratio.toString()] || ratio.toFixed(2)}
          </div>
        </div>
      ) : (
        <div className="preview-empty">
          <div className="preview-empty-icon">🎬</div>
          <div className="preview-empty-text">No video selected</div>
        </div>
      )}
    </div>
  );
};
