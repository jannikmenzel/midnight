import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import App from "./App";
import MenuBarPanel from "./components/MenuBarPanel";
import { ToastProvider } from "./components/ui";
import { dismissSplash } from "./lib/splash";
import "@fontsource/hedvig-letters-serif/400.css";
import "@fontsource-variable/jetbrains-mono/wght.css";
import "./styles.css";

const isPanel = getCurrentWindow().label === "menubar";
if (isPanel) {
  document.documentElement.classList.add("menubar-panel");
  dismissSplash(true);
}

const render = () =>
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      {isPanel ? (
        <MenuBarPanel />
      ) : (
        <ToastProvider>
          <App />
        </ToastProvider>
      )}
    </React.StrictMode>,
  );

Promise.race([document.fonts.load('1em "Hedvig Letters Serif"'), new Promise((resolve) => setTimeout(resolve, 400))])
  .catch(() => {})
  .then(render);
