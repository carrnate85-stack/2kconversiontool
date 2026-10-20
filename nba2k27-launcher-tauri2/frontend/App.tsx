import React, { useEffect, useRef, useState } from "react";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  Bootstrap,
  Settings,
  Media,
  Mod,
  Player,
  PlayerDetail,
  Outfit,
  Garment,
  Library,
} from "./types";
import {
  initialOutfit,
  togglePanel,
  stagedSeasons,
  formatBytes,
  label,
} from "./state";
import {
  cachedOverlayCatalog,
  storeOverlayCatalog,
  type OverlayCatalogRequest,
  type OverlayCatalogResult,
  orderCatalogGroups,
  loadCatalogPages,
  type CatalogPage,
} from "./overlay-catalog-cache";
import samples from "./samples.json";
import { garmentImage } from "./api";
import {
  gameCaptureMessage,
  type GameCaptureEvent,
  type GameCaptureResult,
} from "./GameCapture";
import { Profiles, Mods, Jukebox, Presentation, Blacktop } from "./Workflows";
import parity from "./parity.json";
import { paritySmoke } from "./parity-smoke";
import { PlayerDNA } from "./PlayerDNA";
import { TeamDNA } from "./TeamDNA";
import { EditorEntry, LocalEditorTools } from "./EditorEntry";
import { LaunchReadiness } from "./LaunchReadiness";
import { PlayJukeboxChoice } from "./PlayJukeboxChoice";
import { NativeRules } from "./NativeRules";
import {
  CatalogStatus,
  catalogsPreparing,
  useNativeArena,
} from "./NativeArena";
import { OverlayOutfitEditor } from "./OverlayOutfitEditor";
import {
  NativeOutfitControls,
  OverlayPlayerMenu,
  useNativeOutfits,
  selectedPlayerOutfit,
  automaticOutfitsSupported,
} from "./NativeOutfits";
import {
  AutoRandomPolicy,
  applyRandomBatch,
  autoRandomScanDelay,
  describeRandomResult,
  lineupSignature,
  randomAllTargets,
  randomPlayerModes,
  type RandomMode,
  type RandomSkip,
  type RandomTarget,
} from "./outfit-random";
import type { OverlayStatus } from "./NativeOverlay";
import { nativeOverlayProbe } from "./native-overlay-probe";
import { BallThumbnail } from "./BallThumbnail";
import { ScalerSettings } from "./ScalerSettings";
import { UnlocksPanel, freestyleUnlockMessage } from "./UnlocksPanel";
import { blacktopUnlockAvailable } from "./feature-availability";
import {
  WpfHeader,
  WpfHelpDialog,
  WpfSidebar,
  WpfToast,
  UiIcon,
  wpfPageIds,
} from "./wpf/WpfShell";
import {
  DeveloperTools,
  HotkeyStatus,
  GameBuildCompatibility,
  LauncherHealthCard,
  LauncherUpdatesCard,
  WpfImportCard,
} from "./SettingsTools";

const call = <T,>(channel: string, args: unknown = null) =>
  invoke<T>("launcher", { channel, args });
const pages = [
  "Play session",
  "Profiles",
  "Mod library",
  "Jukebox",
  "Presentation",
  "Roster Editor",
  "Team Editor",
  "Blacktop",
  "Unlocks",
  "Scaler",
  "Settings",
];
const icons: Record<string, string> = {
  "Play session": "play",
  Profiles: "folder",
  "Mod library": "grid",
  Jukebox: "music",
  Presentation: "screen",
  "Roster Editor": "person",
  "Team Editor": "team",
  Blacktop: "court",
  Unlocks: "lock",
  Scaler: "scale",
  Settings: "settings",
};
const pageDescriptions: Record<string, string> = {
  "Play session":
    "Configure your session, load your content, and jump into NBA 2K27.",
  Profiles: "Choose and organize the mod profile used for your next session.",
  "Mod library": "Search, tag, and organize the files in your mod profile.",
  Jukebox: "Build, manage, and preview your custom NBA 2K27 soundtrack.",
  Presentation: "Manage cameras and startup videos for your next session.",
  "Roster Editor": "Edit and transfer player data with guarded verification.",
  "Team Editor": "Edit team identity, staff, strategy, and presentation data.",
  Blacktop: "Customize your streetball setup, presentation, and outfits.",
  Unlocks: "Choose optional offline content unlocks for your next session.",
  Scaler: "Tune supported player scale settings for your next session.",
  Settings: "Manage game folders, launcher behavior, storage, and diagnostics.",
};
function Icon({ name = "grid", size = 20 }: { name?: string; size?: number }) {
  const paths: Record<string, React.ReactNode> = {
    play: <path d="m9 5 11 7-11 7Z" />,
    folder: <path d="M3 7h7l2-3h9v16H3Z" />,
    grid: (
      <>
        <rect x="3" y="3" width="7" height="7" />
        <rect x="14" y="3" width="7" height="7" />
        <rect x="3" y="14" width="7" height="7" />
        <rect x="14" y="14" width="7" height="7" />
      </>
    ),
    music: (
      <>
        <path d="M10 18V5l10-2v13M10 8l10-2" />
        <circle cx="7" cy="18" r="3" />
        <circle cx="17" cy="16" r="3" />
      </>
    ),
    screen: (
      <>
        <rect x="3" y="3" width="18" height="13" />
        <path d="M12 16v5M8 21h8m-7-15 6 4-6 4Z" />
      </>
    ),
    person: (
      <>
        <circle cx="12" cy="7" r="4" />
        <path d="M4 22v-4a8 8 0 0 1 16 0v4" />
      </>
    ),
    team: (
      <>
        <circle cx="8" cy="7" r="3" />
        <circle cx="18" cy="8" r="2" />
        <path d="M2 21v-4a6 6 0 0 1 12 0v4m3-8a5 5 0 0 1 5 5v3" />
      </>
    ),
    court: (
      <>
        <rect x="2" y="5" width="20" height="14" />
        <path d="M12 5v14M2 9h4v6H2m20-6h-4v6h4" />
        <circle cx="12" cy="12" r="3" />
      </>
    ),
    ball: (
      <>
        <circle cx="12" cy="12" r="9" />
        <path d="M3 12h18M12 3v18M5.6 5.6c3.5 3.5 3.5 9.3 0 12.8m12.8-12.8c-3.5 3.5-3.5 9.3 0 12.8" />
      </>
    ),
    lock: (
      <>
        <rect x="4" y="10" width="16" height="11" rx="2" />
        <path d="M8 10V6a4 4 0 0 1 8 0v4m-4 5v3" />
      </>
    ),
    scale: (
      <>
        <path d="M4 4v16h16M8 4h12v12M8 8h8v8" />
      </>
    ),
    settings: (
      <>
        <path d="M3 6h18M3 12h18M3 18h18" />
        <path d="M8 3v6m8 0v6m-7 0v6" />
      </>
    ),
  };
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {paths[name] ?? paths.grid}
    </svg>
  );
}
// The WPF F8 bar's own icons (renderer/sidebar-icons.js), drawn the same way.
const barIconPaths: Record<string, string[]> = {
  crown: ["M2 7l5 5 5-8 5 8 5-5-2 13H4z", "M4 20h16", "M7 4v2", "M17 4v2"],
  players: [
    "M15.5 7a3.5 3.5 0 1 1-7 0 3.5 3.5 0 0 1 7 0",
    "M5 21v-3a7 7 0 0 1 14 0v3",
  ],
  arena: [
    "M3 4h18v16H3z",
    "M12 4v16",
    "M15 12a3 3 0 1 1-6 0 3 3 0 0 1 6 0",
    "M3 9h3v6H3",
    "M21 9h-3v6h3",
  ],
  scoreboard: ["M3 4h18v14H3z", "M10 8l5 3-5 3z", "M12 18v3", "M8 21h8"],
  ball: [
    "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18",
    "M3 12h18",
    "M12 3v18",
    "M5 6c7 2 7 10 0 12",
    "M19 6c-7 2-7 10 0 12",
  ],
  check: ["M3 12l6 6L21 5"],
  hide: [
    "M3 3l18 18",
    "M10.6 5.1A10 10 0 0 1 12 5c5.5 0 9 7 9 7a17 17 0 0 1-3.2 3.5",
    "M6.2 6.2A17 17 0 0 0 3 12s3.5 7 9 7c1 0 2-.2 2.9-.6",
    "M10 10a3 3 0 0 0 4 4",
  ],
};
const wpfChoiceLabel = (name: string) =>
  name === "Currently installed" ? "Current installed" : name;
