import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";

const preventContextMenu = (event: MouseEvent) => {
  event.preventDefault();
};

window.addEventListener("contextmenu", preventContextMenu);

if (import.meta.hot) {
  import.meta.hot.dispose(() => {
    window.removeEventListener("contextmenu", preventContextMenu);
  });
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
