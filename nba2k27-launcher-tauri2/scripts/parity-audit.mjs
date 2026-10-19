// An explicit, versioned acceptance inventory. UI controls alone do not count as parity.
import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";
import { fileURLToPath } from "node:url";
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const original = path.join(root, "../nba2k27-mod-launcher/src");
const main = fs.readFileSync(path.join(original, "main.js"), "utf8");
const release = JSON.parse(
  fs.readFileSync(
    path.join(root, "../nba2k27-launcher-wpf/release-info.json"),
    "utf8",
  ),
);
const partial = new Map([
  [
    "game:launch-without-eac",
    "launch-preflight / native-launch-steam-only / native-launch-modded (owned native provider and independent Version 2 arena/gameplay-ball/scoreboard/rules/presentation/Jukebox executors; feature-specific live acceptance pending)",
  ],
  ["app:bootstrap", "bootstrap"],
  [
    "roster:bootstrap",
    "open-browser-editor (exact WPF browser pages; Rust-owned read-only roster snapshot backend; live acceptance pending)",
  ],
  [
    "roster:auto-attach",
    "native read-only active roster snapshot (live acceptance pending)",
  ],
  [
    "player-research:scan",
    "native read-only player/team/arena/uniform snapshot (live acceptance pending)",
  ],
  ["player-research:status", "native cached read-only roster status"],
  [
    "player:save",
    "native explicit player Save & verify (basic details, 441 editable fields, birthday/years pro; per-tab snapshots, durable intent, guarded rollback; live acceptance pending)",
  ],
  [
    "team:save",
    "native explicit basic team Save & verify / separate arena-floor reserved-slot copy-on-write; destination Team ID unchanged; durable intent and rollback (live acceptance pending)",
  ],
  [
    "team-uniform:save",
    "native explicit 52-control uniform Save & verify; immutable team-slot baseline, shared-slot refresh, preserved IDs/unmapped bits, durable intent and conditional rollback (live acceptance pending)",
  ],
  [
    "editor:presets",
    "original browser preset controls / native signature, equipment and coaching store (no game writes)",
  ],
  ["settings:save", "save-settings"],
  ["settings:choose", "pick-folder"],
  ["mods:scan", "mods"],
  ["mods:save-tags", "save-library"],
  ["mods:open-item", "open-mod-item"],
  [
    "mods:install",
    "mod-install-plan / mod-install-commit (files, folder, ZIP; RAR via Windows tar.exe as in WPF; per-file Replace/Keep review)",
  ],
  ["shell:open-mods", "open-mods-folder"],
  [
    "camera:start",
    "native-camera-command retry (re-arms only a stop or a fully restored hold)",
  ],
  ["camera:stop", "native-camera-command stop (guarded camera restore)"],
  [
    "presentation:library-edit",
    "video-library-edit rename/remove (select via save-settings)",
  ],
  [
    "jukebox:repair-missing-tracks",
    "jukebox:find-missing / jukebox:repair-missing",
  ],
  ["jukebox:refresh-playlist-bundle", "jukebox:refresh-bundle"],
  [
    "arena:favorites",
    "arena-favorites (Data/arena-favorites.json, exact catalog ids)",
  ],
  ["arena:favorite", "arena-favorite / choices group Favorites"],
  ["blacktop:outfits-list", "library"],
  ["blacktop:outfits-save", "save-library"],
  ["blacktop:outfits-import", "import-outfit"],
  ["blacktop:outfits-export", "export-outfit"],
  [
    "blacktop:panel-open",
    "overlay-open / native-overlay-enable (owned native F8 QA; live acceptance pending)",
  ],
  [
    "blacktop:panel-hide",
    "overlay-close / native-overlay-hide / native-overlay-disable",
  ],
  [
    "blacktop:rules-read",
    "native-rules-read (guarded Blacktop QA on Play and owned F8; live acceptance pending)",
  ],
  [
    "blacktop:rules-set",
    "native-rules-apply (explicit single-rule QA, verified read-back and failure rollback; live acceptance pending)",
  ],
  [
    "player23:catalog",
    "original browser search/review/category selection plus local React drafts (native source data, no game writes; live staging acceptance pending)",
  ],
  [
    "player:reference-stats",
    "reference-stats-load / reference-stats-stage (native bounded URL parser, local Player DNA staging; network acceptance pending)",
  ],
  [
    "player-dna:transfer",
    "original browser loaded-player copy and native import/export file dialogs; same review/stage/Save path; separate local React drafts (live/manual UI acceptance pending)",
  ],
  ["camera:save", "save-library"],
  [
    "team-dna:transfer",
    "import-team-dna / team-dna-stage / export-team-dna (offline only)",
  ],
  [
    "ball:select",
    "native-ball-select (explicit independently mapped gameplay-ball QA; no shared prop filename edits on Version 2)",
  ],
  [
    "ball:status",
    "native-session-status / ball (prepared and court-model-confirmed are distinct; appearance unverified)",
  ],
  [
    "scoreboard:select",
    "native-scoreboard-select (explicit Version 2 registry transaction and five frozen layouts)",
  ],
  ["scoreboard:status", "native-session-status / scoreboard"],
  [
    "jukebox:status",
    "jukebox-status (lazy stock catalog, jobs, authoritative current/next-Play state)",
  ],
  [
    "jukebox:choose-audio-batch",
    "jukebox-import-audio (owned native dialog and bounded same-EXE decode worker)",
  ],
  [
    "jukebox:preview-track",
    "jukebox-preview (immutable tokenized loopback audio lease)",
  ],
  ["jukebox:save-track", "jukebox-save-track (durable metadata review)"],
  [
    "jukebox:save-playlist",
    "jukebox-save-playlist (durable CAS revision and ordering)",
  ],
  [
    "jukebox:activate-playlist",
    "jukebox-select-playlist (freeze for next Play; actual playback pending live acceptance)",
  ],
  ["jukebox:delete-playlist", "jukebox-delete-playlist"],
  [
    "jukebox:import-playlist",
    "jukebox-import-playlist / jukebox-export-playlist (.2kplaylist validation and native dialogs)",
  ],
  [
    "jukebox:restore-default",
    "jukebox-select-playlist (default for next Play; no running-game change)",
  ],
  ["presentation:choose-movie", "import-media"],
  ["presentation:preview", "React media player"],
  ["presentation:open-folder", "open-library"],
  ["blacktop:preview-pack-import", "import-previews"],
  ["blacktop:preview-pack-status", "preview-status"],
]);
const routes = [
  ...new Set(
    [
      ...main.matchAll(
        /(?:ipcMain\.handle|registerRoster)\(\s*['"]([^'"]+)['"]/g,
      ),
    ].map((m) => m[1]),
  ),
]
  .sort()
  .map((channel) => ({
    channel,
    status: partial.has(channel) ? "Partial" : "Missing",
    native: partial.get(channel) ?? null,
  }));
const groups = [
  {
    feature: "Portable startup, storage and shutdown",
    status: "Partial",
    remaining:
      "0.36 preserves original WPF data with namespaced settings and reviewed media publication. 0.38 adds active-session close prompts, truthful preparation progress and completed-provider journal reconciliation. Owned helper exits, portable storage and recovery guards are covered offline. Total game launch time, interrupted live cleanup and automatic authenticated reconnection remain open; source/executable rollback checkpoints are retained.",
  },
  {
    feature: "Profiles and mod library",
    status: "Partial",
    remaining:
      "Explicit native mod-profile/provider QA launch, no-copy profile junctions and player-package hard links, authenticated pipe, existing three-archive selector validation/frozen publication and owned restoration are connected. 0.28 adds native stock decoding, portable content-verified caching and WPF-oracle-tested generation when all three selector mods are absent; existing user bundles are retained, unsupported style sets reported, and exact-owned generated files restored. Loader activation/probe and normal cleanup passed live in 0.24; real selected-profile appearance, fresh generated bundles and cleanup still need live acceptance. Import/install conflict review and full profile UI operations remain unfinished.",
  },
  {
    feature: "Jukebox",
    status: "Partial",
    remaining:
      "0.36 imports six saved WPF playlists through the current V2 catalog without re-encoding native Opus; originals remain intact. Native catalog, ordering, audio/.2kplaylist exchange and previews are connected. Conversion is gated against active/uncertain launches with a shared decoder/Play admission gate. Next-Play choices freeze into the provider with pinned chunks/masks/restoration. Actual playback/order/cleanup needs live acceptance; game artwork and the old Freestyle audio-event route remain unported.",
  },
  {
    feature: "Presentation / startup videos / cameras",
    status: "Partial",
    remaining:
      "Saved/new MP4 catalogs, exact launched-window timing, fullscreen playback, Xbox skip and an owned mixer guardian are connected; live timing/input/audio acceptance remains. 0.42 ties import progress/cancel to an exact operation identity and cancels active import work during shutdown. Camera publication validates retained private state before initial publication and reset rearm; H/Z sync and optional lock remain guarded. Camera visibility/tracking and total preparation timing still need gameplay acceptance.",
  },
  {
    feature: "Blacktop overlay and catalogs",
    status: "Partial",
    remaining:
      "0.34 user reports confirm basic mod appearance, F8 manual opening/Alt-Tab hiding and AAU Gym. Its Moneyball affected spectator props, not the court ball. 0.35 replaces new Version 2 requests with the gameplay-setting backend, adds explicit scoreboard transactions/five frozen layouts and presentation execution. Main/Quick Play ownership/restoration is tested offline; new feature gameplay acceptance remains. Version 2 outfit carry-over and automatic fitting/random gameplay are not enabled.",
  },
  {
    feature: "Outfits and previews",
    status: "Partial",
    remaining:
      "Legacy outfit-library compatibility, 3D mannequin, texture/mesh decoder and full render-on-demand cache remain unfinished. Native exact-variant composition and .2koutfit sharing have regression coverage. Manual F8 player binding, Apply/Restore and match carry-over are connected in 0.29 but still require live acceptance; automatic fitting/random game modes remain missing.",
  },
  {
    feature: "Unlocks, crowd and scaler",
    status: "Partial",
    remaining:
      "Player scaling has Version 2 owner/descriptor checks, loaded-cache diagnostics and guarded stock-reset rearming; visual scale remains unverified. The experimental 0.41 WNBA Freestyle sibling-loader route was withdrawn after a live court-entry crash. 0.42 preserves the saved preference but fails closed in the UI, runtime and writer without attempting a WNBA write. Blacktop WNBA and NPC scaling remain unavailable. Drone/DJ/MC and five-size Blacktop crowd routes retain separate gameplay acceptance requirements.",
  },
  {
    feature: "Roster editor / Player DNA",
    status: "Partial",
    remaining:
      "Native reads, player/basic team Save & verify, loaded-player DNA copying, native DNA file import/export, 2K23 category selection and editor preset storage are connected to original browser pages. Original review/staging still precedes explicit Save. Live read/write and native file-dialog/manual UI acceptance remain pending. Live seasons/awards, custom source catalogs, From/audio, artwork/animation, 2KDB/Auction and retained/source catalogs remain unported. Sample previews have separate ports/tokens and no native read/write/file/preset capability. React local-DNA tools stay separate. Real Basketball-Reference fetching still needs acceptance.",
  },
  {
    feature: "Team editor / Team DNA",
    status: "Partial",
    remaining:
      "Native team/arena/uniform browser loading, basic team saves and 52-control uniform Save & verify are connected, with original C#/JS comparisons and failure/rollback tests. Separate arena/floor uses an unused reserved-slot clone, not shared-donor mutation; team/uniform IDs and unmapped uniform bits remain unchanged. Shared uniform slots reload together. Live acceptance remains pending. Staff/rotation/strategy/playbook write services, browser Team DNA staging, artwork and review remain unported. Original browser styling is unchanged; isolated sample previews cannot read or save the game. Separate local Team DNA tools retain import/staging/export, 175 staff controls, 48-minute rotations and 52 uniform controls.",
  },
  {
    feature: "Health, diagnostics and release management",
    status: "Partial",
    remaining:
      "0.43 adds a bounded redacted support ZIP containing only small launcher text/JSON diagnostics; Data, Library, Cache, dumps, binaries, secrets and absolute user paths are excluded. Compatibility by feature remains visible. Game hang/crash capture, deeper native helper diagnostics, performance history and update/rollback transactions remain.",
  },
];
// Capability-level statuses are separate from transport routes. A working
// offline editor is not evidence that its corresponding game write works.
const capabilities = [
  [
    "Portable QA storage and single-instance ownership",
    "Working",
    "src/backend.rs; src/instance.rs; packaged single-instance/export fixtures",
    "Namespaced Rust settings import safe WPF preferences once without overwriting the old settings. Reviewed Main publication covers saved playlists/MP4s only, preserves originals and has a rollback receipt. Full older outfit/preview/advanced settings migration remains incomplete.",
  ],
  [
    "Mod-library tree, search and local tags",
    "Working",
    "frontend/App.tsx; packaged tree-scroll and profile tests",
    "Browsing and local metadata only; installation/conflict resolution is separate.",
  ],
  [
    "Optional 256 px preview ZIP import and persistent cache",
    "Working",
    "src/previews.rs; packaged import/progress/fallback tests",
    "Catalog pictures; no 3D mannequin or render-on-demand coverage claim.",
  ],
  [
    "Local outfit composition and sharing",
    "Working",
    "src/outfits.rs; WPF compatibility oracles and packaged sharing checks",
    "Offline drafts, exact variants and three-accessory limit; no game application.",
  ],
  [
    "Steam without-EAC dispatch and boundary detection",
    "Awaiting live verification",
    "Earlier user live acceptance; src/native_session.rs; src/blacktop_observer_profile.rs; Version 2 disk and fixture checks",
    "Version 2 has separate read-only boundary mappings. Repeat selection, match, Quick Play, Main Menu and exit detection on the new game build; observation grants no memory-edit authority.",
  ],
  [
    "Native mod-profile launch and loader activation",
    "Awaiting live verification",
    "src/provider_launch.rs; src/loader_files.rs; private provider-worker and original-DLL fixtures",
    "QA0.34 user reports say mods appear to load on Version 2. The 47-method provider ABI is independently disk-verified. Full library/Players routing, alternate profiles and normal cleanup still require acceptance; this is not a claim that every third-party mod works.",
  ],
  [
    "Native provider shutdown and profile/file restoration",
    "Awaiting live verification",
    "src/provider_worker.rs; src/mod_profile.rs; src/loader_files.rs; offline failure/ownership tests",
    "0.24 normal game exit restored its exact owned loader files and temporary profile, closed the journal and exited the worker. UI-close-during-game, non-default profiles and failure recovery acceptance remain pending.",
  ],
  [
    "F8 foreground ownership, catalogs and navigation",
    "Awaiting live verification",
    "src/native_overlay.rs; frontend/NativeOverlay.tsx; packaged geometry/focus/toggle fixtures",
    "QA0.34 user confirms manual enable/open and Alt-Tab hiding. Stable-width catalogs and same-button closing have fixture coverage. Repeat transitions, initial auto-show settings and game-exit acceptance remain.",
  ],
  [
    "Blacktop Game Rules read and explicit Apply",
    "Awaiting live verification",
    "src/game_rules.rs; src/rules_v2.rs; src/rules_v2_access.rs; frontend/NativeRules.tsx; independent getters and durable one-rule transactions",
    "0.34 connects all 39 Version 2 controls with one-use snapshots, retained identities, durable intent and full neighboring-value checks. Stock-menu comparison, explicit changes and restoration still require live acceptance.",
  ],
  [
    "Blacktop arena selection and transaction ownership",
    "Awaiting live verification",
    "src/arena_runtime.rs; src/arena_v2.rs; src/arena_ball_v2_adapter.rs; src/arena_ball_v2_access.rs; real Engine/Runner receipt fixtures",
    "QA0.34 user confirms AAU Gym loaded in the match. Other courts/templates, menu restoration, Default and closure remain live checks. Exact selection ownership and Main/Quick Play revalidation pass offline.",
  ],
  [
    "Blacktop live player binding and automatic outfits",
    "Partial",
    "src/outfit_runtime.rs; src/outfit_selection_session.rs; src/outfit_match_runtime.rs; frontend/NativeOutfits.tsx; preserved C#/PowerShell oracles",
    "0.29 connects manual F8 player binding and explicit Apply/Restore to the guarded native writer and proven DLL match worker. Durable page/plan ownership, confirmation, cleanup and Main/Quick Play rearming have fixture coverage, not live acceptance. Match carry-over requires 3–7 pieces and at most three accessories. Automatic selection fitting and random player/all game modes remain missing; local draft randomization is not automatic game application.",
  ],
  [
    "Blacktop ball application and reset",
    "Awaiting live verification",
    "src/ball_gameplay_v2.rs; src/ball_gameplay_runtime.rs; src/ball_gameplay_native.rs; src/ball_runtime.rs; guarded lifecycle and fault fixtures",
    "QA0.34 failed court-ball acceptance: Moneyball appeared only on spectator props. 0.35 changes only the gameplay-setting DWORD after four stable unloaded samples, exact typed empty ball-manager and stock-source checks; no Normal filename edit. Prepared, actual court-model confirmation and visually verified appearance are distinct. New appearance, two-match rearming and menu unload timing need live acceptance.",
  ],
  [
    "Blacktop scoreboard application",
    "Awaiting live verification",
    "src/scoreboard_runtime.rs; src/scoreboard_queue.rs; src/scoreboard_assets.rs; src/provider_assets.rs; actual provider transaction fixtures",
    "0.35 connects explicit Version 2 scoreboard choices and five SHA-bound layouts through the provider. Collisions preserve user files; held owned transactions retain restoration polling. Registry/read-back and layout preparation are not proof of gameplay appearance.",
  ],
  [
    "Drone, DJ, MC and Blacktop-only crowd removal",
    "Awaiting live verification",
    "src/presentation_v2.rs; src/presentation_v2_runtime.rs; src/provider_launch.rs; scoped boundary/five-size lifecycle fixtures",
    "0.35 freezes next-Play options. Drone/DJ/MC keep their session scope; crowd changes prepare at stable Blacktop selection and restore/rearm at verified Main/Quick Play boundaries for five match sizes. Actual standing/stands crowd, other modes and repeated matches need live acceptance.",
  ],
  [
    "WNBA, unlocks and scaler",
    "Partial",
    "src/scaler_v2.rs; src/freestyle_v2.rs; src/feature_runtime.rs; original WPF policy and pinned Version 2 disk audits",
    "Player shared-input scaling has Version 2 owner/descriptor checks, loaded-cache diagnostics and guarded reset rearming; visual acceptance remains. The experimental Version 2 sibling WNBA loader crashed on first live court entry and is withdrawn. The saved preference is preserved, but 0.42 exposes no WNBA write path and reports the feature unavailable. A future mixed-roster/category route requires independent ABI and lifetime proof. No guessed hook, team injection, NPC scaling or Blacktop WNBA is enabled.",
  ],
  [
    "Camera ordering and saved configuration",
    "Awaiting live verification",
    "src/camera_v2.rs; src/camera_localization.rs; src/feature_runtime.rs; WPF byte comparisons and pinned Version 2 disk audit",
    "0.42 retains exact-loaded-base validation and native-ID-preserving camera names, validates the private mode/metadata/parameter allocation before initial publication and every reset rearm, and bounds initial access retries. Camera order, tracking, owned early labels, H/Z synchronization and optional lock are connected. Menu visibility, appearance, label timing and game lifecycle still need live acceptance.",
  ],
  [
    "Startup MP4 playback and game audio coordination",
    "Awaiting live verification",
    "src/video_library.rs; src/startup_video_launch.rs; src/startup_video_tauri.rs; src/startup_video_audio.rs; native/startup-audio-guardian; frontend/StartupIntro.tsx",
    "ID-resolved saved/imported MP4s, exact launched-window timing, fullscreen, keyboard/Xbox skip, Alt-Tab cancellation and conditional owned-mixer restoration are integrated. Real game timing, controller and audio acceptance remain pending; no game MOV replacements or memory patches are used.",
  ],
  [
    "Jukebox game playlists and session restoration",
    "Awaiting live verification",
    "src/jukebox_api.rs; src/jukebox_launch.rs; src/jukebox_runtime.rs; src/provider_assets.rs; frontend/JukeboxNative.tsx; production chunk/cleanup tests",
    "0.35 connects durable playlist selection, native stock/remapped PCM slots, loader masks and exact-owned music/audio publication/restoration. Batch import, .2kplaylist exchange and token audio previews are connected. Game playback/order/cleanup remains unverified; game artwork and the old Freestyle audio-event route are not enabled.",
  ],
  [
    "Player browser editor: snapshots and basic Save & verify",
    "Awaiting live verification",
    "src/roster_native.rs; src/player_transaction.rs; original C#/JS oracles; browser HTTP tests",
    "Native browser route connected; guarded saves and live roster-menu workflow still need acceptance.",
  ],
  [
    "Team browser editor: basic fields, arena/floor and uniforms",
    "Awaiting live verification",
    "src/team_transaction.rs; src/uniform_transaction.rs; original C#/JS oracles",
    "IDs preserved; reserved-slot arena/floor clone and 52-control uniform writes require live acceptance.",
  ],
  [
    "Player/Team DNA and source import workflows",
    "Partial",
    "Original browser assets; local DNA, NBA 2K23 and season regression fixtures",
    "Local review/import/export works; live seasons/awards, Team DNA staging and advanced staff/strategy/playbook services are incomplete.",
  ],
  [
    "3D mannequin and full animation previews",
    "Missing",
    "WPF reference renderer/animation services",
    "Catalog PNGs are not equivalent to mesh/texture/animation preview.",
  ],
  [
    "Release diagnostics, migration and updates",
    "Partial",
    "src/support_bundle.rs; versioned QA packaging, backups, logs and ownership tests",
    "Bounded support redaction is connected. Hang/crash capture, settings/library migration, secondary Steam libraries and update/rollback transactions remain.",
  ],
].map(([feature, status, evidence, remaining]) => ({
  feature,
  status,
  evidence,
  remaining,
}));
const statuses = [
  "Working",
  "Partial",
  "Missing",
  "Awaiting live verification",
];
for (const c of capabilities) {
  if (!statuses.includes(c.status) || !c.evidence || !c.remaining)
    throw Error(`Incomplete capability inventory: ${c.feature}`);
}
const report = {
  deliveryPriority: [
    "actual modded launch: native asset provider, owned loader and session restoration",
    "Blacktop outfit/arena/ball/scoreboard application and selection-to-match carry-over",
    "Main Menu and Quick Play restoration/rearming, then remaining WPF feature parity",
  ],
  target:
    "Dependable WPF feature coverage, familiar visual style and release quality; cleaner implementations and workflows are allowed",
  complete: false,
  baselineVersion: release.version,
  baselineMainSha256: crypto.createHash("sha256").update(main).digest("hex"),
  baselineRoutes: routes.length,
  note: "Partial means a local prototype workflow exists, not equivalent game functionality. Each route requires offline regression tests and, where applicable, live-game acceptance.",
  groups,
  statuses,
  capabilities,
  capabilityCounts: Object.fromEntries(
    statuses.map((status) => [
      status,
      capabilities.filter((c) => c.status === status).length,
    ]),
  ),
  routes,
};
const file = path.join(root, "frontend/parity.json"),
  bytes = JSON.stringify(report, null, 2);
if (!fs.existsSync(file) || fs.readFileSync(file, "utf8") !== bytes)
  fs.writeFileSync(file, bytes);
console.log(
  JSON.stringify({
    baseline: release.version,
    routes: routes.length,
    complete: false,
  }),
);
