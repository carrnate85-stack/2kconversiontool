interface CameraState { target: number[]; position?: number[]; distance: number; fittedDistance: number; yaw: number; pitch: number; [key: string]: unknown; }
interface RendererBounds { min: number[]; max: number[]; [key: string]: unknown; }
interface PreviewPick { viewer?: string; name?: string; target?: string; model?: string; primitive?: string; material?: { name: string; shader?: string; lightmap?: string; textures?: { channel: string; name: string }[] }; }
interface WindowEventMap {
  'model-preview:camera:hairCatalog': CustomEvent<CameraState>;
  'model-preview:pick': CustomEvent<PreviewPick>;
}
interface PreviewRenderer {
  clear(): void;
  load(payload: unknown, progress?: (value: { total?: number; phase?: string; completed?: number }) => void): Promise<{ cancelled?: boolean; decodedTextures?: number }>;
  resize(): void;
  fitCamera(bounds?: RendererBounds): void;
  focusHead(): void;
  setView(name: string): void;
  stats(): { payload: unknown; [key: string]: unknown };
  bounds(): RendererBounds;
  cameraState(): CameraState;
  setCameraState(value: CameraState): void;
  setWireframe(value: boolean): void;
  setStudioLighting(value: boolean): void;
  setClayMode(value: boolean): void;
  setGridVisible(value: boolean): void;
  setVisible(value: boolean): void;
  setJerseyVisible(value: boolean): void;
  setEyelashesVisible(value: boolean): void;
  setMaterialTargetVisible(target: string, value: boolean): void;
  setOcclusionBase(value: unknown): void;
  setHeadOcclusionVisible(value: boolean): void;
  setBodyFit(values: Record<string, number>): void;
  setChestCorrection(value: boolean): void;
  suggestBodyFit(values: Record<string, number>): { suggested: Record<string, number>; changed: number; evaluated: number };
}
interface Window {
  modelPreview?: PreviewRenderer;
  hairBasePreview?: PreviewRenderer;
  hairAssetPreview?: PreviewRenderer;
  configsEditor?: { refresh(): Promise<void>; reset(): void; active(): string; renderPreviewSelector(): void; stagingBlockReason?(): string };
  characterModAppearance?: { set(value: string): void };
  lucide?: { createIcons(options?: unknown): void };
  VirtualTable: new <T>(body: HTMLTableSectionElement, renderRow: (item: T) => HTMLTableRowElement, options?: { rowHeight?: number; overscan?: number; columns?: number }) => { setItems(items: T[], reset?: boolean): void };
}
