export type Settings = {
  gameRoot: string;
  modsRoot: string;
  profile: string;
  autoShowOverlay: boolean;
  autoHideOverlay: boolean;
  removeDrone: boolean;
  removeDj: boolean;
  removeMc: boolean;
  removeCrowd: boolean;
  lockCamera: boolean;
  camerasEnabled: boolean;
  startupVideo: boolean;
  camera: string;
  arena: string;
  /** Catalog id behind the saved arena name (names can repeat). */
  arenaId?: string;
  ball: string;
  scoreboard: string;
  scoreboardId?: string;
  theme: "dark" | "light";
  video: string;
  music: string;
  unlockWnba: boolean;
  unlockWnbaFreestyle: boolean;
  /** Experimental V2 mixed NBA+WNBA Freestyle roster (default off). */
  wnbaFreestyleMixed?: boolean;
  wnbaFreestyleVariant?: "wpf-era" | "national" | "clubs";
  scaler: number;
  scalerCoaches: boolean;
  scalerCheerleaders: boolean;
  scalerArenaPersonnel: boolean;
  autoRandomOutfits: boolean;
  usePlaylistInFreestyle: boolean;
};
export type Bootstrap = {
  version: string;
  settings: Settings;
  root: string;
  gameFound: boolean;
  runtime: string;
  liveGameEnabled: false;
  capabilities?: {
    wnbaBlacktop?: boolean;
    debugMenus?: boolean;
    developerTools?: boolean;
    modelPreview?: boolean;
    customBlacktop?: boolean;
  };
  startupMs: number;
};
export type Media = {
  id?: string;
  kind: "Music" | "Videos";
  name: string;
  path: string;
  bytes: number;
  durationMs?: number;
};
export type Mod = { name: string; path: string; bytes: number; folder: string };
export type Player = {
  id: number;
  name: string;
  seasons: number;
  teams: number[];
};
export type Season = {
  id: number;
  year: number;
  primaryTeamId: number;
  secondaryTeamId?: number | null;
  totals: Record<string, number>;
  awards: Record<string, number>;
};
export type PlayerDetail = {
  player: {
    id: number;
    firstName: string;
    lastName: string;
    data: { info: Record<string, string | number>; values: number[] };
  };
  records: Season[];
  teams: { id: number; name: string; city: string; category: string }[];
  fieldKeys: string[];
  unsupported: string[];
};
export type Outfit = {
  name: string;
  top: string;
  bottom: string;
  shoes: string;
  socks?: string;
  pool?: "male" | "female";
  accessories: string[];
  streetId?: string;
  layer?: string;
};
export type Garment = {
  asset: string;
  category: string;
  image?: string;
  imagePath?: string;
  previewKey?: string;
};
export type Library = {
  profiles: { name: string; folder: string }[];
  cameras: {
    id?: number;
    name: string;
    enabled: boolean;
    height: number;
    zoom: number;
  }[];
  playlists: { name: string; tracks: string[] }[];
  outfits: Outfit[];
  tags: Record<string, string>;
};
declare global {
  interface Window {
    __launcherSmoke?: boolean;
    __overlayProbe?: boolean;
    __nativeOverlayProbe?: boolean;
    __startupVideoDecodeProbe?: boolean;
    /** Exact launch-owned bootstrap injected only into the startup-intro view. */
    __startupIntroBootstrap?: unknown;
  }
}
