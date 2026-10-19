import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import type { Garment } from "./types";
export const call = <T>(channel: string, args: unknown = null) =>
  invoke<T>("launcher", { channel, args });
export const garmentImage = (g: Garment) =>
  g.imagePath ? convertFileSrc(g.imagePath) : g.image;