function BarIcon({ name }: { name: string }) {
  return (
    <svg
      className="nav-icon"
      viewBox="0 0 24 24"
      aria-hidden="true"
      focusable="false"
    >
      {barIconPaths[name].map((d) => (
        <path key={d} d={d} />
      ))}
    </svg>
  );
}
function Switch({
  label: caption,
  value,
  onChange,
  note,
  disabled = false,
}: {
  label: string;
  value: boolean;
  onChange: (v: boolean) => void;
  note?: string;
  disabled?: boolean;
}) {
  return (
    <label className="switch-row">
      <span>
        {caption}
        {note && <small>{note}</small>}
      </span>
      <input
        type="checkbox"
        checked={value}
        disabled={disabled}
        onChange={(e) => onChange(e.target.checked)}
      />
      <span className="toggle" aria-hidden="true" />
    </label>
  );
}
function Card({
  title,
  children,
  action,
}: {
  title: string;
  children: React.ReactNode;
  action?: React.ReactNode;
}) {
  // WPF settings card: .settings-card-heading over plain content.
  return (
    <article className="card settings-option-card">
      <div className="settings-card-heading">
        <strong>{title}</strong>
        {action}
      </div>
      <div className="card-content">{children}</div>
    </article>
  );
}
function Empty({ text }: { text: string }) {
  return (
    <div className="empty">
      <Icon name="folder" size={30} />
      <p>{text}</p>
    </div>
  );
}
function ErrorBox({ text }: { text: string }) {
  return text ? (
    <div className="error" role="alert">
      {text}
    </div>
  ) : null;
}
const outfitSlots = [
  "top",
  "bottom",
  "socks",
  "shoes",
  "headband",
  "eyewear",
  "faceMask",
  "headwear",
  "headphones",
  "leftArm",
  "rightArm",
  "leftLeg",
  "rightLeg",
  "backpack",
  "leftHand",
  "rightHand",
  "leftWrist",
  "rightWrist",
  "neckwear",
  "waist",
  "bodywear",
  "leftPiercing",
  "rightPiercing",
  "other",
];
function Overlay({
  settings,
  change,
  outfit,
  setOutfit,
  onClose,
  standalone = false,
  startPanel = null,
  live = false,
}: {
  settings: Settings;
  change: (p: Partial<Settings>) => void;
  outfit: Outfit;
  setOutfit: (o: Outfit) => void;
  onClose: () => void | Promise<unknown>;
  standalone?: boolean;
  startPanel?: string | null;
  live?: boolean;
}) {
  const [panel, setPanel] = useState<string | null>(startPanel),
    [query, setQuery] = useState(""),
    [slot, setSlot] = useState("top"),
    [garments, setGarments] = useState<Garment[]>([]),
    [offset, setOffset] = useState(0),
    [total, setTotal] = useState(0),
    [group, setGroup] = useState("All"),
    [groups, setGroups] = useState<string[]>([]),
    // WPF arena-favorites.json: null until read, so stars stay disabled.
    [favorites, setFavorites] = useState<string[] | null>(null),
    [favoriteBusy, setFavoriteBusy] = useState(""),
    [options, setOptions] = useState<
      {
        id: string;
        name: string;
        group: string;
        image?: string;
        imagePath?: string;
        file?: string;
        caution?: string | null;
        disabled?: boolean;
      }[]
    >([]);
  const [message, setMessage] = useState(
    "Offline preview · dummy player · no game writes",
  );
  const request = useRef(0);
  const garmentCache = useRef(new Map<string, OverlayCatalogResult<Garment>>());
  const choiceCache = useRef(
    new Map<
      string,
      OverlayCatalogResult<{
        id: string;
        name: string;
        group: string;
        image?: string;
        imagePath?: string;
        file?: string;
        caution?: string | null;
        disabled?: boolean;
      }>
    >(),
  );
  const [nativeStatus, setNativeStatus] = useState<OverlayStatus | null>(null);
  const [closeError, setCloseError] = useState("");
  const arena = useNativeArena(
    live,
    settings.gameRoot,
    "arena",
    panel === "arena",
  );
  const ball = useNativeArena(
    live,
    settings.gameRoot,
    "ball",
    panel === "ball",
  );
  const scoreboard = useNativeArena(
    live,
    settings.gameRoot,
    "scoreboard",
    panel === "scoreboard",
  );
  const [randomRunning, setRandomRunning] = useState(false),
    [randomNote, setRandomNote] = useState("");
  // WPF's compact dropdowns under the bar (not catalog panels): the PLAYER
  // list, the Random player modes, and the Random all per-player report.
  const [menu, setMenu] = useState<"player" | "random" | "report" | null>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  // WPF reopened the Random menu after a player was chosen from it.
  const reopenRandom = useRef(false);
  const clothing = useNativeOutfits(
    live,
    settings.gameRoot,
    !!nativeStatus?.visible ||
      panel === "outfit" ||
      menu !== null ||
      randomRunning,
  );
  const clothingRef = useRef(clothing);
  clothingRef.current = clothing;
  const autoRandom = useRef(new AutoRandomPolicy());
  // WPF scanned the lineup whenever its player menu/editor opened. Scan once
  // per open, as soon as the worker is free (opening while busy used to skip
  // the scan for good and leave the dropdown empty).
  const outfitView =
    panel === "outfit" || menu === "player" || menu === "random"
      ? `${panel}:${menu}`
      : "";
  const scannedView = useRef("");
  useEffect(() => {
    if (!outfitView) {
      scannedView.current = "";
      return;
    }
    if (
      live &&
      clothing.eligible &&
      !clothing.busy &&
      scannedView.current !== outfitView
    ) {
      scannedView.current = outfitView;
      // With a lineup already shown, re-read it invisibly (no busy flicker).
      void (clothing.lineup ? clothing.refresh() : clothing.scan());
    }
  }, [outfitView, live, clothing.eligible, clothing.busy]);
  // Sequential random applies through the existing outfit queue. Every player
  // waits for its own verified reply; any error, hold or lineup change stops.
  async function runRandom(
    targets: RandomTarget[],
    mode: RandomMode,
    automatic: boolean,
    skipped: readonly RandomSkip[] = [],
  ) {
    if (randomRunning) return;
    setRandomRunning(true);
    const label = automatic ? "Automatic random" : "Random outfits";
    // A clothing-pool template is immutable during this batch. Fetch it once
    // per pool, while still generating an independent random fit per player.
    const templates = new Map<string, Promise<Outfit>>();
    function template(pool: "male" | "female") {
      let pending = templates.get(pool);
      if (!pending) {
        pending = call<Outfit>("outfit-for-pool", { pool });
        templates.set(pool, pending);
      }
      return pending;
    }
    try {
      const result = await applyRandomBatch(targets, {
        outfitFor: async (t) =>
          call<Outfit>("random-outfit", {
            mode,
            // WPF random-all kept each player's roster shoes.
            outfit: {
              ...(await template(t.pool)),
              shoes: "Currently installed",
              // Batch randomization is streetwear only. Accessories remain
              // an explicit per-player choice in the outfit editor.
              accessories: [],
            },
          }),
        applyMany: async (items) => {
          // Marked before the write: an uncertain result never re-applies.
          for (const item of items) autoRandom.current.mark(item.playerBinding);
          const revision = await clothingRef.current.queueMany(items, automatic);
          // Refused before queueing: nothing was written, so a later
          // automatic pass may try these players again.
          if (revision === null)
            for (const item of items)
              autoRandom.current.unmark(item.playerBinding);
          return revision;
        },
        wait: (revision, timeoutMs) =>
          clothingRef.current.wait(revision, timeoutMs),
        onStep: (text) => setRandomNote(`${label}: ${text}`),
      });
      setRandomNote(
        `${label}: ${describeRandomResult(result, skipped, targets.length + skipped.length)}`,
      );
      // Routine progress and success stay quiet; show actionable failures.
      if (result.stopped || result.outcomes.some((o) => !o.ok))
        setMenu("report");
    } finally {
      setRandomRunning(false);
    }
  }
  async function randomPlayer(mode: RandomMode) {
    const c = clothingRef.current,
      player = c.player;
    if (!player) {
      setRandomNote("Choose a player from the PLAYER list first.");
      return;
    }
    if (!c.eligible || c.busy || c.status?.boundary !== "selection") {
      setRandomNote(
        c.unavailable || "Random outfits apply at Blacktop player selection.",
      );
      return;
    }
    if (player.pool !== (outfit.pool ?? "male")) {
      setRandomNote("Preparing this player's clothing pool; try again.");
      return;
    }
    try {
      const o = await call<Outfit>("random-outfit", { mode, outfit });
      setOutfit(o);
      setRandomNote(`Random outfit sent for ${player.name}.`);
      await c.apply(o);
    } catch (e) {
      setRandomNote(String(e));
    }
  }
  function randomAll() {
    if (!live) {
      setMessage(
        "Random all players is only available while the game is running.",
      );
      return;
    }
    // One click randomizes every readable selection player (WPF).
    setMenu(null);
    const c = clothingRef.current;
    if (!c.eligible) {
      setRandomNote(c.unavailable || "Outfit changes are not available yet.");
      setMenu("report");
      return;
    }
    // WPF blacktop-batch.js: every selected player must have a verified fit.
    const found = randomAllTargets(c.status, true);
    if (typeof found === "string") {
      setRandomNote(found);
      setMenu("report");
      if (!c.busy) void c.scan();
      return;
    }
    void runRandom(found.targets, "clothes", false, found.skipped);
  }
  // WPF autoRandomSelection parity: on an eligible game,
  // each new lineup player without a manual choice gets one random fit once
  // the same lineup has been read twice. Lineup reads are read-only.
  const autoOn = live && settings.autoRandomOutfits !== false && automaticOutfitsSupported(clothing.status);
  useEffect(() => {
    if (!autoOn || !clothing.eligible || randomRunning || clothing.busy) return;
    const s = clothing.status,
      policy = autoRandom.current;
    for (const b of clothing.manual) policy.mark(b);
    const due = policy.observe(clothing.session, s);
    if (due.length) {
      void runRandom(due, "clothes", true);
      return;
    }
    // Never refresh under the user's own Edit outfit work.
    if (panel === "outfit") return;
    const delay = autoRandomScanDelay(
      s,
      !!s?.selection && policy.seenAll(s.selection.players),
      !!nativeStatus?.visible,
    );
    if (delay === null) return;
    const timer = setTimeout(() => {
      const c = clothingRef.current;
      // Background read: keeps showing the last lineup, never toggles busy.
      if (!c.working && c.eligible) void c.refresh();
    }, delay);
    return () => clearTimeout(timer);
  }, [
    autoOn,
    clothing.eligible,
    clothing.busy,
    clothing.session,
    clothing.status?.update,
    randomRunning,
    panel,
    nativeStatus?.visible,
  ]);
  useEffect(() => {
    const player = clothing.player;
    if (!live || !player?.pool) return;
    const worn = selectedPlayerOutfit(player);
    if (worn) setOutfit(worn);
    else {
      // Unknown uniforms must not masquerade as the previous player's draft.
      setOutfit({
        ...initialOutfit,
        pool: player.pool,
        top: "",
        bottom: "",
        streetId: "",
        shoes: "Currently installed",
        socks: "",
        layer: "",
        accessories: [],
      });
    }
  }, [
    live,
    clothing.player?.binding,
    clothing.player?.pool,
    JSON.stringify(clothing.player?.currentOutfit),
    panel === "outfit",
  ]);
  // WPF wording: the stock ball is "Current installed"; no scoreboard
  // request shows the stock "Default".
  const ballName = wpfChoiceLabel(
    live ? (ball.status?.requestedName ?? "Current installed") : settings.ball,
  );
  const scoreboardName = live
    ? (scoreboard.status?.requestedName ?? "Default")
    : settings.scoreboard;
  const arenaName = live
    ? (arena.status?.requestedName ?? settings.arena)
    : settings.arena;
  const catalogControl = (p: string | null) =>
    p === "arena"
      ? arena
      : p === "ball"
        ? ball
        : p === "scoreboard"
          ? scoreboard
          : null;
  const preparing = catalogsPreparing({
    arena: arena.status,
    ball: ball.status,
    scoreboard: scoreboard.status,
  });
  // WPF's bar badge: Ready, Loading or Check status; detail in the tooltip.
  const barStatus = !live
    ? { text: "Preview", state: "ready", title: message }
    : closeError
      ? { text: "Check status", state: "error", title: closeError }
      : arena.busy || ball.busy || scoreboard.busy
        ? { text: "Loading", state: "loading", title: "Applying choice…" }
        : nativeStatus?.boundary === "selection" && preparing.length
          ? {
              text: "Preparing",
              state: "loading",
              title: `${preparing.map((k) => k[0].toUpperCase() + k.slice(1)).join(", ")} preparing… wait for Ready before starting the match.`,
            }
          : nativeStatus?.boundary === "selection" ||
              nativeStatus?.boundary === "match"
            ? {
                text: "Ready",
                state: "ready",
                title:
                  nativeStatus.message || `Blacktop ${nativeStatus.boundary}`,
              }
            : {
                text: "Waiting",
                state: "loading",
                title:
                  nativeStatus?.message ||
                  "Waiting for Blacktop player selection",
              };
  const barRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!live || !nativeStatus?.enabled || window.__nativeOverlayProbe) return;
    const timer = setInterval(() => {
      void call("native-overlay-heartbeat").catch(() => {});
    }, 2000);
    return () => clearInterval(timer);
  }, [live, nativeStatus?.enabled]);
  useEffect(() => {
    if (!live) return;
    let disposed = false;
    const registered = listen<OverlayStatus>("native-overlay-state", (e) => {
      if (disposed) return;
      setNativeStatus(e.payload);
      if (!e.payload.visible && !e.payload.catalog) setPanel(null);
    });
    void call<OverlayStatus>("native-overlay-status")
      .then(setNativeStatus)
      .catch((e) => setMessage(String(e)));
    return () => {
      disposed = true;
      void registered.then((stop) => stop());
    };
  }, [live]);
  useEffect(() => {
    if (!live || !barRef.current) return;
    const bar = barRef.current;
    let last = "",
      disposed = false;
    function layout() {
      if (disposed) return;
      const barHeight = Math.max(
        70,
        Math.min(
          600,
          // WPF sized the compact window to the bar plus its 4px margins
          // (78 px wide-screen, 120 px two-row).
          Math.ceil(
            (Math.max(
              bar.getBoundingClientRect().bottom,
              menuRef.current?.getBoundingClientRect().bottom ?? 0,
            ) +
              4) *
              devicePixelRatio,
          ),
        ),
      );
      const key = `${panel}:${menu}:${barHeight}`;
      if (key === last) return;
      last = key;
      // User decision (intentional WPF divergence): the arena picker is the
      // same expanded catalog as ball/scoreboard, not WPF's arena dropdown.
      void call("native-overlay-layout", {
        catalog: !!panel,
        barHeight,
        dropdown: false,
      }).catch((e) => {
        if (!disposed) setMessage(String(e));
      });
    }
    const observer = new ResizeObserver(layout);
    observer.observe(bar);
    if (menuRef.current) observer.observe(menuRef.current);
    layout();
    return () => {
      disposed = true;
      observer.disconnect();
    };
  }, [live, panel, menu]);
  // WPF: Escape returns an open catalog to the compact bar, otherwise hides.
  useEffect(() => {
    function key(e: KeyboardEvent) {
      if (e.key !== "Escape" || e.repeat) return;
      e.preventDefault();
      if (menu) setMenu(null);
      else if (panel) setPanel(null);
      else if (live)
        void Promise.resolve(onClose()).catch((err) =>
          setCloseError(`Could not hide overlay: ${String(err)}`),
        );
    }
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [menu, panel, live, onClose]);
  // WPF closed its dropdowns on any pointer-down outside them.
  useEffect(() => {
    if (!menu) return;
    function outside(e: PointerEvent) {
      const t = e.target as Node;
      if (menuRef.current?.contains(t) || barRef.current?.contains(t)) return;
      setMenu(null);
    }
    document.addEventListener("pointerdown", outside);
    return () => document.removeEventListener("pointerdown", outside);
  }, [menu]);
  function open(next: string) {
    const target = togglePanel(panel, next);
    setPanel(target);
    if (!target) return;
    setQuery("");
    setOffset(0);
    setGroup("All");
    const base: OverlayCatalogRequest = {
      kind: target,
      query: "",
      group: "All",
      offset: 0,
    };
    if (target === "outfit") {
      const cached = cachedOverlayCatalog(garmentCache.current, {
        ...base,
        slot,
        pool: outfit.pool ?? "male",
      });
      setGarments(cached?.items ?? []);
      setTotal(cached?.total ?? 0);
      setGroups(cached?.groups ?? []);
      setOptions([]);
      return;
    }
    if (["ball", "arena", "scoreboard"].includes(target)) {
      const cached = cachedOverlayCatalog(choiceCache.current, base);
      setOptions(cached?.items ?? []);
      setTotal(cached?.total ?? 0);
      setGroups(cached?.groups ?? []);
      setGarments([]);
      return;
    }
    setOptions([]);
    setGroups([]);
    setGarments([]);
    setTotal(0);
  }
  useEffect(() => {
    if (panel !== "outfit") return;
    const catalogRequest: OverlayCatalogRequest = {
      kind: panel,
      slot,
      pool: outfit.pool ?? "male",
      query,
      group,
      offset,
    };
    const cached = cachedOverlayCatalog(garmentCache.current, catalogRequest);
    if (cached) {
      setGarments(cached.items);
      setTotal(cached.total);
      setGroups(cached.groups);
      return;
    }
    const id = ++request.current;
    const timer = setTimeout(
      () => {
        call<{ items: Garment[]; total: number; groups: string[] }>(
          "wardrobe",
          {
            slot,
            query,
            group,
            offset,
            limit: 24,
            pool: outfit.pool ?? "male",
          },
        )
          .then((r) => {
            if (id === request.current) {
              storeOverlayCatalog(garmentCache.current, catalogRequest, r);
              setGarments(r.items);
              setTotal(r.total);
              setGroups(r.groups);
            }
          })
          .catch((e) => setMessage(String(e)));
      },
      query ? 100 : 0,
    );
    return () => {
      clearTimeout(timer);
      request.current++;
    };
  }, [panel, slot, query, group, offset, outfit.pool]);
  useEffect(() => {
    if (panel !== "arena" || favorites !== null) return;
    let alive = true;
    call<string[]>("arena-favorites")
      .then((ids) => {
        if (alive) setFavorites(ids);
      })
      .catch((e) => setMessage(String(e)));
    return () => {
      alive = false;
    };
  }, [panel, favorites]);
  async function toggleFavorite(id: string, name: string, favorite: boolean) {
    setFavoriteBusy(id);
    try {
      setFavorites(await call<string[]>("arena-favorite", { id, favorite }));
    } catch (e) {
      setMessage(`Could not save favorite ${name}: ${String(e)}`);
    } finally {
      setFavoriteBusy("");
    }
  }
  useEffect(() => {
    if (!panel || !["ball", "arena", "scoreboard"].includes(panel)) return;
    const catalogRequest: OverlayCatalogRequest = {
      kind: panel,
      query,
      group,
      offset,
    };
    // Favorites change while the panel is open; never serve them from cache.
    const cacheable = group !== "Favorites";
    const cached = cacheable
      ? cachedOverlayCatalog(choiceCache.current, catalogRequest)
      : null;
    if (cached) {
      setOptions(cached.items);
      setTotal(cached.total);
      setGroups(cached.groups);
      return;
    }
    let alive = true;
    const timer = setTimeout(
      () => {
        // Every catalog, arena included, pages by 24 like the ball picker.
        loadCatalogPages(
          (at) =>
            call<CatalogPage<(typeof options)[number]>>("choices", {
              kind: panel,
              query,
              group,
              offset: at,
            }),
          false,
          offset,
        )
          .then((r) => {
            if (alive) {
              if (cacheable)
                storeOverlayCatalog(choiceCache.current, catalogRequest, r);
              setOptions(r.items);
              setTotal(r.total);
              setGroups(orderCatalogGroups(panel, r.groups));
            }
          })
          .catch((e) => setMessage(String(e)));
      },
      query ? 100 : 0,
    );
    return () => {
      alive = false;
      clearTimeout(timer);
    };
  }, [panel, query, group, offset, group === "Favorites" && favorites]);
  // Warm the arena catalog's first page so the picker opens filled at once
  // (WPF loaded its arena state at startup).
  useEffect(() => {
    const request: OverlayCatalogRequest = {
      kind: "arena",
      query: "",
      group: "All",
      offset: 0,
    };
    if (cachedOverlayCatalog(choiceCache.current, request)) return;
    let alive = true;
    loadCatalogPages(
      (at) =>
        call<CatalogPage<(typeof options)[number]>>("choices", {
          kind: "arena",
          query: "",
          group: "All",
          offset: at,
        }),
      false,
    )
      .then((r) => {
        if (!alive) return;
        storeOverlayCatalog(choiceCache.current, request, r);
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, []);
  // WPF ball picker "Show selected": clear filters, open the selected ball's page.
  async function showSelectedBall() {
    setQuery("");
    setGroup("All");
    const wanted = (o: { id: string; name: string }) =>
      live && ball.status
        ? o.id === ball.status.requestedId
        : o.name === settings.ball;
    try {
      for (let at = 0; at < 2000; at += 24) {
        const page = await call<{ items: typeof options; total: number }>(
          "choices",
          { kind: "ball", query: "", group: "All", offset: at },
        );
        const index = page.items.findIndex(wanted);
        if (index >= 0) return setOffset(at);
        if (at + 24 >= page.total) break;
      }
      setOffset(0);
    } catch (e) {
      setMessage(String(e));
    }
  }
  function pickAsset(g: Garment) {
    call<Outfit>("select-outfit-piece", { outfit, slot, asset: g.asset })
      .then(setOutfit)
      .catch((e) => setMessage(String(e)));
  }
  return (
    <div
      className={`overlay-demo ${standalone ? "standalone" : ""} ${live ? "native-live" : ""}`}
    >
      <div className="overlay-bar" data-testid="overlay-bar" ref={barRef}>
        <div className="overlay-brand" aria-label="Blacktop">
          <BarIcon name="crown" />
        </div>
        <div className="overlay-item mode">
          <small>MODE</small>
          <span className="overlay-static">Blacktop</span>
        </div>
        <div className="overlay-item player">
          <small>PLAYER</small>
          <button
            aria-haspopup="menu"
            aria-expanded={menu === "player"}
            aria-controls="overlay-dropdown"
            onClick={() => {
              if (!live) {
                setMessage("Dummy player selected · Stephen Curry");
                return;
              }
              if (menu === "player") {
                setMenu(null);
                return;
              }
              reopenRandom.current = menu === "random";
              setMenu("player");
            }}
          >
            {live
              ? (clothing.player?.name ?? "Select players in game")
              : "Stephen Curry"}{" "}
            <span>⌄</span>
          </button>
        </div>
        <div className="overlay-actions">
          <button
            className={panel === "outfit" ? "active" : ""}
            aria-expanded={panel === "outfit"}
            aria-controls="overlay-catalog"
            onClick={() => open("outfit")}
          >
            Edit outfit <span>⌄</span>
          </button>
          <button
            className={menu === "random" ? "active" : ""}
            aria-haspopup="menu"
            aria-expanded={menu === "random"}
            aria-controls="overlay-dropdown"
            onClick={() =>
              setMenu((current) => (current === "random" ? null : "random"))
            }
          >
            Random player <span>⌄</span>
          </button>
          <button disabled={randomRunning} onClick={randomAll}>
            Random all players
          </button>
        </div>
        {(["arena", "scoreboard", "ball"] as const).map((p) => (
          <div className={`overlay-item presentation ${p}`} key={p}>
            <small>{p.toUpperCase()}</small>
            <BarIcon name={p} />
            <button
              className={panel === p ? "active" : ""}
              aria-expanded={panel === p}
              aria-controls="overlay-catalog"
              data-phase={live ? catalogControl(p)?.status?.phase : undefined}
              title={
                live
                  ? catalogControl(p)?.error ||
                    catalogControl(p)?.status?.message ||
                    catalogControl(p)?.unavailable ||
                    undefined
                  : undefined
              }
              onClick={() => open(p)}
            >
              {p === "ball"
                ? ballName
                : p === "scoreboard"
                  ? scoreboardName
                  : arenaName}
            </button>
          </div>
        ))}
        <div className="overlay-item rules">
          <small>RULES</small>
          <button
            className={panel === "rules" ? "active" : ""}
            aria-expanded={panel === "rules"}
            aria-controls="overlay-catalog"
            onClick={() => open("rules")}
          >
            Game rules
          </button>
        </div>
        <div className="overlay-item status">
          <small>STATUS</small>
          <span
            className="ready"
            role="status"
            data-state={barStatus.state}
            title={barStatus.title}
          >
            <BarIcon name="check" />
            {barStatus.text}
          </span>
        </div>
        <div className="overlay-item hide">
          <small>OVERLAY</small>
          <button
            title={closeError || "Hide overlay (F8 or Escape)"}
            onClick={async () => {
              setCloseError("");
              try {
                await onClose();
              } catch (e) {
                setCloseError(`Could not hide overlay: ${String(e)}`);
              }
            }}
          >
            <BarIcon name="hide" />
            {closeError ? "Retry hide" : "Hide overlay"}
          </button>
        </div>
      </div>
      {menu && (
        <div
          className={`overlay-dropdown ${menu}`}
          id="overlay-dropdown"
          ref={menuRef}
        >
          {menu === "player" &&
            (live ? (
              <OverlayPlayerMenu
                control={clothing}
                onChoose={() => {
                  setMenu(reopenRandom.current ? "random" : null);
                  reopenRandom.current = false;
                }}
              />
            ) : null)}
          {menu === "random" && (
            <>
              <div className="overlay-random-modes" role="menu">
                {randomPlayerModes.map(([mode, text]) => (
                  <button
                    key={mode}
                    role="menuitem"
                    disabled={live && (randomRunning || clothing.busy)}
                    onClick={() =>
                      live
                        ? void randomPlayer(mode)
                        : call<Outfit>("random-outfit", { mode, outfit })
                            .then((o) => {
                              setOutfit(o);
                              setRandomNote(
                                "Dummy outfit randomized · shoes unchanged · maximum 3 accessories",
                              );
                            })
                            .catch((e) => setRandomNote(String(e)))
                    }
                  >
                    {text}
                  </button>
                ))}
              </div>
            </>
          )}
          {menu === "report" && (
            <p role="status" className="overlay-random-report">
              {randomNote || clothing.error || "Random all players"}
            </p>
          )}
        </div>
      )}
      {panel && (
        <section className="overlay-catalog" id="overlay-catalog">
          {
            <header>
              <div>
                <small>BLACKTOP</small>
                <h2>
                  {panel === "outfit"
                    ? "Edit outfit"
                    : panel === "rules"
                      ? "Game rules"
                      : `${panel[0].toUpperCase() + panel.slice(1)} catalog`}
                </h2>
              </div>
              <button className="primary" onClick={() => setPanel(null)}>
                Back to game
              </button>
            </header>
          }
          {["ball", "arena", "scoreboard"].includes(panel) &&
            (live && catalogControl(panel)?.unavailable ? (
              <CatalogStatus
                live
                control={catalogControl(panel)!}
                feature={panel as "scoreboard"}
              />
            ) : null)}
          {["ball", "arena", "scoreboard"].includes(panel) && (
            <section className="search-row catalog-controls">
              <input
                type="search"
                aria-label="Search catalog"
                placeholder={
                  panel === "arena"
                    ? "Search team, court or arena file…"
                    : panel === "outfit"
                      ? "Search names or categories"
                      : panel === "scoreboard"
                        ? "Search scoreboard names"
                        : `Search ${panel} names or categories`
                }
                value={query}
                onChange={(e) => {
                  setQuery(e.target.value);
                  setOffset(0);
                }}
              />
              {panel === "ball" && (
                <button type="button" onClick={() => void showSelectedBall()}>
                  Show selected
                </button>
              )}
              {panel === "outfit" && (
                <select
                  aria-label="Outfit slot"
                  value={slot}
                  onChange={(e) => {
                    setSlot(e.target.value);
                    setOffset(0);
                    setGroup("All");
                  }}
                >
                  {outfitSlots.map((s) => (
                    <option key={s} value={s}>
                      {label(s)}
                    </option>
                  ))}
                </select>
              )}
              <div
                className="catalog-filters"
                role="group"
                aria-label="Catalog category"
              >
                {[
                  "All",
                  ...(panel === "arena" ? ["Favorites"] : []),
                  ...orderCatalogGroups(panel, groups),
                ].map((g) => (
                  <button
                    key={g}
                    type="button"
                    aria-pressed={g === group}
                    onClick={() => {
                      setGroup(g);
                      setOffset(0);
                    }}
                  >
                    {g === "Favorites" ? "★ Favorites" : g}
                  </button>
                ))}
              </div>
            </section>
          )}
          <div className="catalog-content">
            {catalogControl(panel) &&
              options.length === 0 &&
              total === 0 &&
              (query || group !== "All") && (
                <div className="catalog-empty">
                  <strong>
                    {group === "Favorites" && !query && !favorites?.length
                      ? "Star an arena to find it here."
                      : `No ${panel === "ball" ? "balls" : `${panel}s`} match those filters.`}
                  </strong>
                  <span>Try another name or show every category.</span>
                  <button
                    type="button"
                    onClick={() => {
                      setQuery("");
                      setGroup("All");
                      setOffset(0);
                    }}
                  >
                    Reset filters
                  </button>
                </div>
              )}
            {options.length > 0 && (
              <div className={`catalog-grid ${panel}-grid`}>
                {options.map((o) => {
                  const control = catalogControl(panel);
                  const selecting =
                    !!control?.busy && control.pendingId === o.id;
                  const selected =
                    live && control?.status
                      ? control.status.requestedId === o.id
                      : settings[panel as "ball"] === o.name &&
                        (panel !== "arena" ||
                          !settings.arenaId ||
                          settings.arenaId === o.id);
                  const tile = (
                    <button
                      className={`catalog-tile ${selected ? "selected" : ""}`}
                      key={o.id}
                      aria-pressed={selected}
                      aria-busy={selecting || undefined}
                      // WPF arena-picker.js: unfinished arenas stay listed but
                      // cannot be picked; the tooltip says why.
                      disabled={
                        !!control?.busy || (panel === "arena" && !!o.disabled)
                      }
                      title={
                        control?.unavailable ||
                        control?.error ||
                        (panel === "arena"
                          ? o.caution || o.file || "Uses Day / Night"
                          : undefined)
                      }
                      onClick={async () => {
                        // WPF: picking an item is the choice. It applies at
                        // Blacktop player selection and returns to the bar.
                        if (control && !(await control.select(o.id))) return;
                        // The id disambiguates repeated names; the next
                        // mod-profile launch applies this choice automatically.
                        change({
                          [panel]: o.name,
                          ...(panel === "arena"
                            ? { arenaId: o.id }
                            : panel === "scoreboard"
                              ? { scoreboardId: o.id }
                              : {}),
                        });
                        setMessage(
                          control && live
                            ? "Request queued; wait for verified Ready before advancing."
                            : "Saved. It applies the next time you play.",
                        );
                        if (control) setPanel(null);
                      }}
                    >
                      {panel === "ball" && o.id === "current" ? (
                        <span className="tile-current">CURRENT</span>
                      ) : panel === "ball" ? (
                        <BallThumbnail id={o.id} gameRoot={settings.gameRoot}
                          image={o.imagePath ? convertFileSrc(o.imagePath) : o.image}
                          imageClassName=""
                          placeholder={<span className="tile-symbol"><Icon name="ball" size={40} /></span>} />
                      ) : o.imagePath || o.image ? (
                        <img
                          src={
                            o.imagePath ? convertFileSrc(o.imagePath) : o.image
                          }
                          alt=""
                        />
                      ) : (
                        <div className="tile-symbol">
                          <Icon
                            name={
                              panel === "ball"
                                ? "ball"
                                : panel === "arena"
                                  ? "court"
                                  : "screen"
                            }
                            size={40}
                          />
                        </div>
                      )}
                      <strong>
                        {wpfChoiceLabel(o.name)}
                        {panel === "arena" && o.disabled ? " · Needs work" : ""}
                      </strong>
                      <small>{selecting ? "Selecting…" : o.group}</small>
                      {selected && (
                        <span className="tile-selected">Selected</span>
                      )}
                    </button>
                  );
                  if (panel !== "arena") return tile;
                  const starred = favorites?.includes(o.id) ?? false;
                  const starLabel = `${starred ? "Remove" : "Add"} ${o.name} ${starred ? "from" : "to"} favorites`;
                  return (
                    <div className="arena-option" key={o.id}>
                      {tile}
                      <button
                        type="button"
                        className="arena-star"
                        data-favorite={o.id}
                        aria-pressed={starred}
                        aria-label={starLabel}
                        title={starLabel}
                        disabled={favorites === null || favoriteBusy === o.id}
                        onClick={() => toggleFavorite(o.id, o.name, !starred)}
                      >
                        {starred ? "★" : "☆"}
                      </button>
                    </div>
                  );
                })}
              </div>
            )}
            {panel === "outfit" && (
              <OverlayOutfitEditor
                live={live}
                control={clothing}
                outfit={outfit}
                setOutfit={setOutfit}
                message={message}
                setMessage={setMessage}
                restore={() => {
                  if (!live) setOutfit(initialOutfit);
                  void clothing.restore();
                }}
              />
            )}
            {panel === "rules" && live && (
              <NativeRules
                gameRoot={settings.gameRoot}
                running={!!nativeStatus?.enabled}
                compact
                autoRead
              />
            )}
            {panel === "rules" && !live && (
              <NativeRules gameRoot="" running compact preview />
            )}
          </div>
          {panel !== "rules" && panel !== "outfit" && (
            <footer>
              {live && catalogControl(panel) ? (
                <CatalogStatus
                  live={live}
                  control={catalogControl(panel)!}
                  feature={panel as "arena"}
                />
              ) : (
                <span role="status">
                  {live
                    ? panel === "outfit"
                      ? clothing.error ||
                        clothing.status?.message ||
                        "Read Blacktop players, review the draft, then Apply outfit at selection."
                      : "Local catalog preview · no game writes"
                    : message}
                </span>
              )}
              {["ball", "arena", "scoreboard"].includes(panel!) && (
                <div className="catalog-pagination">
                  <button
                    disabled={offset === 0}
                    onClick={() => setOffset(Math.max(0, offset - 24))}
                  >
                    Previous
                  </button>
                  <span>
                    {total
                      ? `${offset + 1}–${Math.min(offset + 24, total)} of ${total.toLocaleString()}`
                      : "No results"}
                  </span>
                  <button
                    disabled={offset + 24 >= total}
                    onClick={() => setOffset(offset + 24)}
                  >
                    Next
                  </button>
                </div>
              )}
            </footer>
          )}
        </section>
      )}
      {!panel && !live && (
        <div className="preview-backdrop">
          <Icon name="court" size={72} />
          <h3>Blacktop · offline overlay preview</h3>
          <p>
            No game required. Open a catalog above to test spacing and
            navigation.
          </p>
          <small>Dummy player — not connected to NBA 2K27</small>
        </div>
      )}
    </div>
  );
}
function Roster({ setStatus }: { setStatus: (s: string) => void }) {
  const [query, setQuery] = useState("Stephen Curry"),
    [players, setPlayers] = useState<Player[]>([]),
    [total, setTotal] = useState(0),
    [detail, setDetail] = useState<PlayerDetail | null>(null),
    [selected, setSelected] = useState(new Set<number>()),
    [modal, setModal] = useState(false),
    [draft, setDraft] = useState<any>(null),
    [error, setError] = useState(""),
    [loading, setLoading] = useState(false);
  const generation = useRef(0);
  useEffect(() => {
    const id = ++generation.current;
    const timer = setTimeout(() => {
      setLoading(true);
      call<{ items: Player[]; total: number }>("players", { query })
        .then((r) => {
          if (id === generation.current) {
            setPlayers(r.items);
            setTotal(r.total);
          }
        })
        .catch((e) => setError(String(e)))
        .finally(() => {
          if (id === generation.current) setLoading(false);
        });
    }, 120);
    return () => {
      clearTimeout(timer);
      generation.current++;
    };
  }, [query]);
  useEffect(() => {
    call<any>("draft")
      .then(setDraft)
      .catch((e) => setError(String(e)));
  }, []);
  async function choose(p: Player) {
    setLoading(true);
    try {
      setDetail(await call<PlayerDetail>("player", { id: p.id }));
      setSelected(new Set());
      setModal(true);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }
  async function stage() {
    if (!detail || !selected.size) return;
    const seasons = stagedSeasons(selected, detail.records);
    const next = {
      name: `${detail.player.firstName} ${detail.player.lastName}`,
      sourceId: detail.player.id,
      info: detail.player.data.info,
      seasons,
      gameWrite: false,
    };
    try {
      await call("save-draft", next);
      setDraft(next);
      setModal(false);
      setStatus(
        `Saved local draft with ${seasons.length} seasons. No game writes.`,
      );
    } catch (e) {
      setError(String(e));
    }
  }
  return (
    <>
      <ErrorBox text={error} />
      <EditorEntry section="players" onError={setError} />
      <LocalEditorTools>
        <PlayerDNA status={setStatus} />
        <Card
          title="Search players"
          action={
            <span className="badge">{total.toLocaleString()} results</span>
          }
        >
          <input
            className="wide-input"
            placeholder="Player name"
            aria-label="Search players"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
          <div className="players-list">
            {loading && <p className="muted">Searching…</p>}
            {players.map((p) => (
              <button key={p.id} onClick={() => choose(p)}>
                <Icon name="person" />
                <div>
                  <strong>{p.name}</strong>
                  <small>
                    Source #{p.id} · {p.seasons} seasons
                  </small>
                </div>
                <span>Review seasons →</span>
              </button>
            ))}
          </div>
        </Card>
        {draft && (
          <Card title={`Local stats draft · ${draft.name}`}>
            <SeasonTable records={draft.seasons} />
            <p className="muted">
              Stored in Data/roster-draft.json. Not applied to the game.
            </p>
          </Card>
        )}
      </LocalEditorTools>
      {modal && detail && (
        <div className="modal-backdrop">
          <section className="modal">
            <header>
              <div>
                <h2>
                  {detail.player.firstName} {detail.player.lastName}
                </h2>
                <small>NBA 2K23 season history · local draft</small>
              </div>
              <button onClick={() => setModal(false)}>Back to players</button>
            </header>
            <div className="modal-body">
              <div className="info-grid">
                {["birthDate", "yearsPro", "weightLbs", "potential"].map(
                  (k) => (
                    <div key={k}>
                      <small>{label(k)}</small>
                      <strong>{detail.player.data.info[k] ?? "—"}</strong>
                    </div>
                  ),
                )}
              </div>
              <div className="toolbar">
                <span>{selected.size} seasons selected</span>
                <button
                  onClick={() =>
                    setSelected(new Set(detail.records.map((r) => r.id)))
                  }
                >
                  Select all
                </button>
                <button onClick={() => setSelected(new Set())}>Clear</button>
              </div>
              <table>
                <thead>
                  <tr>
                    <th>Use</th>
                    <th>Team</th>
                    <th>Season</th>
                    <th>GP</th>
                    <th>MPG</th>
                    <th>PPG</th>
                    <th>RPG</th>
                    <th>APG</th>
                    <th>PTS</th>
                  </tr>
                </thead>
                <tbody>
                  {[...detail.records]
                    .sort((a, b) => b.year - a.year)
                    .map((r) => (
                      <tr key={r.id}>
                        <td>
                          <input
                            type="checkbox"
                            aria-label={`Import ${r.year}`}
                            checked={selected.has(r.id)}
                            onChange={() => {
                              const next = new Set(selected);
                              next.has(r.id)
                                ? next.delete(r.id)
                                : next.add(r.id);
                              setSelected(next);
                            }}
                          />
                        </td>
                        <td>
                          {detail.teams.find((t) => t.id === r.primaryTeamId)
                            ?.name ?? "Unknown source team"}
                        </td>
                        <td>
                          {r.year - 1}–{String(r.year).slice(-2)}
                        </td>
                        {[
                          "gp",
                          "minutes",
                          "points",
                          "rebounds",
                          "assists",
                          "points",
                        ].map((k, i) => (
                          <td key={i}>
                            {i === 0 || i === 5
                              ? (r.totals[k] ?? 0)
                              : (
                                  (k === "rebounds"
                                    ? r.totals.oreb + r.totals.dreb
                                    : r.totals[k]) / Math.max(1, r.totals.gp)
                                ).toFixed(1)}
                          </td>
                        ))}
                      </tr>
                    ))}
                </tbody>
              </table>
            </div>
            <footer>
              <p>
                {selected.size
                  ? "Import saves a draft, then returns to the stats view."
                  : "Choose at least one season."}
              </p>
              <button
                className="primary"
                disabled={!selected.size}
                onClick={stage}
              >
                Import stats to local draft
              </button>
            </footer>
          </section>
        </div>
      )}
    </>
  );
}
function SeasonTable({ records }: { records: any[] }) {
  return (
    <table>
      <thead>
        <tr>
          <th>Season</th>
          <th>GP</th>
          <th>PTS</th>
          <th>REB</th>
          <th>AST</th>
          <th>STL</th>
          <th>BLK</th>
        </tr>
      </thead>
      <tbody>
        {[...records]
          .sort((a, b) => b.year - a.year)
          .map((r) => (
            <tr key={r.id}>
              <td>
                {r.year - 1}–{String(r.year).slice(-2)}
              </td>
              <td>{r.totals.gp}</td>
              <td>{r.totals.points}</td>
              <td>{r.totals.oreb + r.totals.dreb}</td>
              <td>{r.totals.assists}</td>
              <td>{r.totals.steals}</td>
              <td>{r.totals.blocks}</td>
            </tr>
          ))}
      </tbody>
    </table>
  );
}
export function App() {
  const [overlayPanel, setOverlayPanel] = useState<string | null>(null);
  const [library, setLibrary] = useState<Library | null>(null);
  const libraryRef = useRef<Library | null>(null),
    libraryQueue = useRef(Promise.resolve());
  async function saveLibrary(patch: Partial<Library>) {
    if (!libraryRef.current) throw new Error("Library not ready");
    const next = { ...libraryRef.current, ...patch };
    libraryRef.current = next;
    setLibrary(next);
    const pending = libraryQueue.current.then(() => call("save-library", next));
    libraryQueue.current = pending.then(
      () => {},
      () => {},
    );
    try {
      await pending;
    } catch (e) {
      const persisted = await call<Library>("library");
      libraryRef.current = persisted;
      setLibrary(persisted);
      throw e;
    }
  }
  const [boot, setBoot] = useState<Bootstrap | null>(null),
    [page, setPage] = useState("Play session"),
    [settings, setSettings] = useState<Settings | null>(null),
    [status, setStatus] = useState("Starting Rust backend…"),
    [error, setError] = useState(""),
    [mods, setMods] = useState<Mod[]>([]),
    [modsScanned, setModsScanned] = useState(false),
    [modsError, setModsError] = useState(""),
    [modsScanning, setModsScanning] = useState(false),
    [media, setMedia] = useState<Media[]>([]),
    [query, setQuery] = useState(""),
    [overlay, setOverlay] = useState(false),
    [outfit, setOutfit] = useState(initialOutfit),
    [busy, setBusy] = useState(false),
    [playing, setPlaying] = useState<Media | null>(null),
    [teams, setTeams] = useState<
      { id: number; name: string; city: string; category: string }[]
    >([]);
  const standalone = new URLSearchParams(location.search).has("overlay");
  const settingsRef = useRef<Settings | null>(null),
    saveQueue = useRef(Promise.resolve()),
    modsScanRevision = useRef(0),
    modsRootSeen = useRef<string | null>(null),
    smokeStarted = useRef(false),
    probeStarted = useRef(false);
  useEffect(() => {
    Promise.all([
      call<Bootstrap>("bootstrap"),
      call<Media[]>("media"),
      call<Outfit | null>("outfit"),
      call<Library>("library"),
    ])
      .then(([b, m, o, l]) => {
        setBoot(b);
        setSettings(b.settings);
        settingsRef.current = b.settings;
        setMedia(m);
        setLibrary(l);
        libraryRef.current = l;
        if (o) setOutfit(o);
        setStatus("Ready");
      })
      .catch((e) => setError(String(e)));
  }, []);
  useEffect(() => {
    let disposed = false;
    const unlisten: (() => void)[] = [];
    const register = (p: Promise<() => void>) =>
      p
        .then((f) => {
          if (disposed) f();
          else unlisten.push(f);
        })
        .catch((e) => setError(String(e)));
    register(
      listen<Settings>("settings-changed", (e) => {
        settingsRef.current = e.payload;
        setSettings(e.payload);
      }),
    );
    register(listen<Outfit>("outfit-changed", (e) => setOutfit(e.payload)));
    return () => {
      disposed = true;
      unlisten.forEach((f) => f());
    };
  }, []);
  const [helpOpen, setHelpOpen] = useState(false);
  useEffect(() => {
    document.documentElement.dataset.theme = settings?.theme ?? "dark";
  }, [settings?.theme]);
  useEffect(() => {
    const view = document.querySelector(".page");
    if (view) view.scrollTop = 0;
  }, [page]);
  useEffect(() => {
    if (!window.__overlayProbe || !boot || probeStarted.current) return;
    probeStarted.current = true;
    // Keep this hidden fixture alive for main-window geometry/reuse checks.
    // finish_smoke closes the owning application and all its fixture windows.
    call("save-settings", { ...boot.settings, ball: "Allstar Flaming" }).catch(
      (e) => setError(String(e)),
    );
  }, [boot]);
  useEffect(() => {
    if (!window.__nativeOverlayProbe || !boot || probeStarted.current) return;
    probeStarted.current = true;
    void nativeOverlayProbe().catch((e) => setError(String(e)));
  }, [boot]);
  useEffect(() => {
    if (window.__launcherSmoke) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen<GameCaptureEvent>("game-capture-status", (event) =>
      setStatus(gameCaptureMessage(event.payload)),
    ).then((stop) => {
      if (disposed) stop();
      else unlisten = stop;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);
  function change(p: Partial<Settings>) {
    const current = settingsRef.current;
    if (!current) return;
    const next = { ...current, ...p };
    settingsRef.current = next;
    setSettings(next);
    saveQueue.current = saveQueue.current
      .then(async () => {
        const saved = await call<Settings>("save-settings", p);
        settingsRef.current = saved;
        setSettings(saved);
        setStatus("Saved");
      })
      .catch(async (e) => {
        try {
          const persisted = (await call<Bootstrap>("bootstrap")).settings;
          settingsRef.current = persisted;
          setSettings(persisted);
        } catch {
          // Keep the original save failure visible if recovery also fails.
        }
        setError(String(e));
        setStatus("Settings not saved");
      });
  }
  async function task(fn: () => Promise<void>) {
    if (busy) return;
    setBusy(true);
    setError("");
    try {
      await fn();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }
  // WPF scanModCatalog: the newest scan for the current profile wins; a
  // profile change clears the list and rescans the new folder.
  async function scan(manual = false) {
    const revision = ++modsScanRevision.current;
    setModsScanning(true);
    try {
      // The backend scans its saved modsRoot; wait for pending setting saves.
      await saveQueue.current;
      const root = settingsRef.current?.modsRoot ?? "";
      const result = await call<{
        items: Mod[];
        missing: boolean;
        truncated: boolean;
      }>("mods");
      if (
        revision !== modsScanRevision.current ||
        root !== (settingsRef.current?.modsRoot ?? "")
      )
        return;
      setMods(result.missing ? [] : result.items);
      setModsScanned(true);
      setModsError(
        result.missing ? "The selected mods folder does not exist." : "",
      );
      setStatus(
        result.missing
          ? "Mods folder not found · choose a profile folder"
          : manual
            ? "Active mod library refreshed."
            : `${result.items.length} files · read-only scan`,
      );
    } catch (e) {
      if (revision !== modsScanRevision.current) return;
      setModsScanned(true);
      setModsError(String(e).replace(/^Error: /, ""));
      if (manual) throw e;
    } finally {
      if (revision === modsScanRevision.current) setModsScanning(false);
    }
  }
  useEffect(() => {
    const root = settings?.modsRoot;
    if (root === undefined) return;
    const previous = modsRootSeen.current;
    modsRootSeen.current = root;
    if (previous === null || previous === root) return;
    const wasScanned = modsScanned;
    setMods([]);
    setModsError("");
    setModsScanned(false);
    if (wasScanned) void scan();
  }, [settings?.modsRoot]);
  async function folder(key: "gameRoot" | "modsRoot") {
    const path = await call<string | null>("pick-folder");
    if (path) {
      change({ [key]: path });
      if (key === "gameRoot") {
        const b = await call<Bootstrap>("bootstrap");
        setBoot({ ...b, gameFound: false });
        setStatus("Game path saved; click Refresh status to check executable.");
      }
    }
  }
  useEffect(() => {
    if (page === "Mod library" && !modsScanned) void task(scan);
    if (page === "Team Editor")
      call<PlayerDetail>("player", { id: 0 })
        .then((d) => setTeams(d.teams.filter((t) => t.category === "current")))
        .catch((e) => setError(String(e)));
  }, [page]);
  // Exercises actual native IPC in an isolated Smoke profile; no game session or installed files are touched.
  useEffect(() => {
    if (!window.__launcherSmoke || !boot || smokeStarted.current) return;
    smokeStarted.current = true;
    (async () => {
      const checks: string[] = [];
      let result: Record<string, unknown>;
      try {
        await new Promise((r) => setTimeout(r, 200));
        await call("capture", { name: "play-session" });
        const players = await call<{ items: Player[]; total: number }>(
          "players",
          { query: "Stephen Curry" },
        );
        if (!players.items.length)
          throw new Error("Bundled player search empty");
        checks.push("rust-player-search");
        const d = await call<PlayerDetail>("player", {
          id: players.items[0].id,
        });
        if (!d.records.length) throw new Error("Season records empty");
        checks.push("rust-season-history");
        await call("save-settings", {
          ...boot.settings,
          profile: "Smoke test",
        });
        const reread = await call<Bootstrap>("bootstrap");
        if (reread.settings.profile !== "Smoke test")
          throw new Error("Settings did not persist");
        checks.push("rust-settings-round-trip");
        await call("save-outfit", initialOutfit);
        const o = await call<Outfit>("outfit");
        if (o.shoes !== initialOutfit.shoes) throw new Error("Outfit changed");
        checks.push("rust-outfit-round-trip");
        const garments = await call<{ items: Garment[] }>("wardrobe", {
          slot: "top",
          query: "",
        });
        if (!garments.items.length) throw new Error("Wardrobe empty");
        checks.push("rust-wardrobe-search");
        await call("save-draft", {
          name: players.items[0].name,
          seasons: d.records.slice(0, 1),
          gameWrite: false,
        });
        checks.push("local-season-draft");
        setOverlay(true);
        await new Promise((r) => setTimeout(r, 150));
        const buttons = [
          ...document.querySelectorAll<HTMLButtonElement>(
            ".overlay-actions button",
          ),
        ];
        buttons[0].click();
        await new Promise((r) => setTimeout(r, 180));
        if (!document.querySelector(".overlay-catalog"))
          throw new Error("Outfit catalog did not open");
        const before = document
          .querySelector(".overlay-bar")!
          .getBoundingClientRect();
        buttons[0].click();
        await new Promise((r) => setTimeout(r, 60));
        if (document.querySelector(".overlay-catalog"))
          throw new Error("Repeated button did not close catalog");
        const after = document
          .querySelector(".overlay-bar")!
          .getBoundingClientRect();
        if (before.width !== after.width || before.height !== after.height)
          throw new Error("Overlay bar resized");
        checks.push("same-button-close", "fixed-overlay-dimensions");
        // Address the Ball section directly: the bar shows WPF's wording
        // ("Current installed"), not always the saved setting name.
        const ballButton = document.querySelector<HTMLButtonElement>(
          ".overlay-item.ball button",
        );
        if (
          !ballButton ||
          !ballButton.textContent?.includes(wpfChoiceLabel(boot.settings.ball))
        )
          throw new Error(
            `Overlay ball button missing or not showing ${boot.settings.ball}`,
          );
        ballButton.click();
        await new Promise((r) => setTimeout(r, 180));
        await call("capture", { name: "ball-catalog" });
        ballButton.click();
        await new Promise((r) => setTimeout(r, 80));
        buttons[0].click();
        await new Promise((r) => setTimeout(r, 250));
        // The WPF-layout editor: player, Apply/Restore and status together.
        const outfitControls = document.querySelector(".wpf-outfit");
        if (
          !outfitControls?.textContent?.includes("Stephen Curry") ||
          !outfitControls.textContent.includes("Apply outfit") ||
          !outfitControls.textContent.includes("Restore original") ||
          !outfitControls.textContent.includes("Offline example")
        )
          throw new Error(
            "Shared outfit preview is missing its player or native controls",
          );
        checks.push("shared-outfit-preview-player-apply-restore-controls");
        await call("capture", { name: "outfit-catalog" });
        buttons[0].click();
        await new Promise((r) => setTimeout(r, 80));
        const rulesButton = document.querySelector<HTMLButtonElement>(
          ".overlay-item.rules button",
        )!;
        rulesButton.click();
        await new Promise((r) => setTimeout(r, 80));
        await call("capture", { name: "rules-catalog" });
        setOverlay(false);
        setPage("Roster Editor");
        await new Promise((r) => setTimeout(r, 400));
        const localTools = document.querySelector<HTMLDetailsElement>(
          "[data-local-editor-tools]",
        );
        if (
          !localTools ||
          localTools.open ||
          document.querySelectorAll("[data-editor-open]").length !== 1
        )
          throw new Error(
            "Roster editor did not lead with the browser workflow",
          );
        await call("capture", { name: "roster-browser-entry" });
        localTools.querySelector("summary")!.click();
        await new Promise((r) => setTimeout(r, 120));
        checks.push("browser-first-roster-local-tools-separated");
        await call("capture", { name: "roster-catalog" });
        checks.push("native-webview2-screenshots");
        document
          .querySelector<HTMLButtonElement>(".players-list button")!
          .click();
        await new Promise((r) => setTimeout(r, 250));
        if (!document.querySelector(".modal"))
          throw new Error("Season dialog did not open");
        await call("capture", { name: "season-dialog" });
        document.querySelector<HTMLInputElement>(".modal tbody input")!.click();
        await new Promise((r) => setTimeout(r, 80));
        document
          .querySelector<HTMLButtonElement>(".modal footer .primary")!
          .click();
        await new Promise((r) => setTimeout(r, 200));
        if (document.querySelector(".modal"))
          throw new Error("Season import did not return to stats");
        const saved = await call<any>("draft");
        if (saved.seasons.length !== 1 || saved.gameWrite !== false)
          throw new Error("Season import saved wrong draft");
        checks.push("season-checkbox-import-return");
        const support = await call<{
          path: string;
          files: number;
          redacted: boolean;
        }>("support-bundle-create");
        if (
          !support.redacted ||
          !Number.isSafeInteger(support.files) ||
          support.files < 0 ||
          !support.path.endsWith(".zip")
        )
          throw new Error("Redacted support bundle was not created");
        checks.push("redacted-support-bundle-created");
        setPage("Play session");
        await new Promise((r) => setTimeout(r, 80));
        checks.push(
          ...(await paritySmoke({
            page: setPage,
            change,
            save: saveLibrary,
            media: setMedia,
            outfit: setOutfit,
          })),
        );
        setPage("Blacktop");
        await new Promise((r) => setTimeout(r, 200));
        const previewButton = document.querySelector<HTMLButtonElement>(
          "[data-overlay-preview]",
        );
        if (!previewButton)
          throw new Error("Primary overlay preview is missing");
        previewButton.click();
        let previewState: any;
        for (let i = 0; i < 40; i++) {
          previewState = await call("smoke-offline-preview-state");
          if (previewState.exists) break;
          await new Promise((r) => setTimeout(r, 50));
        }
        if (
          !previewState?.exists ||
          !previewState.hidden ||
          previewState.gameAccess !== false ||
          Math.abs(previewState.position.y - previewState.expected.y) > 2 ||
          Math.abs(previewState.position.x - previewState.expected.x) > 2 ||
          Math.abs(previewState.size.width - previewState.expected.width) > 2
        )
          throw new Error(
            "Primary preview did not open as the isolated top-aligned window",
          );
        previewButton.click();
        await new Promise((r) => setTimeout(r, 120));
        const reused: any = await call("smoke-offline-preview-state");
        if (
          !reused.exists ||
          !reused.hidden ||
          JSON.stringify(reused.position) !==
            JSON.stringify(previewState.position)
        )
          throw new Error(
            "Repeated preview changed visibility or position in smoke",
          );
        checks.push("primary-preview-top-aligned-and-repeat-stable");
        setOverlay(true);
        for (
          let i = 0;
          i < 40 &&
          !document
            .querySelector(".overlay-bar")
            ?.textContent?.includes("Allstar Flaming");
          i++
        )
          await new Promise((r) => setTimeout(r, 50));
        if (
          !document
            .querySelector(".overlay-bar")
            ?.textContent?.includes("Allstar Flaming")
        )
          throw new Error(
            "Separate overlay did not sync native settings to main",
          );
        checks.push("separate-overlay-native-settings-sync");
        result = {
          pass: true,
          checks,
          startupMs: boot.startupMs,
          ui: `React ${React.version} / TypeScript`,
          runtime: boot.runtime,
          gameWrites: false,
        };
      } catch (e) {
        result = {
          pass: false,
          checks,
          error: String(e),
          status: document.querySelector('[role="status"]')?.textContent,
          alerts: [...document.querySelectorAll('[role="alert"]')].map(
            (e) => e.textContent,
          ),
        };
        await call("capture", { name: "failure" }).catch(() => {});
      }
      await invoke("finish_smoke", { result });
    })().catch((e) => setError(String(e)));
  }, [boot]);
  if (error && !settings)
    return (
      <div className="fatal">
        <h1>2K Launcher could not start</h1>
        <ErrorBox text={error} />
        <p>Your working launcher is unaffected.</p>
      </div>
    );
  if (!boot || !settings || !library)
    return (
      <div className="loading">
        <div className="spinner" />
        <h2>2k Launcher</h2>
        <p>Starting…</p>
      </div>
    );
  if (standalone)
    return (
      <Overlay
        settings={settings}
        change={change}
        outfit={outfit}
        setOutfit={setOutfit}
        onClose={() =>
          call(
            new URLSearchParams(location.search).has("live")
              ? "native-overlay-hide"
              : "overlay-close",
          )
        }
        standalone
        startPanel={new URLSearchParams(location.search).get("panel")}
        live={new URLSearchParams(location.search).has("live")}
      />
    );
  const mediaKind = page === "Jukebox" ? "Music" : "Videos";
  const selectedMedia = media.filter((m) => m.kind === mediaKind);
  return (
    <div className="app-shell">
      <WpfSidebar
        pages={pages}
        page={page}
        onPage={(p) => {
          setPage(p);
          setQuery("");
          setPlaying(null);
          setError("");
        }}
        providerReady={boot.gameFound}
        providerLabel={boot.gameFound ? "Version 2" : "Not configured"}
        theme={settings.theme}
        onTheme={(theme) => {
          if (theme !== settings.theme) change({ theme });
        }}
        onHelp={() => setHelpOpen(true)}
      />
      <main>
        <WpfHeader page={page} version={boot.version} />
        <section
          className="page page-width-fluid active"
          id={`page-${wpfPageIds[page]}`}
        >
          <ErrorBox text={error} />
          {page === "Play session" && (
            <LaunchReadiness
              gameRoot={settings.gameRoot}
              modsRoot={settings.modsRoot}
              gameFound={boot.gameFound}
            >
              <div
                className="card home-session-picks"
                aria-label="Session setup"
              >
                <label>
                  <span className="home-field-label" data-ui-icon="mods">
                    <UiIcon name="mods" />
                    Mod profile
                  </span>
                  <select
                    id="homeModProfile"
                    aria-label="Mod profile"
                    value={settings.profile}
                    onChange={(e) => {
                      const p = library.profiles.find(
                        (p) => p.name === e.target.value,
                      );
                      if (p) change({ profile: p.name, modsRoot: p.folder });
                    }}
                  >
                    {!library.profiles.some(
                      (p) => p.name === settings.profile,
                    ) && <option>{settings.profile}</option>}
                    {library.profiles.map((p) => (
                      <option key={p.name}>{p.name}</option>
                    ))}
                  </select>
                </label>
                <PlayJukeboxChoice
                  rememberedId={settings.music}
                  onRemember={(music) => change({ music })}
                />
                <div className="home-startup-video">
                  <label className="home-startup-toggle">
                    <span
                      className="home-field-label"
                      data-ui-icon="presentation"
                    >
                      <UiIcon name="presentation" />
                      Startup video
                    </span>
                    <input
                      id="startupVideoEnabled"
                      aria-label="Enable startup video"
                      type="checkbox"
                      checked={settings.startupVideo}
                      onChange={(e) =>
                        change({ startupVideo: e.target.checked })
                      }
                    />
                  </label>
                  <select
                    id="presentationVideoSelect"
                    aria-label="Startup video"
                    value={settings.video}
                    onChange={(e) => change({ video: e.target.value })}
                  >
                    <option value="">No movie selected</option>
                    {media
                      .filter((m) => m.kind === "Videos")
                      .map((m) => (
                        <option key={m.id ?? m.path} value={m.id ?? m.path}>
                          {m.name}
                        </option>
                      ))}
                  </select>
                </div>
              </div>
            </LaunchReadiness>
          )}
          {page === "Profiles" && (
            <Profiles
              library={library}
              save={saveLibrary}
              settings={settings}
              change={change}
              status={setStatus}
            />
          )}
          {page === "Mod library" && (
            <Mods
              library={library}
              save={saveLibrary}
              settings={settings}
              change={change}
              status={setStatus}
              mods={mods}
              busy={busy || modsScanning}
              scan={() => scan(true)}
              scanned={modsScanned}
              scanError={modsError}
            />
          )}
          {page === "Jukebox" && (
            <Jukebox
              library={library}
              save={saveLibrary}
              settings={settings}
              change={change}
              status={setStatus}
              media={media}
              setMedia={setMedia}
            />
          )}
          {page === "Presentation" && (
            <Presentation
              library={library}
              save={saveLibrary}
              settings={settings}
              change={change}
              status={setStatus}
              media={media}
              setMedia={setMedia}
            />
          )}
          {page === "Roster Editor" && <Roster setStatus={setStatus} />}
          {page === "Team Editor" && (
            <>
              <EditorEntry section="teams" onError={setError} />
              <LocalEditorTools>
                <TeamDNA status={setStatus} />
                <Card title="NBA 2K23 · current teams">
                  <input
                    className="wide-input"
                    placeholder="Search teams"
                    value={query}
                    onChange={(e) => setQuery(e.target.value)}
                  />
                  <div className="team-grid">
                    {teams
                      .filter((t) =>
                        `${t.city} ${t.name}`
                          .toLowerCase()
                          .includes(query.toLowerCase()),
                      )
                      .map((t) => (
                        <div className="team-tile" key={t.id}>
                          <Icon name="team" size={26} />
                          <strong>{t.name}</strong>
                          <small>
                            {t.city} · source #{t.id}
                          </small>
                        </div>
                      ))}
                  </div>
                </Card>
              </LocalEditorTools>
            </>
          )}
          {page === "Blacktop" && (
            <Blacktop
              library={library}
              save={saveLibrary}
              settings={settings}
              change={change}
              status={setStatus}
              outfit={outfit}
              setOutfit={setOutfit}
              preview={(panel) => {
                setOverlayPanel(panel ?? null);
                setOverlay(true);
              }}
              topPreview={() =>
                void task(async () => {
                  await call("overlay-open");
                })
              }
            />
          )}
          {page === "Unlocks" && (
            <UnlocksPanel
              blacktop={settings.unlockWnba}
                blacktopAvailable={blacktopUnlockAvailable(boot)}
              onBlacktopChange={(unlockWnba) => change({ unlockWnba })}
              freestyle={settings.unlockWnbaFreestyle === true}
              onFreestyleChange={(unlockWnbaFreestyle) =>
                change({ unlockWnbaFreestyle })
              }
              freestyleMessage={freestyleUnlockMessage(
                settings.unlockWnbaFreestyle === true,
              )}
            />
          )}
          {page === "Scaler" && (
            <ScalerSettings
              value={settings.scaler}
              onSave={(scaler) => change({ scaler })}
              groups={{
                coaches: settings.scalerCoaches,
                cheerleaders: settings.scalerCheerleaders,
                arenaPersonnel: settings.scalerArenaPersonnel,
              }}
              onGroupsChange={(groups) =>
                change({
                  scalerCoaches: groups.coaches,
                  scalerCheerleaders: groups.cheerleaders,
                  scalerArenaPersonnel: groups.arenaPersonnel,
                })
              }
            />
          )}
          {page === "Settings" && (
            <div className="settings-page-layout settings-workspace">
              <LauncherUpdatesCard setStatus={setStatus} />
              <LauncherHealthCard setStatus={setStatus} />
              <article className="card settings-path-card">
                <div className="settings-card-heading">
                  <strong>Game installation</strong>
                  <span
                    className="settings-badge"
                    id="validationLabel"
                    style={{
                      color: boot.gameFound ? "var(--green)" : "var(--amber)",
                    }}
                  >
                    {boot.gameFound ? "GAME READY" : "CHECK PATHS"}
                  </span>
                </div>
                <p className="settings-description">
                  The launcher uses this folder for the game executable, mods
                  and verified runtime integration.
                </p>
                <div className="settings-path-controls">
                  <label htmlFor="gameRoot">NBA 2K27 folder</label>
                  <input id="gameRoot" value={settings.gameRoot} readOnly />
                  <button
                    className="secondary"
                    data-choose="gameRoot"
                    onClick={() => void task(() => folder("gameRoot"))}
                  >
                    Browse
                  </button>
                  <button
                    className="primary"
                    id="saveSettings"
                    onClick={() =>
                      void task(async () => {
                        setBoot(await call("bootstrap"));
                        setStatus("Game folder status refreshed");
                      })
                    }
                  >
                    Refresh status
                  </button>
                </div>
                <div className="settings-path-controls">
                  <label htmlFor="modsRoot">Mods folder</label>
                  <input id="modsRoot" value={settings.modsRoot} readOnly />
                  <button
                    className="secondary"
                    data-choose="modsRoot"
                    onClick={() => void task(() => folder("modsRoot"))}
                  >
                    Browse
                  </button>
                  <button
                    className="secondary"
                    onClick={() =>
                      void task(async () => {
                        const { games } = await call<{ games: string[] }>(
                          "steam-find-game",
                        );
                        if (games.length !== 1) {
                          setStatus(
                            games.length
                              ? `NBA 2K27 found in ${games.length} Steam libraries · choose one with Browse`
                              : "NBA 2K27 was not found in any Steam library · choose the folder with Browse",
                          );
                          return;
                        }
                        change({ gameRoot: games[0] });
                        const b = await call<Bootstrap>("bootstrap");
                        setBoot({ ...b, gameFound: false });
                        setStatus(
                          `Found NBA 2K27 in Steam libraries · ${games[0]} · click Refresh status to check executable.`,
                        );
                      })
                    }
                  >
                    Find in Steam libraries
                  </button>
                </div>
                <div className="settings-support-actions">
                  <button
                    className="secondary"
                    id="openLauncherStorage"
                    type="button"
                    onClick={() =>
                      void task(() => call("open-launcher-storage"))
                    }
                  >
                    Open launcher folder
                  </button>
                  <small id="launcherStoragePath">{boot.root}</small>
                </div>
              </article>
              <div className="settings-preferences-grid">
                <WpfImportCard setStatus={setStatus} />
                <article className="card settings-option-card settings-support-card">
                  <div className="settings-card-heading">
                    <strong>Support</strong>
                    <span className="settings-badge">Redacted</span>
                  </div>
                  <p className="settings-description">
                    Save a small ZIP of launcher logs and settings for support.
                    Personal paths and recognized credential fields are
                    redacted. Media, previews, drafts, caches and crash dumps
                    are excluded. Review the ZIP before sharing it.
                  </p>
                  <div className="settings-support-actions">
                    <button
                      className="secondary"
                      id="exportSupportBundle"
                      type="button"
                      onClick={() =>
                        void task(async () => {
                          const result = await call<{
                            path: string;
                            files: number;
                          }>("support-bundle-create");
                          setStatus(
                            `Redacted support bundle saved · ${result.files} diagnostic files · ${result.path}`,
                          );
                        })
                      }
                    >
                      Export support bundle
                    </button>
                    <button
                      className="secondary"
                      type="button"
                      onClick={() =>
                        void task(() => call("open-support-folder"))
                      }
                    >
                      Open support folder
                    </button>
                    <small id="supportBundleStatus">
                      Nothing is sent automatically.
                    </small>
                  </div>
                  <div className="settings-support-actions">
                    <button
                      className="secondary"
                      id="captureGameHang"
                      type="button"
                      onClick={() =>
                        void task(async () => {
                          setStatus(
                            "Capturing game snapshot · the game may pause briefly",
                          );
                          const result =
                            await call<GameCaptureResult>("game-hang-capture");
                          setStatus(
                            gameCaptureMessage({ phase: "captured", result }),
                          );
                        })
                      }
                    >
                      Capture game hang
                    </button>
                    <button
                      className="secondary"
                      id="openGameDiagnostics"
                      type="button"
                      onClick={() =>
                        void task(() => call("open-game-captures"))
                      }
                    >
                      Open diagnostics
                    </button>
                  </div>
                  <small id="gameCaptureStatus">
                    Ctrl+Shift+F12 captures a stuck game while it is still
                    running.
                  </small>
                  <HotkeyStatus />
                  <small>
                    Snapshots can briefly pause the game and may contain private
                    memory. Stored locally in Logs/game-captures; never uploaded
                    or included in support exports. Keeps three sessions, up to
                    three snapshots each.
                  </small>
                </article>
              </div>
              <DeveloperTools setStatus={setStatus} />
              <details className="card settings-advanced">
                <summary>Advanced · Runtime and feature readiness</summary>
                <GameBuildCompatibility />
                <Card title="Portable storage">
                  <p className="path-line">{boot.root}</p>
                  <div className="storage-grid">
                    {[
                      ["App", "One native executable · embedded React UI"],
                      ["Data", "Settings, outfit and roster draft"],
                      ["Library", "Imported Music and Videos"],
                      ["Cache", "Portable WebView2 profile"],
                      ["Logs", "Prototype test reports"],
                    ].map(([a, b]) => (
                      <div key={a}>
                        <strong>{a}</strong>
                        <span>{b}</span>
                      </div>
                    ))}
                  </div>
                  <p className="settings-description">
                    No AppData settings, Node runtime, Python runtime or .NET
                    runtime. WebView2 itself is the shared Windows browser
                    runtime.
                  </p>
                </Card>
                <Card title="Runtime">
                  <div className="feature-grid">
                    <div>
                      <strong>{boot.runtime}</strong>
                      <p>Native bootstrap: {boot.startupMs.toFixed(0)} ms</p>
                    </div>
                    <div>
                      <strong>Explicit QA actions</strong>
                      <p>
                        Browsing and previews do not change the game. Live
                        launch and supported edits require their separate
                        confirmations.
                      </p>
                    </div>
                  </div>
                </Card>
                <Card title="Feature readiness">
                  <p className="settings-description">
                    Baseline: main WPF24 · {parity.baselineRoutes} feature
                    routes inventoried. This build is not feature-equivalent
                    yet.
                  </p>
                  <div className="parity-list">
                    {parity.capabilities.map((g) => (
                      <details key={g.feature}>
                        <summary>
                          <strong>{g.feature}</strong>
                          <span className="settings-badge">{g.status}</span>
                        </summary>
                        <p>{g.remaining}</p>
                        <p className="settings-description">
                          Evidence: {g.evidence}
                        </p>
                      </details>
                    ))}
                  </div>
                </Card>
              </details>
            </div>
          )}
        </section>
        <WpfToast text={status} />
      </main>
      <WpfHelpDialog
        open={helpOpen}
        onClose={() => setHelpOpen(false)}
        onDiagnostics={() => {
          setHelpOpen(false);
          setPage("Settings");
        }}
      />
      {overlay && (
        <div className="overlay-backdrop">
          <Overlay
            startPanel={overlayPanel}
            settings={settings}
            change={change}
            outfit={outfit}
            setOutfit={setOutfit}
            onClose={() => {
              setOverlay(false);
              setOverlayPanel(null);
            }}
          />
        </div>
      )}
    </div>
  );
}
