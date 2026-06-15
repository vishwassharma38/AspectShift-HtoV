import { useState } from "react";
import type { AppDepsState, DependencyId } from "../../types/backend";
import {
  getDependencyHealthSummary,
  getManagedDependencyReports,
} from "../../services/dependencyManager";
import { formatBuildModeLabel } from "../../utils/buildMetadata";
import type { ProductEdition } from "../../utils/productEdition";
import { HelpSettingsPanel } from "./HelpSettingsPanel";
import { LicenseSettingsPanel } from "./LicenseSettingsPanel";

interface SettingsOverlayProps {
  open: boolean;
  depsState: AppDepsState | null;
  dependencyOperation:
    | "idle"
    | "downloading"
    | "redownloading"
    | "checking"
    | "verifying"
    | "completed"
    | "failed";
  depsInstallMessage: string | null;
  aboutMetadata: {
    appName: string;
    appVersion: string;
    tauriVersion: string;
    identifier: string;
    buildMode: string;
  };
  edition: ProductEdition;
  onClose: () => void;
  onInstallMissing: () => void;
  onForceReinstall: () => void;
  onRescan: () => void;
  missingSubtitleDependencies: DependencyId[];
}

type SettingsTab = "dependencies" | "about" | "license" | "help";

const NAV_ITEMS: { id: SettingsTab; label: string; icon: string }[] = [
  { id: "dependencies", label: "Dependencies", icon: "⬡" },
  { id: "about", label: "About", icon: "◎" },
];

function LicenseTabIcon() {
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M12 3v18" />
      <path d="m19 8 3 8a5 5 0 0 1-6 0zV7" />
      <path d="M3 7h1a17 17 0 0 0 8-2 17 17 0 0 0 8 2h1" />
      <path d="m5 8 3 8a5 5 0 0 1-6 0zV7" />
      <path d="M7 21h10" />
    </svg>
  );
}

function AboutTabIcon() {
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <circle cx="12" cy="12" r="10" />
      <path d="M12 16v-4" />
      <path d="M12 8h.01" />
    </svg>
  );
}

function DependenciesTabIcon() {
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M17 19a1 1 0 0 1-1-1v-2a2 2 0 0 1 2-2h2a2 2 0 0 1 2 2v2a1 1 0 0 1-1 1z" />
      <path d="M17 21v-2" />
      <path d="M19 14V6.5a1 1 0 0 0-7 0v11a1 1 0 0 1-7 0V10" />
      <path d="M21 21v-2" />
      <path d="M3 5V3" />
      <path d="M4 10a2 2 0 0 1-2-2V6a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v2a2 2 0 0 1-2 2z" />
      <path d="M7 5V3" />
    </svg>
  );
}

function HelpTabIcon() {
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <circle cx="12" cy="12" r="10" />
      <path d="m4.93 4.93 4.24 4.24" />
      <path d="m14.83 9.17 4.24-4.24" />
      <path d="m14.83 14.83 4.24 4.24" />
      <path d="m9.17 14.83-4.24 4.24" />
      <circle cx="12" cy="12" r="4" />
    </svg>
  );
}

