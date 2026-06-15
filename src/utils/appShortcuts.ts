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
  ctrlKey?: true;
  metaKey?: true;
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
  { key: "F5", primaryKey: true },
  { key: "F5", shiftKey: true },
  { key: "F5", primaryKey: true, shiftKey: true },
  { key: "r", primaryKey: true, shiftKey: true },
  { key: "ArrowLeft", altKey: true },
  { key: "ArrowRight", altKey: true },
  { key: "ArrowLeft", primaryKey: true },
  { key: "ArrowRight", primaryKey: true },
  { key: "[", primaryKey: true, metaKey: true },
  { key: "]", primaryKey: true, metaKey: true },
  { key: "l", primaryKey: true },
  { key: "k", primaryKey: true },
  { key: "e", primaryKey: true },
  { key: "t", primaryKey: true },
  { key: "n", primaryKey: true },
  { key: "w", primaryKey: true },
  { key: "s", primaryKey: true },
  { key: "p", primaryKey: true },
  { key: "f", primaryKey: true },
  { key: "h", primaryKey: true },
  { key: "j", primaryKey: true },
  { key: "o", primaryKey: true },
  { key: "+", primaryKey: true },
  { key: "=", primaryKey: true },
  { key: "-", primaryKey: true },
  { key: "_", primaryKey: true },
  { key: "0", primaryKey: true },
  { key: "i", primaryKey: true, shiftKey: true },
  { key: "j", primaryKey: true, shiftKey: true },
  { key: "c", primaryKey: true, shiftKey: true },
  { key: "u", primaryKey: true, shiftKey: true },
  { key: "s", primaryKey: true, shiftKey: true },
  { key: "p", primaryKey: true, shiftKey: true },
  { key: "f", primaryKey: true, shiftKey: true },
  { key: "h", primaryKey: true, shiftKey: true },
  { key: "j", ctrlKey: true, shiftKey: true },
  { key: "Delete", primaryKey: true, ctrlKey: true, shiftKey: true },
  { key: "i", primaryKey: true, altKey: true },
  { key: "j", primaryKey: true, altKey: true },
  { key: "c", primaryKey: true, altKey: true },
  { key: "u", primaryKey: true, altKey: true },
];

const TEXT_EDITING_SHORTCUT_KEYS = new Set([
  "a",
  "c",
  "v",
  "x",
  "z",
  "y",
  "ArrowLeft",
  "ArrowRight",
  "ArrowUp",
  "ArrowDown",
  "Backspace",
  "Delete",
  "Home",
  "End",
]);

export function normalizeShortcutKey(event: KeyboardEvent): string {
  return event.key.length === 1 ? event.key.toLowerCase() : event.key;
}

export function hasPrimaryShortcutModifier(event: KeyboardEvent): boolean {
  return event.ctrlKey || event.metaKey;
}

export function isStandardClipboardShortcut(event: KeyboardEvent): boolean {
  const key = normalizeShortcutKey(event);
  return (
    hasPrimaryShortcutModifier(event) &&
    !event.altKey &&
    !event.shiftKey &&
    (key === "c" || key === "x" || key === "v")
  );
}

function isTextEditingShortcut(event: KeyboardEvent): boolean {
  if (!(event.target instanceof HTMLElement)) return false;
  if (
    !event.target.isContentEditable &&
    !event.target.closest('[contenteditable="true"]') &&
    event.target.tagName.toLowerCase() !== "input" &&
    event.target.tagName.toLowerCase() !== "textarea" &&
    event.target.tagName.toLowerCase() !== "select"
  ) {
    return false;
  }
  if (!hasPrimaryShortcutModifier(event) || event.altKey) return false;
  return TEXT_EDITING_SHORTCUT_KEYS.has(normalizeShortcutKey(event));
}

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

export function isAppPrimaryShortcut(event: KeyboardEvent): boolean {
  const key = normalizeShortcutKey(event);

  return APP_SHORTCUTS.some((item) => {
    return (
      item.shortcut.key === key &&
      hasPrimaryShortcutModifier(event) &&
      !event.altKey &&
      !event.shiftKey
    );
  });
}

export function isBrowserOnlyShortcut(event: KeyboardEvent): boolean {
  if (isStandardClipboardShortcut(event) || isTextEditingShortcut(event)) {
    return false;
  }

  const key = normalizeShortcutKey(event);
  const hasPrimaryKey = hasPrimaryShortcutModifier(event);

  return BROWSER_ONLY_SHORTCUTS.some((shortcut) => {
    return (
      shortcut.key === key &&
      Boolean(shortcut.primaryKey) === hasPrimaryKey &&
      (shortcut.ctrlKey === undefined ||
        Boolean(shortcut.ctrlKey) === event.ctrlKey) &&
      (shortcut.metaKey === undefined ||
        Boolean(shortcut.metaKey) === event.metaKey) &&
      Boolean(shortcut.shiftKey) === event.shiftKey &&
      Boolean(shortcut.altKey) === event.altKey
    );
  });
}

export function isAppRefreshShortcut(event: KeyboardEvent): boolean {
  if (
    event.key === "F5" &&
    !event.ctrlKey &&
    !event.metaKey &&
    !event.altKey &&
    !event.shiftKey
  ) {
    return true;
  }

  return (
    hasPrimaryShortcutModifier(event) &&
    !event.altKey &&
    !event.shiftKey &&
    normalizeShortcutKey(event) === "r"
  );
}

export function shouldSuppressBrowserShortcutDefault(
  event: KeyboardEvent,
): boolean {
  return (
    isAppRefreshShortcut(event) ||
    isAppPrimaryShortcut(event) ||
    isBrowserOnlyShortcut(event)
  );
}
