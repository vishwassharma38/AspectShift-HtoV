import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import {
  isBrowserOnlyShortcut,
  shouldSuppressBrowserShortcutDefault,
} from "./utils/appShortcuts";

const preventContextMenu = (event: MouseEvent) => {
  event.preventDefault();
};

window.addEventListener("contextmenu", preventContextMenu);

const preventBrowserShortcutDefaults = (event: KeyboardEvent) => {
  if (!import.meta.env.PROD || !shouldSuppressBrowserShortcutDefault(event)) {
    return;
  }

  event.preventDefault();

  if (isBrowserOnlyShortcut(event)) {
    event.stopImmediatePropagation();
  }
};

window.addEventListener("keydown", preventBrowserShortcutDefaults, {
  capture: true,
});

if (import.meta.hot) {
  import.meta.hot.dispose(() => {
    window.removeEventListener("contextmenu", preventContextMenu);
    window.removeEventListener("keydown", preventBrowserShortcutDefaults, {
      capture: true,
    });
  });
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
