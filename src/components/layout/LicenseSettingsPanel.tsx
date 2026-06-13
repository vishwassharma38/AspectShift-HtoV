import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { ProductEdition } from "../../utils/productEdition";
import { legalDocuments, legalSummary } from "../../data/legalContent";

interface LicenseSettingsPanelProps {
  appName: string;
  appVersion: string;
  edition: ProductEdition;
  buildModeLabel: string;
  appIdentifier: string;
}

// Icon components
function FileTextIcon() {
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 14 14"
      fill="none"
      aria-hidden="true"
    >
      <path
        d="M8 1H3a1 1 0 0 0-1 1v10a1 1 0 0 0 1 1h8a1 1 0 0 0 1-1V5L8 1Z"
        stroke="currentColor"
        strokeWidth="1.3"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <path
        d="M8 1v4h4M5 7.5h4M5 9.5h3"
        stroke="currentColor"
        strokeWidth="1.3"
        strokeLinecap="round"
      />
    </svg>
  );
}

function ExternalLinkIcon() {
  return (
    <svg
      width="11"
      height="11"
      viewBox="0 0 11 11"
      fill="none"
      aria-hidden="true"
    >
      <path
        d="M4.5 2H2a1 1 0 0 0-1 1v6a1 1 0 0 0 1 1h6a1 1 0 0 0 1-1V6.5"
        stroke="currentColor"
        strokeWidth="1.25"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <path
        d="M6.5 1H10M10 1v3.5M10 1 5.5 5.5"
        stroke="currentColor"
        strokeWidth="1.25"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

async function openExternalDoc(fileName: string) {
  await invoke("open_license_document", { fileName });
}

function LicenseDocRow({
  title,
  filePath,
  fileName,
  index,
  onOpen,
}: {
  title: string;
  filePath: string;
  fileName: string;
  index: number;
  onOpen: (fileName: string) => void | Promise<void>;
}) {
  // Derive a short tag from the file path (e.g. "GPL-3.0", "NOTICES")
  const tag = (() => {
    const base = filePath.split("/").pop() ?? filePath;
    const name = base.replace(/\.(txt|md|toml)$/i, "");
    if (name.length <= 22) return name;
    return name.slice(0, 20) + "…";
  })();

  return (
    <button
      className="so-lic-row"
      style={{ animationDelay: `${index * 35}ms` }}
      onClick={() => onOpen(fileName)}
      title={`Open ${filePath}`}
      type="button"
    >
      <span className="so-lic-row-icon">
        <FileTextIcon />
      </span>
      <span className="so-lic-row-body">
        <span className="so-lic-row-title">{title}</span>
        <code className="so-lic-row-path">{filePath}</code>
      </span>
      <span className="so-lic-row-tag">{tag}</span>
      <span className="so-lic-row-arrow">
        <ExternalLinkIcon />
      </span>
    </button>
  );
}

export function LicenseSettingsPanel({
  appName,
  appVersion,
}: LicenseSettingsPanelProps) {
  const [openError, setOpenError] = useState<string | null>(null);

  async function handleOpenDocument(fileName: string) {
    setOpenError(null);
    try {
      await openExternalDoc(fileName);
    } catch (error) {
      const message =
        typeof error === "object" &&
        error !== null &&
        "message" in error &&
        typeof (error as { message?: unknown }).message === "string"
          ? (error as { message: string }).message
          : error instanceof Error
            ? error.message
            : "Unknown error while opening document.";
      setOpenError(`Could not open ${fileName}: ${message}`);
    }
  }

  return (
    <div className="so-content so-lic-content">
      {/* Page header — matches other tabs exactly */}
      <div className="so-page-header">
        <div>
          <p className="so-eyebrow">Legal</p>
          <h2 className="so-page-title">License</h2>
        </div>
      </div>

      {/* App identity strip */}
      <div className="so-lic-identity">
        <div className="so-lic-identity-main">
          <span className="so-lic-identity-name">{appName}</span>
          <code className="so-lic-identity-ver">v{appVersion}</code>
        </div>
        <dl className="so-lic-identity-meta">
          {[
            { label: "Author", value: legalSummary.author },
            { label: "Publisher", value: legalSummary.publisher },
            { label: "License", value: legalSummary.license, mono: true },
            { label: "Copyright", value: legalSummary.copyright },
          ].map(({ label, value, mono }) => (
            <div key={label} className="so-lic-identity-meta-row">
              <dt className="so-lic-identity-meta-label">{label}</dt>
              <dd className="so-lic-identity-meta-value">
                {mono ? (
                  <code className="so-lic-identity-meta-code">{value}</code>
                ) : (
                  value
                )}
              </dd>
            </div>
          ))}
        </dl>
      </div>

      {/* Document list */}
      <div className="so-section-label">Documents</div>
      {openError ? (
        <p className="so-lic-error" role="alert">
          {openError}
        </p>
      ) : null}
      <div className="so-lic-list" role="list">
        {legalDocuments.map((doc, index) => (
          <LicenseDocRow
            key={doc.id}
            title={doc.title}
            filePath={doc.filePath}
            fileName={doc.fileName}
            index={index}
            onOpen={handleOpenDocument}
          />
        ))}
      </div>

      {/* Footnote */}
      <p className="so-lic-footnote">
        License files are stored in <code>LICENSE/</code> and tracked in the
        repository. Click any entry to open it in your default viewer.
      </p>

      <style>{`
        /* ── Layout ───────────────────────────────────────────── */
        .so-lic-content {
          gap: 0;
        }

        /* ── Identity strip ───────────────────────────────────── */
        .so-lic-identity {
          display: flex;
          flex-direction: column;
          gap: 0;
          border-radius: 14px;
          border: 1px solid var(--border);
          background: var(--bg-card);
          overflow: hidden;
          margin-bottom: 24px;
        }
        .so-lic-identity-main {
          display: flex;
          align-items: baseline;
          gap: 10px;
          padding: 16px 20px 14px;
          border-bottom: 1px solid var(--border);
          background: var(--bg-secondary);
        }
        .so-lic-identity-name {
          font-size: 15px;
          font-weight: 700;
          letter-spacing: -0.025em;
          color: var(--text-primary);
          line-height: 1.1;
        }
        .so-lic-identity-ver {
          font-family: var(--font-mono);
          font-size: 10.5px;
          font-weight: 600;
          color: var(--text-muted);
        }
        .so-lic-identity-meta {
          display: grid;
          grid-template-columns: repeat(2, minmax(0, 1fr));
          gap: 1px;
          margin: 0;
          background: var(--border);
        }
        .so-lic-identity-meta-row {
          display: flex;
          align-items: center;
          justify-content: space-between;
          gap: 12px;
          padding: 10px 20px;
          background: var(--bg-card);
          min-width: 0;
        }
        .so-lic-identity-meta-label {
          font-size: 10px;
          font-weight: 700;
          letter-spacing: 0.08em;
          text-transform: uppercase;
          color: var(--text-muted);
          flex-shrink: 0;
        }
        .so-lic-identity-meta-value {
          font-size: 11px;
          color: var(--text-primary);
          text-align: right;
          min-width: 0;
          overflow: hidden;
          text-overflow: ellipsis;
          white-space: nowrap;
        }
        .so-lic-identity-meta-code {
          font-family: var(--font-mono);
          font-size: 10.5px;
          font-weight: 500;
          color: var(--text-secondary);
        }

        /* ── Document list ────────────────────────────────────── */
        .so-lic-list {
          display: flex;
          flex-direction: column;
          gap: 6px;
        }

        .so-lic-error {
          margin: 0 0 10px;
          padding: 10px 12px;
          border-radius: 10px;
          border: 1px solid color-mix(in srgb, var(--danger, #c0392b) 30%, var(--border));
          background: color-mix(in srgb, var(--danger, #c0392b) 8%, var(--bg-input));
          color: var(--text-secondary);
          font-size: 11px;
          line-height: 1.45;
        }

        .so-lic-row {
          display: flex;
          align-items: center;
          gap: 12px;
          width: 100%;
          padding: 13px 16px;
          border-radius: 12px;
          border: 1px solid var(--border);
          background: var(--bg-card);
          cursor: pointer;
          text-align: left;
          transition: background 0.14s ease, border-color 0.14s ease,
            box-shadow 0.14s ease;
          animation: soLicRowIn 0.22s ease both;
        }
        @keyframes soLicRowIn {
          from {
            opacity: 0;
            transform: translateY(6px);
          }
          to {
            opacity: 1;
            transform: translateY(0);
          }
        }
        .so-lic-row:hover {
          background: var(--bg-hover, var(--bg-input));
          border-color: var(--border-strong, var(--border));
          box-shadow: 0 2px 8px rgba(0, 0, 0, 0.06);
        }
        .so-lic-row:focus-visible {
          outline: 2px solid var(--accent);
          outline-offset: 2px;
        }
        .so-lic-row:active {
          transform: scale(0.99);
        }

        .so-lic-row-icon {
          display: flex;
          align-items: center;
          justify-content: center;
          width: 30px;
          height: 30px;
          border-radius: 8px;
          background: var(--bg-input);
          border: 1px solid var(--border);
          color: var(--text-muted);
          flex-shrink: 0;
          transition: color 0.14s ease, background 0.14s ease;
        }
        .so-lic-row:hover .so-lic-row-icon {
          color: var(--accent, var(--text-secondary));
          background: color-mix(in srgb, var(--accent, #666) 10%, var(--bg-input));
        }

        .so-lic-row-body {
          display: flex;
          flex-direction: column;
          gap: 3px;
          flex: 1;
          min-width: 0;
        }
        .so-lic-row-title {
          font-size: 12.5px;
          font-weight: 600;
          color: var(--text-primary);
          letter-spacing: -0.01em;
          line-height: 1.2;
          white-space: nowrap;
          overflow: hidden;
          text-overflow: ellipsis;
        }
        .so-lic-row-path {
          font-family: var(--font-mono);
          font-size: 10px;
          font-weight: 500;
          color: var(--text-muted);
          white-space: nowrap;
          overflow: hidden;
          text-overflow: ellipsis;
        }

        .so-lic-row-tag {
          font-family: var(--font-mono);
          font-size: 9.5px;
          font-weight: 600;
          letter-spacing: 0.02em;
          color: var(--text-muted);
          background: var(--bg-input);
          border: 1px solid var(--border);
          border-radius: 6px;
          padding: 3px 7px;
          flex-shrink: 0;
          white-space: nowrap;
          max-width: 140px;
          overflow: hidden;
          text-overflow: ellipsis;
          transition: color 0.14s ease;
        }
        .so-lic-row:hover .so-lic-row-tag {
          color: var(--text-secondary);
        }

        .so-lic-row-arrow {
          display: flex;
          align-items: center;
          color: var(--text-muted);
          opacity: 0;
          flex-shrink: 0;
          transition: opacity 0.14s ease, color 0.14s ease;
        }
        .so-lic-row:hover .so-lic-row-arrow {
          opacity: 1;
          color: var(--text-secondary);
        }

        /* ── Footnote ─────────────────────────────────────────── */
        .so-lic-footnote {
          margin-top: 16px;
          padding: 11px 14px;
          border-radius: 10px;
          border: 1px solid var(--border);
          background: var(--bg-input);
          font-size: 11px;
          line-height: 1.55;
          color: var(--text-muted);
        }
        .so-lic-footnote code {
          font-family: var(--font-mono);
          font-size: 10.5px;
          color: var(--text-secondary);
        }

        /* ── Responsive ───────────────────────────────────────── */
        @media (max-width: 700px) {
          .so-lic-identity-meta {
            grid-template-columns: 1fr;
          }
          .so-lic-row-tag {
            display: none;
          }
        }
        @media (max-width: 420px) {
          .so-lic-row {
            padding: 11px 13px;
            gap: 10px;
          }
          .so-lic-row-icon {
            width: 26px;
            height: 26px;
          }
        }
      `}</style>
    </div>
  );
}
