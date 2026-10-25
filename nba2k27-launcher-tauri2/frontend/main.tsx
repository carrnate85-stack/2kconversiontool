import React from "react";
import { createRoot } from "react-dom/client";
const intro = location.search === "?startupIntro=1";
const decode = !!window.__startupVideoDecodeProbe;
const mainWindow = !intro && !decode && !new URLSearchParams(location.search).has("overlay");
async function mount() {
  const root=createRoot(document.getElementById("root")!);
  if(intro) { const {StartupIntro}=await import("./StartupIntro"); root.render(<StartupIntro/>); return; }
  if(decode) { const {StartupDecodeProbe}=await import("./StartupDecodeProbe"); root.render(<StartupDecodeProbe/>); return; }
  if(mainWindow)document.documentElement.classList.add("wpf-ui");
  const [{App}]=await Promise.all([import("./App"),import("./style.css"),import("./ux-polish.css"),mainWindow?import("./wpf/launcher-styles"):Promise.resolve()]);
  root.render(<App/>);
}
void mount();
