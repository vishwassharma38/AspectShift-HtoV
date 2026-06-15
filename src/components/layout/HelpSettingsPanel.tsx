import { useState, type ReactNode } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";

const DEVELOPER_NAME = "Vishwas Sharma";
const DEVELOPER_EMAIL = "vishwassharma38@gmail.com";
const SUPPORT_EMAIL = "support@aspectshift-htov.com";
const INSTAGRAM_URL = "https://www.instagram.com/aspectshift_htov/";

function MailIcon() {
  return (
    <svg width="13" height="13" viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <path
        d="M4 4h16a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2Z"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <path
        d="m22 6-10 7L2 6"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

function ExternalLinkIcon() {
  return (
    <svg width="13" height="13" viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <path
        d="M14 3h7v7"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <path
        d="M10 14 21 3"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <path
        d="M21 14v6a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V5a1 1 0 0 1 1-1h6"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

type HelpField = {
  label: string;
  value: string;
  mono?: boolean;
};

function HelpFieldRow({ label, value, mono }: HelpField) {
  return (
    <div className="so-help-meta-row">
      <dt className="so-help-meta-label">{label}</dt>
      <dd className="so-help-meta-value">
        {mono ? (
          <code className="so-help-meta-code">{value}</code>
        ) : (
          <span className="so-help-meta-text">{value}</span>
        )}
      </dd>
    </div>
  );
}

function HelpContactCard({
  title,
  description,
  fields,
  actionLabel,
  actionSubtitle,
  actionTarget,
  actionIcon,
}: {
  title: string;
  description: string;
  fields: HelpField[];
  actionLabel: string;
  actionSubtitle: string;
  actionTarget: string;
  actionIcon: ReactNode;
}) {
  const [error, setError] = useState<string | null>(null);

  async function handleOpen() {
    setError(null);
    try {
      await openUrl(actionTarget);
    } catch (cause) {
      const message =
        cause instanceof Error ? cause.message : "Unknown error while opening link.";
      setError(`Could not open ${actionTarget}: ${message}`);
    }
  }

  return (
    <section className="so-help-card">
      <div className="so-help-card-head">
        <div>
          <p className="so-help-card-eyebrow">Contact</p>
          <h3 className="so-help-card-title">{title}</h3>
        </div>
      </div>

      <p className="so-help-card-desc">{description}</p>

      <dl className="so-help-meta">
        {fields.map((field) => (
          <HelpFieldRow key={field.label} {...field} />
        ))}
      </dl>

      {error ? (
        <p className="so-help-error" role="alert">
          {error}
        </p>
      ) : null}

      <button
        className="so-action-btn so-help-action"
        onClick={handleOpen}
        type="button"
        aria-label={`${actionLabel} ${actionSubtitle}`}
        title={actionSubtitle}
      >
        <span className="so-help-action-icon">{actionIcon}</span>
        <span className="so-help-action-body">
          <span className="so-help-action-title">{actionLabel}</span>
          <span className="so-help-action-subtitle">{actionSubtitle}</span>
        </span>
        <span className="so-help-action-arrow">
          <ExternalLinkIcon />
        </span>
      </button>
    </section>
  );
}

export function HelpSettingsPanel() {
  return (
    <div className="so-content so-help-content">
      <div className="so-page-header">
        <div>
          <p className="so-eyebrow">Contact</p>
          <h2 className="so-page-title">Help</h2>
        </div>
      </div>

      <div className="so-help-stack">
        <HelpContactCard
          title="Developer Information"
          description="Use this contact for developer-related inquiries or direct communication where appropriate."
          fields={[
            { label: "Developer", value: DEVELOPER_NAME },
            { label: "Developer Email", value: DEVELOPER_EMAIL, mono: true },
          ]}
          actionLabel="Email Vishwas Sharma"
          actionSubtitle={DEVELOPER_EMAIL}
          actionTarget={`mailto:${DEVELOPER_EMAIL}`}
          actionIcon={<MailIcon />}
        />

        <HelpContactCard
          title="Support Contact"
          description="Use this email for support requests, bug reports, troubleshooting, installation or usage issues, and other software-related help."
          fields={[{ label: "Support Email", value: SUPPORT_EMAIL, mono: true }]}
          actionLabel="Email Support"
          actionSubtitle={SUPPORT_EMAIL}
          actionTarget={`mailto:${SUPPORT_EMAIL}`}
          actionIcon={<MailIcon />}
        />

        <HelpContactCard
          title="Instagram / Community"
          description="Use this page for updates, announcements, community interaction, feedback, and feature suggestions."
          fields={[{ label: "Instagram", value: INSTAGRAM_URL, mono: true }]}
          actionLabel="Open Instagram"
          actionSubtitle={INSTAGRAM_URL}
          actionTarget={INSTAGRAM_URL}
          actionIcon={<ExternalLinkIcon />}
        />
      </div>

      <style>{`
        .so-help-content {
          gap: 0;
        }

        .so-help-stack {
          display: flex;
          flex-direction: column;
          gap: 12px;
        }

        .so-help-card {
          padding: 20px 20px 18px;
          border-radius: 16px;
          border: 1px solid var(--border);
          background: var(--bg-card);
          overflow: hidden;
        }

        .so-help-card-head {
          display: flex;
          align-items: flex-start;
          justify-content: space-between;
          gap: 12px;
          margin-bottom: 10px;
        }

        .so-help-card-eyebrow {
          font-size: 10px;
          font-weight: 700;
          letter-spacing: 0.1em;
          text-transform: uppercase;
          color: var(--text-muted);
          margin-bottom: 4px;
        }

        .so-help-card-title {
          font-size: 17px;
          font-weight: 700;
          letter-spacing: -0.03em;
          line-height: 1.15;
          color: var(--text-primary);
        }

        .so-help-card-desc {
          font-size: 12.5px;
          line-height: 1.6;
          color: var(--text-secondary);
          margin-bottom: 14px;
        }

        .so-help-meta {
          display: flex;
          flex-direction: column;
          gap: 8px;
          margin: 0;
        }

        .so-help-meta-row {
          display: flex;
          align-items: center;
          justify-content: space-between;
          gap: 14px;
          padding: 12px 14px;
          border-radius: 12px;
          border: 1px solid var(--border);
          background: var(--bg-input);
          min-width: 0;
        }

        .so-help-meta-label {
          font-size: 10px;
          font-weight: 700;
          letter-spacing: 0.08em;
          text-transform: uppercase;
          color: var(--text-muted);
          flex-shrink: 0;
        }

        .so-help-meta-value {
          margin: 0;
          min-width: 0;
          text-align: right;
        }

        .so-help-meta-text,
        .so-help-meta-code {
          display: block;
          min-width: 0;
          overflow: hidden;
          text-overflow: ellipsis;
          white-space: nowrap;
        }

        .so-help-meta-text {
          font-size: 11.5px;
          color: var(--text-primary);
        }

        .so-help-meta-code {
          font-family: var(--font-mono);
          font-size: 10.5px;
          font-weight: 500;
          color: var(--text-secondary);
        }

        .so-help-error {
          margin-top: 10px;
          padding: 10px 12px;
          border-radius: 10px;
          border: 1px solid color-mix(in srgb, var(--error) 30%, var(--border));
          background: color-mix(in srgb, var(--error) 8%, var(--bg-input));
          color: var(--text-secondary);
          font-size: 11px;
          line-height: 1.45;
        }

        .so-help-action {
          width: 100%;
          margin-top: 12px;
          justify-content: flex-start;
          gap: 10px;
          white-space: normal;
          text-align: left;
        }

        .so-help-action-icon {
          display: inline-flex;
          align-items: center;
          justify-content: center;
          width: 26px;
          height: 26px;
          border-radius: 8px;
          border: 1px solid var(--border);
          background: var(--bg-input);
          color: var(--accent);
          flex-shrink: 0;
        }

        .so-help-action-body {
          flex: 1;
          min-width: 0;
          display: flex;
          flex-direction: column;
          gap: 2px;
        }

        .so-help-action-title {
          font-size: 12px;
          font-weight: 600;
          color: var(--text-primary);
          letter-spacing: -0.01em;
        }

        .so-help-action-subtitle {
          font-family: var(--font-mono);
          font-size: 10px;
          color: var(--text-muted);
          overflow-wrap: anywhere;
          line-height: 1.45;
        }

        .so-help-action-arrow {
          color: var(--text-muted);
          flex-shrink: 0;
          display: inline-flex;
          align-items: center;
          justify-content: center;
        }

        .so-help-action:focus-visible {
          outline: 2px solid var(--accent);
          outline-offset: 2px;
        }

        @media (max-width: 700px) {
          .so-help-meta-row {
            flex-direction: column;
            align-items: flex-start;
          }

          .so-help-meta-value {
            text-align: left;
          }

          .so-help-action {
            align-items: flex-start;
          }
        }
      `}</style>
    </div>
  );
}
