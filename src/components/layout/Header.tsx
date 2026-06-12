import { useEffect, useMemo, useRef, useState } from "react";
import { LicenseStatusDot } from "../ui/LicenseStatusDot";

interface HeaderProps {
  theme: "day" | "night";
  onToggleTheme: () => void;
  onOpenSettings: () => void;
  onOpenLicensePanel: () => void;
  onOpenAbout: () => void;
  onRefresh: () => void;
  onCheckForUpdates: () => void;
  isCheckingUpdates: boolean;
  isLicensed: boolean;
  statusBadge?: { tone: "success" | "error"; label: string } | null;
}

function SunIcon() {
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
      <circle cx="12" cy="12" r="4" />
      <path d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M6.34 17.66l-1.41 1.41M19.07 4.93l-1.41 1.41" />
    </svg>
  );
}

function MoonIcon() {
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
      <path d="M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9" />
    </svg>
  );
}

function ChevronDownIcon() {
  return (
    <svg
      width="10"
      height="10"
      viewBox="0 0 10 10"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M2 3.5l3 3 3-3" />
    </svg>
  );
}

const MENU_ITEMS = [
  { id: "updates", label: "Check for Updates", meta: "Ctrl+U" },
  { id: "settings", label: "Settings", meta: "Ctrl+," },
  { id: "license", label: "License", meta: null },
  { id: "refresh", label: "Refresh", meta: "Ctrl+R" },
  { id: "divider", label: "", meta: null },
  { id: "about", label: "About", meta: null },
] as const;