export function SettingsOverlay({
  open,
  depsState,
  dependencyOperation,
  depsInstallMessage,
  aboutMetadata,
  edition,
  onClose,
  onInstallMissing,
  onForceReinstall,
  onRescan,
  missingSubtitleDependencies,
}: SettingsOverlayProps) {
  const health = getDependencyHealthSummary(depsState);
  const managedDependencies = getManagedDependencyReports(depsState);
  const [activeTab, setActiveTab] = useState<SettingsTab>("dependencies");

  const allReady = missingSubtitleDependencies.length === 0;
  const isScanning = health.scanStatus === "scanning";
  const operationActive =
    dependencyOperation === "downloading" ||
    dependencyOperation === "redownloading" ||
    dependencyOperation === "checking" ||
    dependencyOperation === "verifying" ||
    isScanning;
  const isDownloading =
    dependencyOperation === "downloading" ||
    dependencyOperation === "verifying";
  const isRedownloading = dependencyOperation === "redownloading";
  const isChecking = dependencyOperation === "checking" || isScanning;
  const healthPct =
    health.totalCount > 0
      ? Math.round((health.readyCount / health.totalCount) * 100)
      : 0;

  return (
    <div className={`view-overlay${open ? " open" : ""}`} aria-hidden={!open}>
      <div className="view-overlay-scrim" onClick={onClose} />

      <section className="so-panel">
        {/* ── Left nav rail ─────────────────────────────────── */}
        <aside className="so-rail">
          <div className="so-rail-top">
            <div className="so-rail-wordmark">
              <span className="so-rail-icon">⚙</span>
              <span className="so-rail-label">Settings</span>
            </div>
          </div>

          <nav className="so-nav">
            {NAV_ITEMS.map((item) => (
              <button
                key={item.id}
                className={`so-nav-item${activeTab === item.id ? " active" : ""}`}
                onClick={() => setActiveTab(item.id)}
                type="button"
                aria-pressed={activeTab === item.id}
              >
                <span className="so-nav-icon">
                  {item.id === "dependencies" ? (
                    <DependenciesTabIcon />
                  ) : item.id === "about" ? (
                    <AboutTabIcon />
                  ) : (
                    item.icon
                  )}
                </span>
                <span className="so-nav-text">{item.label}</span>
                {item.id === "dependencies" && !allReady && (
                  <span className="so-nav-dot" />
                )}
              </button>
            ))}
            <button
              className={`so-nav-item${activeTab === "license" ? " active" : ""}`}
              onClick={() => setActiveTab("license")}
              type="button"
              aria-pressed={activeTab === "license"}
            >
              <span className="so-nav-icon">
                <LicenseTabIcon />
              </span>
              <span className="so-nav-text">License</span>
            </button>
            <button
              className={`so-nav-item${activeTab === "help" ? " active" : ""}`}
              onClick={() => setActiveTab("help")}
              type="button"
              aria-pressed={activeTab === "help"}
            >
              <span className="so-nav-icon">
                <HelpTabIcon />
              </span>
              <span className="so-nav-text">Help</span>
            </button>
          </nav>

          <div className="so-rail-bottom">
            <button
              className="so-close-btn"
              onClick={onClose}
              aria-label="Close settings"
              type="button"
            >
              <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
                <path
                  d="M1 1l12 12M13 1L1 13"
                  stroke="currentColor"
                  strokeWidth="1.5"
                  strokeLinecap="round"
                />
              </svg>
              <span>Close</span>
            </button>
          </div>
        </aside>

        {/* ── Main content ──────────────────────────────────── */}
        <div className="so-body">
          {activeTab === "dependencies" && (
            <div className="so-content">
              {/* Page header */}
              <div className="so-page-header">
                <div>
                  <p className="so-eyebrow">Manage</p>
                  <h2 className="so-page-title">Dependencies</h2>
                </div>
                <div
                  className="so-health-ring-wrap"
                  title={`${healthPct}% healthy`}
                >
                  <svg
                    className="so-health-ring"
                    viewBox="0 0 44 44"
                    width="44"
                    height="44"
                  >
                    <circle
                      cx="22"
                      cy="22"
                      r="18"
                      fill="none"
                      stroke="var(--border)"
                      strokeWidth="3"
                    />
                    <circle
                      cx="22"
                      cy="22"
                      r="18"
                      fill="none"
                      stroke={allReady ? "var(--success)" : "var(--warning)"}
                      strokeWidth="3"
                      strokeDasharray={`${2 * Math.PI * 18}`}
                      strokeDashoffset={`${2 * Math.PI * 18 * (1 - healthPct / 100)}`}
                      strokeLinecap="round"
                      transform="rotate(-90 22 22)"
                      style={{
                        transition:
                          "stroke-dashoffset 0.6s cubic-bezier(0.4,0,0.2,1)",
                      }}
                    />
                  </svg>
                  <span
                    className="so-health-pct"
                    style={{
                      color: allReady ? "var(--success)" : "var(--warning)",
                    }}
                  >
                    {healthPct}
                  </span>
                </div>
              </div>

              {/* Status strip */}
              <div className="so-status-strip">
                <div className="so-stat">
                  <span className="so-stat-label">Status</span>
                  <span
                    className={`so-stat-value ${allReady ? "so-stat-ok" : "so-stat-warn"}`}
                  >
                    {allReady
                      ? "Ready"
                      : `${missingSubtitleDependencies.length} missing`}
                  </span>
                </div>
                <div className="so-stat-divider" />
                <div className="so-stat">
                  <span className="so-stat-label">Installed</span>
                  <span className="so-stat-value">
                    {health.readyCount}
                    <span className="so-stat-total">
                      {" "}
                      / {health.totalCount || 2}
                    </span>
                  </span>
                </div>
                <div className="so-stat-divider" />
                <div className="so-stat">
                  <span className="so-stat-label">Scan</span>
                  <span className="so-stat-value">{health.scanStatus}</span>
                </div>
                <div className="so-stat-divider" />
                <div className="so-stat">
                  <span className="so-stat-label">Source</span>
                  <span className="so-stat-value">
                    {health.scanSource ?? "n/a"}
                  </span>
                </div>
                <div className="so-stat-divider" />
                <div className="so-stat">
                  <span className="so-stat-label">Updated</span>
                  <span className="so-stat-value">
                    {health.lastFullScanAt
                      ? new Date(health.lastFullScanAt).toLocaleString([], {
                          month: "short",
                          day: "2-digit",
                          hour: "2-digit",
                          minute: "2-digit",
                        })
                      : "Pending"}
                  </span>
                </div>
              </div>

              {/* Dependency list */}
              <div className="so-section-label">Version Specs</div>
              <div className="so-dep-list">
                {managedDependencies.map((dep, i) => {
                  const isReady = dep.status.status === "ready";
                  return (
                    <div
                      key={dep.id}
                      className={`so-dep-row${isReady ? "" : " so-dep-row--warn"}`}
                      style={{ animationDelay: `${i * 40}ms` }}
                    >
                      <div className="so-dep-indicator">
                        <span
                          className="so-dep-dot"
                          style={{
                            background: isReady
                              ? "var(--success)"
                              : "var(--warning)",
                            boxShadow: `0 0 0 4px ${isReady ? "color-mix(in srgb, var(--success) 16%, transparent)" : "color-mix(in srgb, var(--warning) 16%, transparent)"}`,
                          }}
                        />
                      </div>
                      <div className="so-dep-info">
                        <span className="so-dep-name">{dep.name}</span>
                        <span className="so-dep-versions">
                          <span className="so-dep-version-item">
                            <span className="so-dep-version-label">
                              expected
                            </span>
                            <code className="so-dep-version-val">
                              {dep.expectedVersion ?? "n/a"}
                            </code>
                          </span>
                          <span className="so-dep-version-sep">·</span>
                          <span className="so-dep-version-item">
                            <span className="so-dep-version-label">
                              installed
                            </span>
                            <code className="so-dep-version-val">
                              {dep.installedVersion ?? dep.version ?? "n/a"}
                            </code>
                          </span>
                        </span>
                      </div>
                      <span
                        className={`badge ${isReady ? "badge-success" : "badge-warning"}`}
                      >
                        {dep.status.status}
                      </span>
                    </div>
                  );
                })}

                {managedDependencies.length === 0 && (
                  <div className="so-dep-empty">
                    <span>No dependencies found</span>
                  </div>
                )}
              </div>

              {/* Actions */}
              <div className="so-section-label" style={{ marginTop: 24 }}>
                Controls
              </div>
              <div className="so-actions">
                <button
                  className="so-action-btn so-action-btn--primary"
                  onClick={onInstallMissing}
                  disabled={operationActive || allReady}
                  type="button"
                >
                  {isDownloading ? (
                    <span className="so-spinner" />
                  ) : (
                    <svg width="13" height="13" viewBox="0 0 13 13" fill="none">
                      <path
                        d="M6.5 1v8M3 6l3.5 3.5L10 6"
                        stroke="currentColor"
                        strokeWidth="1.5"
                        strokeLinecap="round"
                        strokeLinejoin="round"
                      />
                      <path
                        d="M1 11h11"
                        stroke="currentColor"
                        strokeWidth="1.5"
                        strokeLinecap="round"
                      />
                    </svg>
                  )}
                  {isDownloading
                    ? "Downloading Dependencies..."
                    : "Download Missing Dependencies"}
                </button>

                <button
                  className="so-action-btn"
                  onClick={onForceReinstall}
                  disabled={operationActive}
                  type="button"
                >
                  {isRedownloading ? (
                    <span className="so-spinner so-spinner--dark" />
                  ) : (
                    <svg width="13" height="13" viewBox="0 0 13 13" fill="none">
                      <path
                        d="M11 2.5A5.5 5.5 0 1 0 12 6.5"
                        stroke="currentColor"
                        strokeWidth="1.5"
                        strokeLinecap="round"
                      />
                      <path
                        d="M11 2.5V0M11 2.5H8.5"
                        stroke="currentColor"
                        strokeWidth="1.5"
                        strokeLinecap="round"
                        strokeLinejoin="round"
                      />
                    </svg>
                  )}
                  {isRedownloading ? "Repairing Dependencies..." : "Repair"}
                </button>

                <button
                  className="so-action-btn so-action-btn--ghost"
                  onClick={onRescan}
                  disabled={operationActive}
                  type="button"
                >
                  <svg width="13" height="13" viewBox="0 0 13 13" fill="none">
                    <circle
                      cx="6.5"
                      cy="6.5"
                      r="5"
                      stroke="currentColor"
                      strokeWidth="1.4"
                    />
                    <path
                      d="M6.5 4v3l2 1"
                      stroke="currentColor"
                      strokeWidth="1.3"
                      strokeLinecap="round"
                      strokeLinejoin="round"
                    />
                  </svg>
                  {isChecking ? "Checking Health..." : "Check Health"}
                </button>
              </div>

              {depsInstallMessage && (
                <div className="so-install-msg">
                  <span className="so-install-msg-dot" />
                  {depsInstallMessage}
                </div>
              )}
            </div>
          )}

          {activeTab === "about" && (
            <div className="so-content">
              <div className="so-page-header">
                <div>
                  <p className="so-eyebrow">Info</p>
                  <h2 className="so-page-title">About</h2>
                </div>
              </div>
              <div className="so-about-card">
                <div className="so-about-header">
                  <div className="so-about-logo-wrap">
                    <img
                      src="logo.png"
                      alt=""
                      className="so-about-logo"
                      aria-hidden="true"
                    />
                  </div>

                  <div className="so-about-identity">
                    <p className="so-about-eyebrow">About</p>
                    <h3 className="so-about-name">{aboutMetadata.appName}</h3>
                    <code className="so-about-version">
                      v{aboutMetadata.appVersion}
                    </code>
                  </div>
                </div>

                <div className="so-about-rule" />

                <dl className="so-about-meta">
                  {[
                    {
                      label: "Edition",
                      value: edition,
                      mono: false,
                    },
                    {
                      label: "Build",
                      value: formatBuildModeLabel(aboutMetadata.buildMode),
                      mono: false,
                    },
                    {
                      label: "Tauri Runtime",
                      value: aboutMetadata.tauriVersion,
                      mono: true,
                    },
                    {
                      label: "Bundle ID",
                      value: aboutMetadata.identifier,
                      mono: true,
                    },
                  ].map(({ label, value, mono }) => (
                    <div key={label} className="so-about-meta-row">
                      <dt className="so-about-meta-label">{label}</dt>
                      <dd className="so-about-meta-value">
                        {mono ? (
                          <code className="so-about-meta-code">{value}</code>
                        ) : (
                          value
                        )}
                      </dd>
                    </div>
                  ))}
                </dl>

                <div className="so-about-footer">
                  <span className="so-about-copyright">
                    Copyright © 2026 Vishwas Sharma
                  </span>
                </div>
              </div>
            </div>
          )}

          {activeTab === "license" && (
            <LicenseSettingsPanel
              appName={aboutMetadata.appName}
              appVersion={aboutMetadata.appVersion}
              edition={edition}
              buildModeLabel={formatBuildModeLabel(aboutMetadata.buildMode)}
              appIdentifier={aboutMetadata.identifier}
            />
          )}

          {activeTab === "help" && <HelpSettingsPanel />}
        </div>
      </section>

      <style>{`
        /* ── Panel shell ──────────────────────────────────────── */
        .so-panel {
          position: absolute;
          top: 24px;
          right: 24px;
          bottom: 24px;
          width: min(700px, calc(100% - 48px));
          display: flex;
          border-radius: 16px;
          border: 1px solid var(--border-strong);
          background: var(--bg-card);
          box-shadow: var(--shadow-lg);
          overflow: hidden;
          transform: scale(0.97) translateZ(0);
          opacity: 0;
          transition:
            opacity 0.3s cubic-bezier(0.4, 0, 0.2, 1),
            transform 0.3s cubic-bezier(0.4, 0, 0.2, 1);
        }
        .view-overlay.open .so-panel {
          opacity: 1;
          transform: scale(1) translateZ(0);
        }

        /* ── Rail ─────────────────────────────────────────────── */
        .so-rail {
          width: 180px;
          flex-shrink: 0;
          display: flex;
          flex-direction: column;
          background: var(--bg-secondary);
          border-right: 1px solid var(--border);
          padding: 20px 12px;
          gap: 0;
        }
        .so-rail-top {
          margin-bottom: 24px;
          padding: 0 4px;
        }
        .so-rail-wordmark {
          display: flex;
          align-items: center;
          gap: 8px;
        }
        .so-rail-icon {
          font-size: 16px;
          color: var(--accent);
          line-height: 1;
        }
        .so-rail-label {
          font-size: 13px;
          font-weight: 700;
          letter-spacing: -0.02em;
          color: var(--text-primary);
        }
        .so-nav {
          display: flex;
          flex-direction: column;
          gap: 2px;
          flex: 1;
        }
        .so-nav-item {
          display: flex;
          align-items: center;
          gap: 9px;
          padding: 9px 10px;
          border-radius: 9px;
          border: none;
          background: transparent;
          color: var(--text-secondary);
          font: inherit;
          font-size: 12px;
          font-weight: 500;
          cursor: pointer;
          text-align: left;
          transition: background 0.16s ease, color 0.16s ease;
          position: relative;
        }
        .so-nav-item:hover {
          background: var(--bg-hover);
          color: var(--text-primary);
        }
        .so-nav-item:focus-visible {
          outline: 2px solid var(--accent);
          outline-offset: 2px;
        }
        .so-nav-item.active {
          background: var(--bg-active);
          color: var(--accent);
          font-weight: 600;
        }
        .so-nav-icon {
          display: flex;
          align-items: center;
          justify-content: center;
          font-size: 14px;
          line-height: 1;
          width: 16px;
          text-align: center;
          flex-shrink: 0;
        }
        .so-nav-icon svg {
          display: block;
        }
        .so-nav-text {
          flex: 1;
        }
        .so-nav-dot {
          width: 6px;
          height: 6px;
          border-radius: 50%;
          background: var(--warning);
          flex-shrink: 0;
          box-shadow: 0 0 0 3px color-mix(in srgb, var(--warning) 20%, transparent);
        }
        .so-rail-bottom {
          margin-top: 12px;
          padding-top: 12px;
          border-top: 1px solid var(--border);
        }
        .so-close-btn {
          display: flex;
          align-items: center;
          gap: 7px;
          width: 100%;
          padding: 8px 10px;
          border-radius: 8px;
          border: none;
          background: transparent;
          color: var(--text-muted);
          font: inherit;
          font-size: 11px;
          font-weight: 500;
          cursor: pointer;
          transition: background 0.16s ease, color 0.16s ease;
        }
        .so-close-btn:hover {
          background: var(--bg-hover);
          color: var(--text-primary);
        }
        .so-close-btn:focus-visible {
          outline: 2px solid var(--accent);
          outline-offset: 2px;
        }

        /* ── Body ─────────────────────────────────────────────── */
        .so-body {
          flex: 1;
          min-width: 0;
          overflow-y: auto;
          scrollbar-gutter: stable;
        }
        .so-body::-webkit-scrollbar { width: 5px; }
        .so-body::-webkit-scrollbar-track { background: transparent; }
        .so-body::-webkit-scrollbar-thumb {
          background: var(--scrollbar-thumb);
          border-radius: 3px;
        }
        .so-content {
          padding: 28px 32px 36px;
          display: flex;
          flex-direction: column;
        }

        /* ── Page header ──────────────────────────────────────── */
        .so-page-header {
          display: flex;
          align-items: flex-start;
          justify-content: space-between;
          margin-bottom: 24px;
        }
        .so-eyebrow {
          font-size: 10px;
          font-weight: 700;
          letter-spacing: 0.1em;
          text-transform: uppercase;
          color: var(--text-muted);
          margin-bottom: 4px;
        }
        .so-page-title {
          font-size: 22px;
          font-weight: 700;
          letter-spacing: -0.03em;
          color: var(--text-primary);
          line-height: 1.1;
        }

        /* ── Health ring ──────────────────────────────────────── */
        .so-health-ring-wrap {
          position: relative;
          width: 44px;
          height: 44px;
          flex-shrink: 0;
          cursor: default;
        }
        .so-health-ring {
          display: block;
        }
        .so-health-pct {
          position: absolute;
          inset: 0;
          display: flex;
          align-items: center;
          justify-content: center;
          font-size: 10px;
          font-weight: 700;
          font-family: var(--font-mono);
          line-height: 1;
        }

        /* ── Status strip ─────────────────────────────────────── */
        .so-status-strip {
          display: flex;
          align-items: center;
          gap: 0;
          padding: 14px 18px;
          border-radius: 11px;
          border: 1px solid var(--border);
          background: var(--bg-input);
          margin-bottom: 24px;
        }
        .so-stat {
          flex: 1;
          display: flex;
          flex-direction: column;
          gap: 4px;
          padding: 0 10px;
        }
        .so-stat:first-child { padding-left: 0; }
        .so-stat:last-child { padding-right: 0; }
        .so-stat-divider {
          width: 1px;
          height: 28px;
          background: var(--border);
          flex-shrink: 0;
        }
        .so-stat-label {
          font-size: 9px;
          font-weight: 700;
          letter-spacing: 0.09em;
          text-transform: uppercase;
          color: var(--text-muted);
        }
        .so-stat-value {
          font-size: 12px;
          font-weight: 600;
          color: var(--text-primary);
          font-family: var(--font-mono);
        }
        .so-stat-total {
          color: var(--text-muted);
          font-weight: 400;
        }
        .so-stat-ok  { color: var(--success); }
        .so-stat-warn { color: var(--warning); }

        /* ── Section label ────────────────────────────────────── */
        .so-section-label {
          font-size: 9px;
          font-weight: 700;
          letter-spacing: 0.1em;
          text-transform: uppercase;
          color: var(--text-muted);
          margin-bottom: 10px;
        }

        /* ── Dependency list ──────────────────────────────────── */
        .so-dep-list {
          display: flex;
          flex-direction: column;
          gap: 6px;
        }
        .so-dep-row {
          display: flex;
          align-items: center;
          gap: 12px;
          padding: 13px 16px;
          border-radius: 10px;
          border: 1px solid var(--border);
          background: var(--bg-input);
          transition: border-color 0.18s ease, background 0.18s ease;
          animation: soDepIn 0.22s ease both;
        }
        .so-dep-row:hover {
          border-color: var(--border-strong);
        }
        .so-dep-row--warn {
          border-color: color-mix(in srgb, var(--warning) 35%, transparent);
        }
        @keyframes soDepIn {
          from { opacity: 0; transform: translateY(6px); }
          to   { opacity: 1; transform: translateY(0);   }
        }
        .so-dep-indicator {
          display: flex;
          align-items: center;
          justify-content: center;
          width: 20px;
          flex-shrink: 0;
        }
        .so-dep-dot {
          width: 8px;
          height: 8px;
          border-radius: 50%;
          flex-shrink: 0;
        }
        .so-dep-info {
          flex: 1;
          min-width: 0;
          display: flex;
          flex-direction: column;
          gap: 3px;
        }
        .so-dep-name {
          font-size: 12px;
          font-weight: 600;
          color: var(--text-primary);
          letter-spacing: -0.01em;
        }
        .so-dep-versions {
          display: flex;
          align-items: center;
          gap: 6px;
          flex-wrap: wrap;
        }
        .so-dep-version-item {
          display: flex;
          align-items: center;
          gap: 4px;
        }
        .so-dep-version-label {
          font-size: 9px;
          text-transform: uppercase;
          letter-spacing: 0.07em;
          color: var(--text-muted);
          font-weight: 600;
        }
        .so-dep-version-val {
          font-family: var(--font-mono);
          font-size: 10px;
          font-weight: 500;
          color: var(--text-secondary);
        }
        .so-dep-version-sep {
          color: var(--border-strong);
          font-size: 10px;
        }
        .so-dep-empty {
          padding: 20px;
          text-align: center;
          color: var(--text-muted);
          font-size: 12px;
        }

        /* ── Action buttons ───────────────────────────────────── */
        .so-actions {
          display: flex;
          gap: 8px;
          flex-wrap: wrap;
        }
        .so-action-btn {
          display: inline-flex;
          align-items: center;
          gap: 7px;
          padding: 9px 16px;
          border-radius: 9px;
          border: 1px solid var(--border);
          background: var(--bg-card);
          color: var(--text-primary);
          font: inherit;
          font-size: 12px;
          font-weight: 500;
          cursor: pointer;
          transition: all 0.16s ease;
          white-space: nowrap;
        }
        .so-action-btn:hover {
          background: var(--bg-hover);
          border-color: var(--border-strong);
        }
        .so-action-btn:active { transform: scale(0.97); }
        .so-action-btn:focus-visible {
          outline: 2px solid var(--accent);
          outline-offset: 2px;
        }
        .so-action-btn:disabled {
          opacity: 0.4;
          cursor: not-allowed;
          pointer-events: none;
        }
        .so-action-btn--primary {
          background: var(--accent);
          border-color: var(--accent);
          color: #fff;
          font-weight: 600;
        }
        .so-action-btn--primary:hover {
          background: var(--accent-hover);
          border-color: var(--accent-hover);
        }
        .so-action-btn--ghost {
          background: transparent;
          border-color: transparent;
          color: var(--text-secondary);
        }
        .so-action-btn--ghost:hover {
          background: var(--bg-hover);
          border-color: var(--border);
          color: var(--text-primary);
        }

        /* ── Spinner ──────────────────────────────────────────── */
        .so-spinner {
          width: 12px;
          height: 12px;
          border: 2px solid rgba(255,255,255,0.3);
          border-top-color: #fff;
          border-radius: 50%;
          animation: soSpin 0.65s linear infinite;
          flex-shrink: 0;
        }
        .so-spinner--dark {
          border-color: color-mix(in srgb, currentColor 25%, transparent);
          border-top-color: currentColor;
        }
        @keyframes soSpin {
          to { transform: rotate(360deg); }
        }

        /* ── Install message ──────────────────────────────────── */
        .so-install-msg {
          margin-top: 14px;
          display: flex;
          align-items: center;
          gap: 8px;
          font-size: 11px;
          color: var(--text-muted);
          font-family: var(--font-mono);
          padding: 10px 14px;
          border-radius: 8px;
          background: var(--bg-input);
          border: 1px solid var(--border);
        }
        .so-install-msg-dot {
          width: 6px;
          height: 6px;
          border-radius: 50%;
          background: var(--accent);
          flex-shrink: 0;
          animation: soPulse 1.4s ease infinite;
        }
        @keyframes soPulse {
          0%, 100% { opacity: 1; }
          50%       { opacity: 0.35; }
        }

        /* ── About card ───────────────────────────────────────── */
        .so-about-card {
          padding: 24px;
          border-radius: 12px;
          border: 1px solid var(--border);
          background: var(--bg-input);
        }
        .so-about-name {
          font-size: 16px;
          font-weight: 700;
          letter-spacing: -0.02em;
          color: var(--text-primary);
          margin-bottom: 4px;
        }
        .so-about-tagline {
          font-size: 12px;
          color: var(--text-secondary);
        }

        .so-about-card {
          border-radius: 16px;
          border: 1px solid var(--border);
          background: var(--bg-card);
          overflow: hidden;
          display: flex;
          flex-direction: column;
        }
        .so-about-header {
          display: flex;
          align-items: flex-start;
          gap: 14px;
          padding: 24px 24px 20px;
        }
        .so-about-logo-wrap {
          width: 44px;
          height: 44px;
          border-radius: 11px;
          border: 1px solid var(--border);
          background: var(--bg-input);
          display: flex;
          align-items: center;
          justify-content: center;
          flex-shrink: 0;
          overflow: hidden;
        }
        .so-about-logo {
          width: 28px;
          height: 28px;
          object-fit: contain;
        }
        .so-about-identity {
          flex: 1;
          min-width: 0;
          display: flex;
          flex-direction: column;
          gap: 5px;
        }
        .so-about-eyebrow {
          font-size: 10px;
          font-weight: 700;
          letter-spacing: 0.1em;
          text-transform: uppercase;
          color: var(--text-muted);
          line-height: 1;
        }
        .so-about-name {
          font-size: 18px;
          font-weight: 700;
          letter-spacing: -0.03em;
          color: var(--text-primary);
          line-height: 1.1;
          margin-bottom: 0;
          white-space: nowrap;
          overflow: hidden;
          text-overflow: ellipsis;
        }
        .so-about-version {
          font-family: var(--font-mono);
          font-size: 11px;
          font-weight: 600;
          color: var(--text-secondary);
        }
        .so-about-rule {
          height: 1px;
          background: var(--border);
          margin: 0 24px;
        }
        .so-about-meta {
          display: flex;
          flex-direction: column;
          gap: 0;
          padding: 6px 0;
          margin: 0;
        }
        .so-about-meta-row {
          display: flex;
          align-items: center;
          justify-content: space-between;
          gap: 16px;
          padding: 10px 24px;
          border-bottom: 1px solid var(--border);
        }
        .so-about-meta-row:last-child {
          border-bottom: none;
        }
        .so-about-meta-label {
          font-size: 11px;
          font-weight: 500;
          color: var(--text-muted);
          flex-shrink: 0;
        }
        .so-about-meta-value {
          font-size: 11px;
          color: var(--text-primary);
          text-align: right;
          min-width: 0;
          overflow: hidden;
          text-overflow: ellipsis;
          white-space: nowrap;
        }
        .so-about-meta-code {
          font-family: var(--font-mono);
          font-size: 10.5px;
          font-weight: 500;
          color: var(--text-secondary);
        }
        .so-about-footer {
          padding: 14px 24px;
          border-top: 1px solid var(--border);
          background: var(--bg-secondary);
        }
        .so-about-copyright {
          font-size: 10px;
          color: var(--text-muted);
          font-weight: 500;
          letter-spacing: 0.01em;
        }

        @media (max-width: 900px) {
          .so-panel {
            top: 10px;
            right: 10px;
            bottom: 10px;
            width: calc(100% - 20px);
          }
          .so-rail { width: 150px; }
          .so-content { padding: 20px 20px 28px; }
        }
        @media (max-width: 560px) {
          .so-panel { flex-direction: column; }
          .so-rail {
            width: 100%;
            flex-direction: row;
            align-items: center;
            padding: 12px 16px;
            border-right: none;
            border-bottom: 1px solid var(--border);
          }
          .so-rail-top { margin-bottom: 0; margin-right: 16px; }
          .so-nav { flex-direction: row; flex: 1; }
          .so-rail-bottom { margin-top: 0; margin-left: 12px; padding-top: 0; border-top: none; border-left: 1px solid var(--border); padding-left: 12px; }
          .so-status-strip { flex-wrap: wrap; }
        }
      `}</style>
    </div>
  );
}
