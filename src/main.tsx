import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { PreviewPopoutWindow } from "./components/PreviewPopoutWindow";
import { isPopoutRouteSync } from "./services/previewController";
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

// The pop-out window hosts ONLY the preview surface (no second application
// shell). The same frontend bundle detects the `#/preview-popout` route and
// renders the shared preview renderer through `PreviewHost` instead of the
// full application.
const isPreviewPopout = isPopoutRouteSync();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {isPreviewPopout ? <PreviewPopoutWindow /> : <App />}
  </React.StrictMode>,
);
