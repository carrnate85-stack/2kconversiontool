import React from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { StartupIntro } from "./StartupIntro";
import { StartupDecodeProbe } from "./StartupDecodeProbe";
import "./style.css";
const intro = location.search === "?startupIntro=1";
// The main window wears the WPF launcher's own stylesheets; the overlay and
// the startup windows keep style.css only.
const mainWindow =
  !intro &&
  !window.__startupVideoDecodeProbe &&
  !new URLSearchParams(location.search).has("overlay");
if (mainWindow) document.documentElement.classList.add("wpf-ui");
(mainWindow ? import("./wpf/launcher-styles") : Promise.resolve()).then(() =>
  createRoot(document.getElementById("root")!).render(
    intro ? (
      <StartupIntro />
    ) : window.__startupVideoDecodeProbe ? (
      <StartupDecodeProbe />
    ) : (
      <App />
    ),
  ),
);