export function Header({
  theme,
  onToggleTheme,
  onOpenSettings,
  onOpenLicensePanel,
  onOpenAbout,
  onRefresh,
  onCheckForUpdates,
  isCheckingUpdates,
  isLicensed,
  statusBadge,
}: HeaderProps) {
  const [menuOpen, setMenuOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!menuOpen) return;
    const onPointerDown = (e: MouseEvent) => {
      if (!menuRef.current?.contains(e.target as Node)) setMenuOpen(false);
    };
    const onEscape = (e: KeyboardEvent) => {
      if (e.key === "Escape") setMenuOpen(false);
    };
    window.addEventListener("mousedown", onPointerDown);
    window.addEventListener("keydown", onEscape);
    return () => {
      window.removeEventListener("mousedown", onPointerDown);
      window.removeEventListener("keydown", onEscape);
    };
  }, [menuOpen]);

  const handlers: Record<string, () => void> = useMemo(
    () => ({
      updates: onCheckForUpdates,
      settings: onOpenSettings,
      license: onOpenLicensePanel,
      refresh: onRefresh,
      about: onOpenAbout,
    }),
    [
      onCheckForUpdates,
      onOpenSettings,
      onOpenLicensePanel,
      onRefresh,
      onOpenAbout,
    ],
  );

  const isDisabled = (id: string) =>
    id === "updates" && (!isLicensed || isCheckingUpdates);

  return (
    <header className="topbar">
      {/* ── Left: logo + menu ───────────────────────────── */}
      <div className="topbar-logo">
        <img src="logo.png" alt="AspectShift" className="topbar-logo-img" />

        <div className="hdr-brand">
          <span className="hdr-title">AspectShift</span>
          <span className="hdr-subtitle">-HtoV</span>
        </div>

        <LicenseStatusDot />

        {/* Divider */}
        <div className="hdr-divider" />

        {/* Menu trigger */}
        <div className="topbar-menu-wrap" ref={menuRef}>
          <button
            type="button"
            className={`hdr-menu-btn${menuOpen ? " active" : ""}`}
            aria-haspopup="menu"
            aria-expanded={menuOpen}
            aria-label="Open application menu"
            onClick={() => setMenuOpen((o) => !o)}
          >
            <span className="hdr-menu-label">Menu</span>
            <span className={`hdr-menu-chevron${menuOpen ? " flipped" : ""}`}>
              <ChevronDownIcon />
            </span>
          </button>

          <div
            className={`topbar-flyout hdr-flyout${menuOpen ? " open" : ""}`}
            role="menu"
            aria-hidden={!menuOpen}
          >
            {MENU_ITEMS.map((item) => {
              if (item.id === "divider") {
                return <div key="divider" className="hdr-flyout-divider" />;
              }
              const disabled = isDisabled(item.id);
              return (
                <button
                  key={item.id}
                  type="button"
                  role="menuitem"
                  className="topbar-flyout-item hdr-flyout-item"
                  disabled={disabled}
                  onClick={() => {
                    if (disabled) return;
                    setMenuOpen(false);
                    handlers[item.id]?.();
                  }}
                >
                  <span className="hdr-flyout-label">
                    {item.id === "updates" && isCheckingUpdates
                      ? "Checking…"
                      : item.label}
                  </span>
                  {item.meta && (
                    <kbd className="hdr-flyout-kbd">{item.meta}</kbd>
                  )}
                </button>
              );
            })}
          </div>
        </div>
      </div>

      {/* ── Right: badge + theme toggle ─────────────────── */}
      <div className="topbar-right">
        {statusBadge && (
          <span className={`badge badge-${statusBadge.tone} hdr-status-badge`}>
            {statusBadge.label}
          </span>
        )}

        <button
          type="button"
          className="theme-toggle hdr-theme-toggle"
          onClick={onToggleTheme}
          aria-label={
            theme === "night" ? "Switch to day theme" : "Switch to night theme"
          }
          title={
            theme === "night" ? "Switch to day theme" : "Switch to night theme"
          }
        >
          {theme === "night" ? <SunIcon /> : <MoonIcon />}
        </button>
      </div>

      <style>{`
        /* ── Brand ────────────────────────────────────────── */
        .hdr-brand {
          display: flex;
          align-items: baseline;
          gap: 4px;
        }
        .hdr-title {
          font-size: 13px;
          font-weight: 700;
          letter-spacing: -0.025em;
          color: var(--text-primary);
          line-height: 1;
        }
        .hdr-sep {
          font-size: 13px;
          color: var(--border-strong);
          font-weight: 300;
          line-height: 1;
          user-select: none;
        }
        .hdr-subtitle {
          font-size: 12px;
          font-weight: 500;
          color: var(--text-secondary);
          letter-spacing: 0.01em;
          line-height: 1;
          font-family: var(--font-mono);
        }

        /* ── Divider ──────────────────────────────────────── */
        .hdr-divider {
          width: 1px;
          height: 16px;
          background: var(--border);
          flex-shrink: 0;
          margin: 0 2px;
        }

        /* ── Menu trigger ─────────────────────────────────── */
        .hdr-menu-btn {
          display: inline-flex;
          align-items: center;
          gap: 5px;
          padding: 5px 9px;
          border-radius: 7px;
          border: 1px solid transparent;
          background: transparent;
          color: var(--text-secondary);
          font: inherit;
          font-size: 12px;
          font-weight: 500;
          cursor: pointer;
          transition: background 0.15s ease, border-color 0.15s ease, color 0.15s ease;
          user-select: none;
        }
        .hdr-menu-btn:hover {
          background: var(--bg-hover);
          border-color: var(--border);
          color: var(--text-primary);
        }
        .hdr-menu-btn.active {
          background: var(--bg-hover);
          border-color: var(--border-strong);
          color: var(--text-primary);
        }
        .hdr-menu-label {
          line-height: 1;
        }
        .hdr-menu-chevron {
          display: flex;
          align-items: center;
          color: var(--text-muted);
          transition: transform 0.18s ease;
        }
        .hdr-menu-chevron.flipped {
          transform: rotate(180deg);
        }

        /* ── Flyout tweaks ────────────────────────────────── */
        .hdr-flyout {
          min-width: 200px;
        }
        .hdr-flyout-item {
          display: flex;
          align-items: center;
          justify-content: space-between;
          gap: 24px;
        }
        .hdr-flyout-label {
          font-size: 12px;
          font-weight: 500;
        }
        .hdr-flyout-kbd {
          font-family: var(--font-mono);
          font-size: 10px;
          color: var(--text-muted);
          background: var(--bg-input);
          border: 1px solid var(--border);
          border-radius: 4px;
          padding: 1px 5px;
          font-style: normal;
          user-select: none;
        }
        .hdr-flyout-divider {
          height: 1px;
          background: var(--border);
          margin: 4px 8px;
        }

        /* ── Status badge ─────────────────────────────────── */
        .hdr-status-badge {
          font-size: 10px;
          font-weight: 600;
          letter-spacing: 0.03em;
          padding: 3px 8px;
          border-radius: 6px;
        }

        /* ── Theme toggle tweak ───────────────────────────── */
        .hdr-theme-toggle {
          width: 30px;
          height: 30px;
          border-radius: 7px;
        }
      `}</style>
    </header>
  );
}
