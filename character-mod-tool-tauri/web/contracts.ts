export type Game = 'NBA 2K18' | 'NBA 2K19' | 'NBA 2K20' | 'NBA 2K23' | 'NBA 2K25' | 'NBA 2K26' | 'NBA 2K27';
export type AssetType = 'hair' | 'facialhair';
export type JsonValue = string | number | boolean | null | JsonValue[] | { [key: string]: JsonValue };
export type Metadata = Record<string, JsonValue>;
export type Unsubscribe = () => void;
export interface Settings { blender_exe: string; game_root: string; game_root_2k27: string; head_swap_tool: string; mesh_data_transfer: string; output_dir: string; photoshop_exe: string; }
export interface Bootstrap { settings: Settings; engineVersion: string; electronVersion?: string; projectRoot?: string; games: Record<string, boolean>; [key: string]: unknown; }
export interface TargetRequest { target: string; config?: string; configName?: string; includeJersey?: boolean; }
export interface StagePlayerRequest { filePath: string; outputDir?: string; gameLabel?: string; }
export interface StagePlayerResult { mainPath: string; sourcePath: string; gameLabel: string; companions: number; warnings: string[]; directory: string; }
export interface CloseSeamsRequest { target: string; }
export interface CloseSeamsResult { output: string; applied: boolean; correctedVertices: number; pairs: number; maxGapBefore: number; maxGapAfter: number; maxVertexMove?: number; backup?: string; warnings: string[]; }
export interface BlenderRequest extends TargetRequest { includeHair?: boolean; includeFacialHair?: boolean; includeTextures?: boolean; fitChest?: boolean; }
export interface AssetRequest { assetType: AssetType; assetId?: string; includeBase?: boolean; nativeGeometry?: boolean; nativeBinaryGeometry?: boolean; }
export interface MeshGroup { start: number; count: number; material: string; }
export interface PreviewMesh { name: string; sourceName: string; sourceKind: string; positions: Float32Array | number[]; uvs?: Float32Array | number[]; indices: Uint16Array | Uint32Array | number[]; groups?: MeshGroup[]; fitMorphNames?: string[]; fitMorphDeltas?: Float32Array | number[]; [key: string]: unknown; }
export interface PreviewTexture { name: string; bytes?: string | Uint8Array; binding?: Metadata | null; assignments?: Metadata[]; inputColorSpace?: string; range?: Metadata | null; [key: string]: unknown; }
export interface PlayerPreview { mode: string; bodyFit: Record<string, number>; jerseyAvailable: boolean; irisColor?: string; irisSize?: string; defaultConfig?: string; [key: string]: unknown; }
export interface Preview { meshes: PreviewMesh[]; textures: PreviewTexture[]; skipped: string[]; stats: { modelsDecoded: number; triangles: number; companions: number; [key: string]: unknown }; player?: PlayerPreview; name?: string; activeItems?: string[]; companionNames?: string[]; [key: string]: unknown; }
export interface PackedArray { kind: 'f32' | 'u32' | 'u16'; data: string; }
export interface PackedPreviewMesh { name: string; sourceName: string; sourceKind: string; positions: PackedArray; indices: PackedArray; uvs: PackedArray; groups: MeshGroup[]; [key: string]: unknown; }
export interface PackedPreview { meshes: PackedPreviewMesh[]; textures: PreviewTexture[]; skipped: string[]; stats: Preview['stats']; [key: string]: unknown; }
export interface Companion { path: string; name: string; kind: string; }
export interface PreviewOptions { companionPaths?: Companion[]; externalTextures?: Metadata[]; projectTangent?: string; skipTextures?: boolean; maxTriangles?: number; maxModels?: number; hairFitBust?: boolean; preferHighestDetail?: boolean; }
export interface Resources { filePath: string; options: PreviewOptions & { companionPaths: Companion[] }; player: PlayerPreview; name: string; activeItems: string[]; warnings: string[]; fingerprint?: string; native?: boolean; resourceLease?: string; }
export interface NativeAsset { id: string; key: string; sourcePng: string; name: string; playerName: string; hasItem: boolean; tangentspace: boolean; [key: string]: unknown; }
export interface NativeCatalog { assetType: AssetType; assets: NativeAsset[]; gameRoot?: string; sourceRevision?: string; }
export interface AssetDescriptor { filePath?: string; [key: string]: JsonValue | undefined; }
export interface CatalogPreview { asset: Preview; base: Preview; stats: Metadata; [key: string]: unknown; }
export interface Inspection { path: string; name: string; gameLabel?: string; [key: string]: unknown; }
export interface Extraction { mainPath: string; inspected?: Inspection; name: string; png: string; gameLabel: string; warnings: string[]; [key: string]: unknown; }
export interface HairCandidate { path: string; key: string; sourceKey: string; label: string; generation: string; [key: string]: unknown; }
export interface HairSlot { key: string; label: string; default?: boolean; [key: string]: unknown; }
export interface HairOptions { candidates: HairCandidate[]; slots: HairSlot[]; targetGame: string; route?: string; message?: string; }
export interface BodyFit { sourceBodyFit: Record<string, number>; targetBodyFit: Record<string, number>; source: string; sourceRole?: string; sourceName?: string; targetName?: string; sourceOrigin?: string; targetOrigin?: string; changed: string[]; [key: string]: unknown; }
export interface ConfigList { configs: { name: string; items: string[]; texturePresent: boolean }[]; defaultConfig: string; activeConfig?: string; items: { name: string; type: string }[]; [key: string]: unknown; }
export interface Templates { templates: Record<string, { png: string; name: string; hair: string; facialhair: string; headband: string }>; exists: boolean; path: string; }
export interface RosterPlayer { display: Record<string, string>; fields: Record<string, string | number>; identity: string; previousDisplay?: string; valueDisplay?: string; [key: string]: unknown; }
export interface SwapRequest { source: string; target: string; sourceGame: string; targetGame: string; outputDir: string; shrinkwrapBody: boolean; hairEnabled: boolean; facialHairEnabled: boolean; hairSource?: string; hairTargetKey?: string; facialHairSource?: string; facialHairTargetKey?: string; [key: string]: unknown; }
export interface SwapResult { output: string; validation?: { errors: number; warnings: number; [key: string]: unknown }; [key: string]: unknown; }
export interface EngineEvent { command?: string; type?: string; state?: string; message?: string; target?: string; sessionId?: string; jobId?: string; [key: string]: unknown; }
export interface Events { 'job:event': EngineEvent; 'job:complete': { jobId: string; result: SwapResult; output: string }; 'job:failed': { jobId: string; error: string; message: string; cancelled: boolean }; 'blender:event': EngineEvent; }
export interface ReferenceBundle { meshes: { name: string; kind: string; sourceName: string; positions: number[]; uvs: number[]; indices: number[]; groups: MeshGroup[] }[]; textures: PreviewTexture[]; eye: { irisColor?: string; irisSize?: string }; }
export interface NativeCommands {
  'bootstrap': Bootstrap;
  'preview-resources': Resources;
  'native-preview-resources': Resources;
  'native-preview-hair': AssetDescriptor;
  'native-catalog-geometry': { asset: PackedPreview; base: PackedPreview | null; stats: Metadata };
  'read-archive': { members: { name: string; size: number; data: string }[]; geometryKey: string; contentKey: string; native: boolean };
  'preview-cache-stats': Metadata;
  'release-preview-resources': boolean;
  'native-hair-catalog': NativeCatalog;
  'native-hair-preview': AssetDescriptor;
  'extract': Extraction;
  'stage-player': StagePlayerResult;
  'run-swap': SwapResult;
  'runtime-info': { smoke: boolean; data: string; session: string; host: string; previewBinary?: boolean; previewGeometryBinary?: boolean };
  'read-file': { data: string };
  'file-stat': { size: number; mtimeMs: number; path: string };
  'blender-open': unknown;
  'smoke-ready': unknown;
  'cancel-command': unknown;
  'cancel-preview': unknown;
  'reset': unknown;
  'open-path': unknown;
  'show-item': unknown;
  'choose-file': string | null;
  'choose-folder': string | null;
  'inspect': Inspection;
  'catalog': { rows: Metadata[]; supportLabel: string; gameRoot: string };
  'health': unknown;
  'health-repair': unknown;
  'configs-list': ConfigList;
  'configs-change': ConfigList;
  'hair-options': HairOptions;
  'facial-hair-options': HairOptions;
  'native-hair-target': { slots: HairSlot[]; targetPng: string };
  'native-hair-install': { sourceKey: string; targetKey: string; outputs: string[]; backups: string[]; warnings: string[]; messages: string[]; [key: string]: unknown };
  'body-fit': BodyFit;
  'body-fit-save': BodyFit;
  'body-chest-fix': { oldThin: number; newThin: number; oldThick: number; newThick: number };
  'headband-options': { sourceHeadbands: { name: string; path: string }[]; configs: string[]; styles: string[]; defaultConfig: string };
  'headband-apply': { outputs: string[]; sourceName: string; targetKey: string; config: string; [key: string]: unknown };
  'preview-chest-fix': { corrected: number; applied: boolean; peak?: number; output: string };
  'preview-close-seams': CloseSeamsResult;
  'cli-read': RosterPlayer;
  'cli-set': RosterPlayer;
  'cli-language-get': { language: string };
  'cli-language-set': { language: string };
  'save-settings': { settings: Settings };
  'templates-load': Templates;
  'templates-save': Templates;
}
