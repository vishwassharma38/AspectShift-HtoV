import React, { useMemo, useRef, useEffect, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import type {
  OrientationInfo,
  PreviewRenderLayout,
  VideoEffectsSettings,
} from "../types/backend";
import {
  type FitMode,
  resolveVideoGeometry,
} from "../utils/resolvedVideoGeometry";

interface VideoCanvasProps {
  videoSrc: string;
  previewLayout: PreviewRenderLayout | null;
  effects: VideoEffectsSettings;
  orientation: OrientationInfo | null;
  previewVolume: number;
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
  previewLayout,
  effects,
  orientation,
  previewVolume,
  showGuides = true,
  showSafeFrames = true,
}) => {
  const containerRef = useRef<HTMLDivElement>(null);
  const [containerDims, setContainerDims] = useState({ width: 0, height: 0 });
  // Tracks whether the video element has decoded enough to display.
  // Reset to false whenever videoSrc changes so the box stays hidden
  // until canplay fires, preventing a flash of the first frame at the
  // wrong size.
  const [videoReady, setVideoReady] = useState(false);
  const showBlur = !!effects.blur;
  const mainVideoRef = useRef<HTMLVideoElement | null>(null);
  const foregroundVideoRef = useRef<HTMLVideoElement | null>(null);

  // Reset readiness every time the source changes.
  useEffect(() => {
    setVideoReady(false);
  }, [videoSrc]);

  useEffect(() => {
    const normalized = Math.max(0, Math.min(100, previewVolume)) / 100;
    const isMuted = normalized <= 0;
    const syncElement = (el: HTMLVideoElement | null) => {
      if (!el) return;
      el.volume = normalized;
      el.muted = isMuted;
    };
    syncElement(mainVideoRef.current);
    syncElement(foregroundVideoRef.current);
  }, [previewVolume, videoSrc, showBlur]);

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
    // layout=null means geometry is not yet known — return zero so the box
    // collapses to nothing while we wait for orientation data.
    if (width === 0 || height === 0 || !previewLayout)
      return { width: 0, height: 0 };

    const ratio = previewLayout.targetWidth / previewLayout.targetHeight;
    const containerRatio = width / height;
    if (containerRatio > ratio) {
      // Container is wider than the target ratio -> height is the limiting factor
      return { width: height * ratio, height };
    } else {
      // Container is taller than the target ratio -> width is the limiting factor
      return { width, height: width / ratio };
    }
  }, [containerDims, previewLayout]);

  const targetScale = useMemo(() => {
    if (!previewLayout || previewLayout.targetWidth <= 0) return 1;
    return canvasSize.width / previewLayout.targetWidth;
  }, [canvasSize.width, previewLayout]);

  const fgFrameSize = useMemo(() => {
    if (!previewLayout) return { width: 0, height: 0 };
    return {
      width: previewLayout.foregroundFrameWidth * targetScale,
      height: previewLayout.foregroundFrameHeight * targetScale,
    };
  }, [previewLayout, targetScale]);

  const resolvedCoverGeometry = useMemo(() => {
    if (!previewLayout) return null;
    const fitMode: FitMode =
      previewLayout.backgroundFit === "contain" ? "contain" : "cover";
    return resolveVideoGeometry({
      orientation,
      transform: effects.transform,
      targetAspectRatio: previewLayout.targetWidth / previewLayout.targetHeight,
      frameWidth: canvasSize.width,
      frameHeight: canvasSize.height,
      fitMode,
    });
  }, [previewLayout, orientation, effects.transform, canvasSize]);

  const resolvedForegroundGeometry = useMemo(() => {
    if (!previewLayout) return null;
    const fitMode: FitMode =
      previewLayout.foregroundFit === "contain" ? "contain" : "cover";
    return resolveVideoGeometry({
      orientation,
      transform: effects.transform,
      targetAspectRatio:
        previewLayout.foregroundFrameWidth / previewLayout.foregroundFrameHeight,
      frameWidth: fgFrameSize.width,
      frameHeight: fgFrameSize.height,
      fitMode,
    });
  }, [previewLayout, orientation, effects.transform, fgFrameSize]);

  const transformStyle = useMemo(() => {
    const rotate = resolvedCoverGeometry?.rotation ?? 0;
    const flipH = !!effects.transform?.flip_h;
    const flipV = !!effects.transform?.flip_v;
    let t = `translate(-50%, -50%) rotate(${rotate}deg)`;
    if (flipH) t += " scaleX(-1)";
    if (flipV) t += " scaleY(-1)";
    return {
      transform: t,
      transformOrigin: "center center",
    };
  }, [
    resolvedCoverGeometry?.rotation,
    effects.transform?.flip_h,
    effects.transform?.flip_v,
  ]);

  const logoStyle = useMemo(() => {
    if (!effects.logo || !effects.logo.enabled || !effects.logo.path)
      return null;
    const { position, opacity, gap } = effects.logo;

    if (!previewLayout) return null;
    if (previewLayout.logoWidth === null || previewLayout.logoGap === null) {
      return null;
    }
    const logoWidth = (previewLayout.logoWidth ?? 0) * targetScale;
    const scaledGap = (previewLayout.logoGap ?? gap) * targetScale;

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
  }, [effects.logo, previewLayout, targetScale]);

  const subtitleStyle = useMemo(() => {
    if (!previewLayout) return null;
    const subtitleScale = Math.max(0.001, canvasSize.height / previewLayout.subtitle.playResY);
    const fontSize = previewLayout.subtitle.fontSize * subtitleScale;
    const marginV = previewLayout.subtitle.marginV * subtitleScale;
    const marginH = previewLayout.subtitle.marginH * subtitleScale;
    const outlineWidth = previewLayout.subtitle.outline * subtitleScale;

    const style: React.CSSProperties = {
      position: "absolute",
      bottom: marginV,
      left: marginH,
      right: marginH,
      textAlign: "center",
      color: "white",
      fontSize: Math.max(12, fontSize),
      fontWeight: "bold",
      fontFamily: "var(--font-ui)",
      zIndex: 20,
      pointerEvents: "none",
      lineHeight: 1.2,
      // Replicate ASS outline with multiple shadows for better coverage
      textShadow: `
        -${outlineWidth}px -${outlineWidth}px 0 #000,
         ${outlineWidth}px -${outlineWidth}px 0 #000,
        -${outlineWidth}px  ${outlineWidth}px 0 #000,
         ${outlineWidth}px  ${outlineWidth}px 0 #000,
         0px ${outlineWidth}px 0 #000,
         0px -${outlineWidth}px 0 #000,
         ${outlineWidth}px 0px 0 #000,
        -${outlineWidth}px 0px 0 #000
      `,
    };

    return style;
  }, [previewLayout, canvasSize]);

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
        // layout===null means orientation invoke hasn't resolved yet.
        // Render nothing so the layout never snaps through a wrong aspect ratio.
        previewLayout !== null && resolvedCoverGeometry && (
          <div
            className="video-canvas-box"
            style={{
              width: canvasSize.width,
              height: canvasSize.height,
              position: "relative",
              overflow: "hidden",
              backgroundColor: "#000",
              borderRadius: "10px",
              // Only animate width/height after the video is ready to avoid
              // the layout-shift frame being visible during ratio transitions.
              transition: "width 0.3s ease, height 0.3s ease",
              // Fade the entire box in once the video can play. This keeps the
              // preview blank while metadata is loading without any layout jump.
              opacity: videoReady ? 1 : 0,
              transitionProperty: "width, height, opacity",
              transitionDuration: "0.3s, 0.3s, 0.25s",
              transitionTimingFunction: "ease, ease, ease-in",
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
                    left: "50%",
                    top: "50%",
                    width:
                      resolvedCoverGeometry.sourceWidth *
                      resolvedCoverGeometry.scale,
                    height:
                      resolvedCoverGeometry.sourceHeight *
                      resolvedCoverGeometry.scale,
                    objectFit: "fill",
                    filter: `blur(${previewLayout.blurSigma}px)`,
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
                  ref={foregroundVideoRef}
                  style={{
                    position: "absolute",
                    left: "50%",
                    top: "50%",
                    width:
                      (resolvedForegroundGeometry ?? resolvedCoverGeometry)
                        .sourceWidth *
                      (resolvedForegroundGeometry ?? resolvedCoverGeometry).scale,
                    height:
                      (resolvedForegroundGeometry ?? resolvedCoverGeometry)
                        .sourceHeight *
                      (resolvedForegroundGeometry ?? resolvedCoverGeometry).scale,
                    objectFit: "fill",
                    zIndex: 2,
                    ...transformStyle,
                  }}
                  autoPlay
                  loop
                  playsInline
                  onCanPlay={(e) => {
                    const normalized = Math.max(0, Math.min(100, previewVolume)) / 100;
                    const isMuted = normalized <= 0;
                    e.currentTarget.volume = normalized;
                    e.currentTarget.muted = isMuted;
                    setVideoReady(true);
                  }}
                />
              </>
            ) : (
              <video
                src={convertFileSrc(videoSrc)}
                className="canvas-video-main"
                ref={mainVideoRef}
                style={{
                  position: "absolute",
                  left: "50%",
                  top: "50%",
                  width:
                    resolvedCoverGeometry.sourceWidth *
                    resolvedCoverGeometry.scale,
                  height:
                    resolvedCoverGeometry.sourceHeight *
                    resolvedCoverGeometry.scale,
                  objectFit: "fill",
                  ...transformStyle,
                }}
                autoPlay
                loop
                playsInline
                onCanPlay={(e) => {
                  const normalized = Math.max(0, Math.min(100, previewVolume)) / 100;
                  const isMuted = normalized <= 0;
                  e.currentTarget.volume = normalized;
                  e.currentTarget.muted = isMuted;
                  setVideoReady(true);
                }}
              />
            )}

            {/* Logo Layer */}
            {effects.logo?.enabled && effects.logo.path && logoStyle && (
              <img
                src={convertFileSrc(effects.logo.path)}
                style={logoStyle}
                alt="Logo"
              />
            )}

            {/* Subtitles Layer */}
            {(effects.exportSubtitles || effects.burnSubtitles) && (
              <div
                className="canvas-subtitles"
                style={subtitleStyle!}
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
            <div
              style={{
                position: "absolute",
                top: 10,
                left: 10,
                padding: "2px 6px",
                background: "rgba(0,0,0,0.6)",
                color: "white",
                fontSize: 10,
                borderRadius: 3,
                fontFamily: "var(--font-mono)",
                zIndex: 40,
              }}
            >
              {RATIO_LABELS[
                (previewLayout.targetWidth / previewLayout.targetHeight).toString()
              ] ||
                (previewLayout.targetWidth / previewLayout.targetHeight).toFixed(2)}
            </div>
          </div>
        )
      ) : (
        <div className="preview-empty">
          <svg
            xmlns="http://www.w3.org/2000/svg"
            width="48"
            height="48"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.5"
            strokeLinecap="round"
            strokeLinejoin="round"
            className="lucide lucide-clapperboard-icon lucide-clapperboard"
            style={{ color: 'var(--accent)', opacity: 0.5, marginBottom: '10px' }}
          >
            <path d="m12.296 3.464 3.02 3.956" />
            <path d="M20.2 6 3 11l-.9-2.4c-.3-1.1.3-2.2 1.3-2.5l13.5-4c1.1-.3 2.2.3 2.5 1.3z" />
            <path d="M3 11h18v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
            <path d="m6.18 5.276 3.1 3.899" />
          </svg>
          <div className="preview-empty-text">No video selected</div>
        </div>
      )}
    </div>
  );
};
