export type AppMenuAction =
  | "updates"
  | "settings"
  | "license"
  | "refresh"
  | "about";

export type AppShortcutAction = "updates" | "settings" | "refresh";

type BrowserShortcut = {
  key: string;
  primaryKey?: true;
  shiftKey?: true;
  altKey?: true;
};

type AppMenuItem =
  | {
      id: AppMenuAction;
      label: string;
      shortcut: {
        meta: string;
        ctrlKey: true;
        key: string;
      } | null;
    }
  | { id: "divider"; label: ""; shortcut: null };

export const APP_MENU_ITEMS = [
  {
    id: "updates",
    label: "Check for Updates",
    shortcut: { meta: "Ctrl+U", ctrlKey: true, key: "u" },
  },
  {
    id: "settings",
    label: "Settings",
    shortcut: { meta: "Ctrl+,", ctrlKey: true, key: "," },
  },
  { id: "license", label: "License", shortcut: null },
  {
    id: "refresh",
    label: "Refresh",
    shortcut: { meta: "Ctrl+R", ctrlKey: true, key: "r" },
  },
  { id: "divider", label: "", shortcut: null },
  { id: "about", label: "About", shortcut: null },
] as const satisfies readonly AppMenuItem[];

export const APP_SHORTCUTS = APP_MENU_ITEMS.filter(
  (
    item,
  ): item is Extract<
    (typeof APP_MENU_ITEMS)[number],
    { id: AppShortcutAction }
  > => item.id !== "divider" && item.shortcut !== null,
);

const BROWSER_ONLY_SHORTCUTS: readonly BrowserShortcut[] = [
  { key: "F12" },
  { key: "j", primaryKey: true },
  { key: "u", primaryKey: true },
  { key: "i", primaryKey: true, shiftKey: true },
  { key: "j", primaryKey: true, shiftKey: true },
  { key: "c", primaryKey: true, shiftKey: true },
  { key: "i", primaryKey: true, altKey: true },
  { key: "j", primaryKey: true, altKey: true },
  { key: "c", primaryKey: true, altKey: true },
  { key: "u", primaryKey: true, altKey: true },
];

export function isEditableShortcutTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;

  if (target.isContentEditable) return true;

  const editableAncestor = target.closest('[contenteditable="true"]');
  if (editableAncestor) return true;

  const tagName = target.tagName.toLowerCase();
  return (
    tagName === "input" ||
    tagName === "textarea" ||
    tagName === "select" ||
    tagName === "button"
  );
}

export function isBrowserOnlyShortcut(event: KeyboardEvent): boolean {
  const key = event.key.length === 1 ? event.key.toLowerCase() : event.key;
  const hasPrimaryKey = event.ctrlKey || event.metaKey;

  return BROWSER_ONLY_SHORTCUTS.some((shortcut) => {
    return (
      shortcut.key === key &&
      Boolean(shortcut.primaryKey) === hasPrimaryKey &&
      Boolean(shortcut.shiftKey) === event.shiftKey &&
      Boolean(shortcut.altKey) === event.altKey
    );
  });
}
