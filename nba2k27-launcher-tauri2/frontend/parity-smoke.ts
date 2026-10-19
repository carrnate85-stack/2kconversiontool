import { call } from "./api";
import type { Library, Media, Settings, Outfit } from "./types";
import type { JukeboxView } from "./JukeboxNative";
const delay = (ms = 120) => new Promise((r) => setTimeout(r, ms));
async function until(test: () => boolean, message: string) {
  for (let i = 0; i < 60; i++) {
    if (test()) return;
    await delay(50);
  }
  throw new Error(message);
}
function button(selector: string, text: string) {
  const el = [...document.querySelectorAll<HTMLButtonElement>(selector)].find(
    (b) => b.textContent?.trim() === text,
  );
  if (!el) throw new Error(`Missing button ${text}`);
  return el;
}
function input(selector: string, value: string) {
  const el = document.querySelector<HTMLInputElement>(selector);
  if (!el) throw new Error(`Missing input ${selector}`);
  Object.getOwnPropertyDescriptor(
    HTMLInputElement.prototype,
    "value",
  )!.set!.call(el, value);
  el.dispatchEvent(new Event("input", { bubbles: true }));
  if (el.type === "color")
    el.dispatchEvent(new Event("change", { bubbles: true }));
}
function select(selector: string, value: string) {
  const el = document.querySelector<HTMLSelectElement>(selector);
  if (!el) throw Error(`Missing select ${selector}`);
  el.value = value;
  el.dispatchEvent(new Event("change", { bubbles: true }));
}
async function openLocalEditorTools(
  section: "players" | "teams",
  checkDefault = false,
) {
  await until(
    () => !!document.querySelector(`[data-editor-entry="${section}"]`),
    "Browser editor entry missing",
  );
  const details = document.querySelector<HTMLDetailsElement>(
    "[data-local-editor-tools]",
  );
  if (
    !details ||
    (checkDefault && details.open) ||
    document.querySelectorAll("[data-editor-open]").length !== 1
  )
    throw Error(
      "Browser editor must lead with one primary action and separate collapsed local tools",
    );
  if (checkDefault) await call("capture", { name: `${section}-browser-entry` });
  if (!details.open) details.querySelector("summary")!.click();
  await until(() => details.open, "Local tools did not expand");
}
type Actions = {
  page: (s: string) => void;
  change: (p: Partial<Settings>) => void;
  save: (p: Partial<Library>) => Promise<void>;
  media: (m: Media[]) => void;
  outfit: (o: Outfit) => void;
};
export async function paritySmoke(a: Actions) {
  const checks: string[] = [];
  if (!(await call<boolean>("smoke-single-instance")))
    throw Error("Portable root accepted a second writer");
  checks.push("portable-root-single-instance");
  const fixture = await call<{ modsRoot: string; asset: string }>(
    "smoke-fixture",
  );
  // Profile activation/removal must remain within the inert fixture too.
  // Merely changing modsRoot leaves the default saved profile's folder intact.
  const fixtureLibrary = await call<Library>("library");
  await a.save({
    profiles: fixtureLibrary.profiles.map((profile) => ({
      ...profile,
      folder: fixture.modsRoot,
    })),
  });
  a.change({ modsRoot: fixture.modsRoot });
  a.media(await call<Media[]>("media"));
  await delay();
  a.page("Play session");
  await delay();
  if (
    document.querySelector<HTMLDetailsElement>("details[data-play-advanced]")
      ?.open ||
    document.querySelector(".native-blacktop-observer")
  )
    throw Error("QA diagnostics were exposed on the simple Play page");
  if (document.querySelectorAll("button[data-primary-play]").length !== 1)
    throw Error("The main page did not have one primary Play action");
  await call("capture", { name: "play-home" });
  document
    .querySelector<HTMLElement>("details[data-play-advanced] > summary")!
    .click();
  await until(
    () => !!document.querySelector(".native-blacktop-observer"),
    "Advanced launch tools did not open",
  );
  checks.push("simple-play-home-advanced-closed-by-default");
  button(".launch-readiness button", "Check launch readiness").click();
  await until(
    () =>
      document
        .querySelector(".launch-readiness")
        ?.textContent?.includes("Read-only checks passed") ?? false,
    "Native launch preflight fixture did not pass",
  );
  const preflight = await call<any>("launch-preflight");
  if (
    preflight.fixture !== true ||
    preflight.liveGameEnabled !== false ||
    preflight.launchPlan.steamUrl !== "steam://launch/123456/option1"
  )
    throw Error(
      "Preflight did not retain disabled live execution or exact launch option",
    );
  checks.push(
    "native-read-only-launch-preflight-ui",
    "validated-steam-without-eac-plan",
    "game-feature-writes-remain-disabled",
  );
  button(".launch-readiness button", "Launch game only (QA)").click();
  await until(
    () => !!document.querySelector(".native-launch-confirm"),
    "Steam QA confirmation did not open",
  );
  if (!button(".native-launch-confirm button", "Launch via Steam").disabled)
    throw Error("Unacknowledged launch was enabled");
  document
    .querySelector<HTMLInputElement>(".native-launch-confirm input")!
    .click();
  await until(
    () => !button(".native-launch-confirm button", "Launch via Steam").disabled,
    "Launch acknowledgement did not enable confirmation",
  );
  await call("capture", { name: "steam-only-confirmation" });
  button(".native-launch-confirm button", "Cancel").click();
  await until(
    () => !document.querySelector(".native-launch-confirm"),
    "Cancel did not close confirmation",
  );
  let launchRefused = false;
  try {
    await call("native-launch-steam-only", {
      acknowledgement: "steam-only-no-features",
    });
  } catch (e) {
    launchRefused = String(e).includes("disabled in smoke mode");
  }
  if (!launchRefused) throw Error("Smoke mode allowed real Steam dispatch");
  // One-click Play (WPF parity): a normal modded launch opens no
  // acknowledgement dialog and goes straight to the guarded backend, which
  // smoke mode refuses before any Steam dispatch.
  document
    .querySelector<HTMLButtonElement>("button[data-primary-play]")!
    .click();
  await until(
    () => (document.body.textContent ?? "").includes("disabled in smoke mode"),
    "One-click Play did not reach the guarded launch request",
  );
  if (document.querySelector(".native-launch-confirm"))
    throw Error("Normal Play still opened a confirmation dialog");
  await call("capture", { name: "native-one-click-play" });
  let moddedRefused = false;
  try {
    await call("native-launch-modded", {
      acknowledgement: "native-modded-qa-explicit-blacktop",
    });
  } catch (e) {
    moddedRefused = String(e).includes("disabled in smoke mode");
  }
  if (!moddedRefused) throw Error("Smoke mode allowed a real modded launch");
  let arenaRefused = false;
  try {
    await call("native-arena-select", {
      acknowledgement: "apply-one-arena-for-current-game",
      session: "a".repeat(48),
      assetId: "arena_000_int",
    });
  } catch (e) {
    arenaRefused = String(e).includes("disabled in smoke mode");
  }
  if (!arenaRefused) throw Error("Smoke mode allowed a live arena write");
  let ballRefused = false;
  try {
    await call("native-ball-select", {
      acknowledgement: "apply-one-ball-for-current-game",
      session: "a".repeat(48),
      assetId: "current",
    });
  } catch (e) {
    ballRefused = String(e).includes("disabled in smoke mode");
  }
  if (!ballRefused) throw Error("Smoke mode allowed a live ball write");
  let outfitRefused = false;
  try {
    await call("native-outfit-command", {
      acknowledgement: "read-blacktop-outfits",
      session: "a".repeat(48),
      command: { action: "list" },
    });
  } catch (e) {
    outfitRefused = String(e).includes("disabled in smoke mode");
  }
  if (!outfitRefused) throw Error("Smoke mode allowed live outfit game access");
  checks.push(
    "native-one-click-play-without-confirmation",
    "real-modded-launch-refused-in-smoke",
    "real-arena-write-refused-in-smoke",
    "real-ball-write-refused-in-smoke",
    "live-outfit-access-refused-in-smoke",
  );
  const lifecycle = await call<any>("smoke-native-session");
  if (
    !lifecycle.pass ||
    lifecycle.realGameLaunched !== false ||
    lifecycle.steamDispatches !== 1 ||
    lifecycle.monitorShutdownMs >= 500
  )
    throw Error("Native session fixture failed");
  const helper = await call<any>("smoke-owned-helper");
  if (
    !helper.pass ||
    helper.remainingOwnedHelpers !== 0 ||
    helper.foreignStopRefused !== true ||
    helper.gameWrites !== false
  )
    throw Error("Owned helper process verification failed");
  checks.push(
    "steam-only-confirmation-gate",
    "real-steam-launch-refused-in-smoke",
    "durable-session-lifecycle-fixture",
    "native-monitor-shutdown-under-500ms",
    "owned-helper-graceful-and-job-close-exit",
    "foreign-helper-stop-refused",
  );
  if (
    !button(".native-blacktop-observer button", "Start read-only observer")
      .disabled
  )
    throw Error("Observer enabled without launched game");
  const observerFixture = await call<any>("smoke-blacktop-observer");
  if (!button(".native-overlay-controls button", "Enable native F8").disabled)
    throw Error("Native F8 enabled without launched game/acknowledgement");
  let f8Refused = false;
  try {
    await call("native-overlay-enable", {
      acknowledgement: "native-overlay-explicit-qa",
    });
  } catch (e) {
    f8Refused = String(e).includes("disabled in smoke mode");
  }
  if (!f8Refused) throw Error("Smoke enabled production F8");
  await call("smoke-native-overlay-open");
  let overlayReport: any = null;
  for (let i = 0; i < 100; i++) {
    overlayReport = await call("smoke-native-overlay-result");
    if (overlayReport) break;
    await delay(75);
  }
  if (!overlayReport?.pass)
    throw Error(
      `Owned native overlay fixture failed: ${overlayReport?.error ?? "timeout"}`,
    );
  const overlayStop = await call<any>("smoke-native-overlay-stop");
  if (
    !overlayStop.fixture ||
    overlayStop.milliseconds >= 500 ||
    overlayStop.status.enabled
  )
    throw Error("Disabled overlay monitor did not stop promptly");
  checks.push(
    "native-overlay-production-enable-refused-in-smoke",
    "native-overlay-monitor-shutdown-under-500ms",
    ...overlayReport.checks,
  );
  if (
    !observerFixture.pass ||
    observerFixture.writes !== 0 ||
    !observerFixture.tornRejected ||
    !observerFixture.selection.lineupReady ||
    observerFixture.quickPlay.boundary !== "quick-play"
  )
    throw Error("Native Blacktop detector fixture failed");
  let observerRefused = false;
  try {
    await call("native-blacktop-enable", {
      acknowledgement: "read-only-blacktop",
    });
  } catch (e) {
    observerRefused = String(e).includes("disabled in smoke mode");
  }
  if (!observerRefused) throw Error("Smoke enabled live memory observer");
  const rulesFixture = await call<any>("smoke-game-rules");
  if (
    !rulesFixture.pass ||
    rulesFixture.mappedRules !== 39 ||
    !rulesFixture.ownedRollback ||
    !rulesFixture.foreignValueHeld
  )
    throw Error("Rules transaction fixture failed");
  for (const channel of ["native-rules-read", "native-rules-apply"]) {
    let refused = false;
    try {
      await call(channel, { acknowledgement: "apply-one-rule" });
    } catch (e) {
      refused = String(e).includes("disabled in smoke mode");
    }
    if (!refused) throw Error("Smoke permitted live rules access");
  }
  button(".native-rules button", "Load isolated rules fixture").click();
  await until(
    () => document.querySelectorAll(".native-rule-row").length === 39,
    "All rules did not render",
  );
  // WPF parity: no confirmation checkbox or Apply button; choosing a value is
  // the change. Off/On rules are two buttons, others a list.
  const rulesPanel = document.querySelector(".native-rules")!;
  if (
    rulesPanel.querySelector('input[type="checkbox"]') ||
    [...rulesPanel.querySelectorAll("button")].some(
      (b) => b.textContent?.trim() === "Apply",
    )
  )
    throw Error("Rules still require a confirmation or Apply step");
  const shotClock = [...document.querySelectorAll(".native-rule-row")].find(
    (r) => r.querySelector(".native-rule-name")?.textContent === "Shot Clock",
  );
  if (
    shotClock &&
    shotClock.querySelectorAll("button[aria-pressed]").length !== 2
  )
    throw Error("Off/On rule is not drawn as two buttons");
  rulesPanel.scrollIntoView({ block: "start" });
  await delay();
  await call("capture", { name: "native-rules-review" });
  document
    .querySelector('[data-rule="ThreePointShotRestriction"]')!
    .scrollIntoView({ block: "center" });
  await delay();
  await call("capture", { name: "native-rules-numeric-choices" });
  await call("smoke-resize", { width: 1060, height: 700 });
  await delay();
  const rulesGrid = document.querySelector<HTMLElement>(".native-rules-grid")!;
  if (rulesGrid.scrollWidth > rulesGrid.clientWidth + 1)
    throw Error("Rules grid has horizontal overflow");
  document.querySelector(".native-rules")!.scrollIntoView({ block: "start" });
  await delay();
  await call("capture", { name: "native-rules-minimum-size" });
  await call("smoke-resize", { width: 1440, height: 900 });
  await delay();
  select('[data-rule="ThreePointShotRestriction"] select', "2");
  await until(
    () =>
      document
        .querySelector(".native-rules")
        ?.textContent?.includes("disabled in smoke mode") ?? false,
    "Live rule change was not visibly refused",
  );
  if (document.querySelectorAll(".native-rule-row").length)
    throw Error("Rules snapshot remained after a refused change");
  checks.push(
    "native-rules-39-mappings-and-rollback-fixtures",
    "live-rule-reads-and-writes-refused-in-smoke",
    "native-rules-choose-applies-and-consumed-snapshot",
    "native-rules-minimum-width-no-overflow",
  );
  checks.push(
    "native-blacktop-sparse-memory-boundaries",
    "live-observer-disabled-in-smoke-and-unlaunched-ui",
  );
  document.querySelector<HTMLElement>(".page")!.scrollTop = 0;
  window.scrollTo(0, 0);
  await call("capture", { name: "launch-readiness" });
  await call("smoke-resize", { width: 1060, height: 700 });
  await until(() => innerWidth <= 1061, "Minimum window resize did not apply");
  const page = document.querySelector<HTMLElement>(".page")!;
  if (page.scrollWidth > page.clientWidth + 1)
    throw Error("Minimum window has horizontal page overflow");
  await call("capture", { name: "play-minimum-size" });
  await call("smoke-resize", { width: 1440, height: 900 });
  await until(() => innerWidth >= 1439, "Window size did not restore");
  checks.push("minimum-supported-window-no-horizontal-overflow");
  a.page("Profiles");
  await until(
    () => !!document.querySelector("#page-profiles .profiles-grid"),
    "Profiles did not render",
  );
  const originalProfiles = await call<Library>("library");
  if (originalProfiles.profiles.length !== 1)
    throw Error("Profile smoke fixture did not start with one profile");
  const originalProfile = originalProfiles.profiles[0];
  // WPF flow: New profile picks a folder first (smoke returns an owned empty
  // fixture), then names it in a dialog; the new profile becomes active.
  const checkedProfile = () =>
    document
      .querySelector('.profile-row input[type="radio"]:checked')
      ?.closest(".profile-row")
      ?.querySelector("strong")?.textContent ?? "";
  button(".profiles-saved-card button", "New profile").click();
  await until(
    () =>
      !!document.querySelector(".profile-dialog") &&
      !!document.querySelector('[aria-label="Profile name"]'),
    "New profile dialog did not open",
  );
  if (
    !document
      .querySelector(".profile-dialog-path")
      ?.textContent?.includes("SmokeProfile")
  )
    throw Error("New profile did not use the picked folder");
  input('[aria-label="Profile name"]', "QA profile");
  button(".profile-dialog button", "Create profile").click();
  await until(
    () =>
      !document.querySelector(".profile-dialog") &&
      document.querySelectorAll(".profile-list .profile-row").length === 2 &&
      checkedProfile() === "QA profile",
    "New profile was not created and selected",
  );
  document
    .querySelector<HTMLButtonElement>('[aria-label="Edit QA profile profile"]')!
    .click();
  await until(
    () => !!document.querySelector('[aria-label="Profile name"]'),
    "Edit profile dialog did not open",
  );
  input('[aria-label="Profile name"]', "QA renamed profile");
  button(".profile-dialog button", "Save changes").click();
  await until(
    () =>
      !document.querySelector(".profile-dialog") &&
      checkedProfile() === "QA renamed profile",
    "Renamed profile was not saved and kept active",
  );
  let savedProfiles = await call<Library>("library");
  if (
    savedProfiles.profiles.length !== 2 ||
    !savedProfiles.profiles.some(
      (profile) =>
        profile.name === "QA renamed profile" &&
        profile.folder.includes("SmokeProfile"),
    )
  )
    throw Error("Created and renamed profile did not persist");
  if (
    document.querySelector('[aria-label="Remove QA renamed profile profile"]')
  )
    throw Error("The active profile offered Remove");
  const originalRow = [
    ...document.querySelectorAll<HTMLElement>(".profile-row"),
  ].find(
    (row) => row.querySelector("strong")?.textContent === originalProfile.name,
  );
  originalRow?.querySelector<HTMLInputElement>('input[type="radio"]')?.click();
  await until(
    () =>
      checkedProfile() === originalProfile.name &&
      !!document.querySelector(
        '[aria-label="Remove QA renamed profile profile"]',
      ),
    "Choosing the original profile did not make it active",
  );
  document
    .querySelector<HTMLButtonElement>(
      '[aria-label="Remove QA renamed profile profile"]',
    )!
    .click();
  await until(
    () =>
      document.querySelectorAll(".profile-list .profile-row").length === 1 &&
      checkedProfile() === originalProfile.name,
    "Removing an inactive profile did not keep the active profile",
  );
  savedProfiles = await call<Library>("library");
  if (
    savedProfiles.profiles.length !== 1 ||
    savedProfiles.profiles[0].name !== originalProfile.name ||
    savedProfiles.profiles[0].folder !== originalProfile.folder
  )
    throw Error("Profile removal did not preserve the original profile");
  await call("capture", { name: "profiles" });
  checks.push(
    "saved-profile-native-ui",
    "profile-create-rename-activate-remove-safe-fallback",
  );
  a.page("Mod library");
  await until(
    () => !!document.querySelector("#modRows .mod-row"),
    "Mod fixture scan missing",
  );
  // 60 single files plus one pngNNNN bundle (base + dependency).
  await until(
    () => document.querySelectorAll("#modRows .mod-row").length === 61,
    "Mod library did not list every fixture entry",
  );
  const bundle = '[aria-label="Expand char/group-12/png1234"]';
  const scroll = document.querySelector<HTMLDivElement>("#modRows")!;
  scroll.scrollTop = 180;
  const before = scroll.scrollTop;
  if (!before) throw Error("Mod library table did not scroll");
  document.querySelector<HTMLButtonElement>(bundle)!.click();
  await until(
    () => document.querySelectorAll(".mod-dependency-row").length === 2,
    "Mod bundle did not expand",
  );
  if (scroll.scrollTop !== before)
    throw Error("Bundle expansion reset scroll position");
  checks.push("mod-tree-expand-preserves-scroll");
  const tagButton = '[aria-label="Edit tags for char/group-12/player-12.iff"]';
  document.querySelector<HTMLButtonElement>(tagButton)!.click();
  await until(
    () => !!document.querySelector(".mod-tag-editor input"),
    "Mod tag editor did not open",
  );
  input(".mod-tag-editor input", "Curry test");
  await delay();
  button(".mod-tag-editor button", "Save tags").click();
  await until(
    () =>
      !document.querySelector(".mod-tag-editor") &&
      (document.querySelector(tagButton)?.textContent ?? "").includes(
        "Curry test",
      ),
    "Tags not saved",
  );
  if (
    !Object.values((await call<Library>("library")).tags).includes("Curry test")
  )
    throw Error("Tags did not persist");
  input('[aria-label="Search mods"]', "curry test");
  await until(
    () => document.querySelectorAll("#modRows .mod-row").length === 1,
    "Tag search did not find the tagged mod",
  );
  input('[aria-label="Search mods"]', "");
  await until(
    () => document.querySelectorAll("#modRows .mod-row").length === 61,
    "Clearing the mod search did not restore every entry",
  );
  await call("capture", { name: "mod-library" });
  checks.push("mod-tag-native-persistence");
  a.page("Jukebox");
  const songRows = () => document.querySelectorAll("#jukeboxRows .jukebox-row");
  await until(
    () => songRows().length === 2,
    "Native inert-stock default playlist missing",
  );
  const jukeGet = () => call<JukeboxView>("jukebox:get");
  let juke = await jukeGet();
  if (
    !juke.state ||
    juke.capabilities.nativePlayback ||
    juke.capabilities.stockPreview
  )
    throw Error("Smoke Jukebox acquired a game playback/read capability");
  if (juke.stockKeys.join(",") !== "1:1:1,1:100:100")
    throw Error("Smoke Jukebox did not use isolated inert stock");
  button(".jukebox-playlist-row button", "New").click();
  await until(
    () => !!document.querySelector('[aria-label="New playlist"]'),
    "New playlist dialog missing",
  );
  input('[aria-label="Playlist name"]', "Smoke native mix");
  // WPF default: "Add all default songs" starts unticked.
  if (
    document.querySelector<HTMLInputElement>(
      '[aria-label="New playlist"] input[type="checkbox"]',
    )!.checked
  )
    throw Error("New playlist must start empty like WPF");
  button('[aria-label="New playlist"] button', "Create playlist").click();
  await until(
    () =>
      !document.querySelector('[aria-label="New playlist"]') &&
      songRows().length === 0,
    "Empty playlist creation not acknowledged",
  );
  button(".jukebox-song-tools button", "Add from library").click();
  await until(
    () => !!document.querySelector('[aria-label="Music catalog"]'),
    "Library chooser missing",
  );
  input('[aria-label="Search library"]', "100");
  await until(
    () => document.querySelectorAll(".juke-library-list > div").length === 1,
    "Library search did not filter",
  );
  button(".juke-library-list button", "Add").click();
  await until(
    () =>
      [
        ...document.querySelectorAll<HTMLButtonElement>(
          ".juke-library-list button",
        ),
      ].find((b) => b.textContent === "Added")?.disabled === true,
    "Version2 stock song not added",
  );
  input('[aria-label="Search library"]', "");
  await until(
    () => document.querySelectorAll(".juke-library-list > div").length === 2,
    "Library search did not clear",
  );
  button(".juke-library-list > div:first-child button", "Add").click();
  await until(
    () =>
      [...document.querySelectorAll(".juke-library-list button")].filter(
        (b) => b.textContent === "Added",
      ).length === 2 &&
      document.querySelector(".juke-workspace")?.getAttribute("aria-busy") ===
        "false",
    "Second stock song not added",
  );
  button('[aria-label="Add from library"] button', "Done").click();
  await until(
    () => !document.querySelector('[aria-label="Add from library"]'),
    "Library chooser did not close",
  );
  await until(() => songRows().length === 2, "Playlist songs missing");
  document
    .querySelector<HTMLButtonElement>('[aria-label="Move song 2 up"]')!
    .click();
  await until(
    () => /Smoke stock 1(?!\d)/.test(songRows()[0]?.textContent ?? ""),
    "Native song ordering not saved",
  );
  juke = await jukeGet();
  const nativePlaylist = juke.state!.playlists.find(
    (p) => p.name === "Smoke native mix",
  )!;
  if (nativePlaylist?.trackKeys.join(",") !== "1:1:1,1:100:100")
    throw Error("Full stock identities/order lost");
  async function finishedJukeJob(previous: string | undefined) {
    for (let i = 0; i < 100; i++) {
      const value = await jukeGet();
      if (
        value.job &&
        value.job.id !== previous &&
        ["completed", "failed", "cancelled"].includes(value.job.phase)
      ) {
        if (value.job.phase !== "completed")
          throw Error(value.job.error || value.job.message);
        return value;
      }
      await delay(40);
    }
    throw Error("Native Jukebox job did not finish");
  }
  button(".jukebox-song-tools button", "Batch add").click();
  juke = await finishedJukeJob(juke.job?.id);
  await until(
    () =>
      songRows().length === 3 &&
      !button(".jukebox-playlist-row button", "Save").disabled,
    "Native audio import not reflected",
  );
  const importedJukePlaylist = juke.state!.playlists.find(
    (p) => p.id === nativePlaylist.id,
  )!;
  const customKey = importedJukePlaylist.trackKeys.find(
    (key) => !juke.stockKeys.includes(key),
  )!;
  if (!customKey || Object.keys(juke.state!.audio).length !== 1)
    throw Error("Native PCM import missing");
  const includeFirst = document.querySelector<HTMLInputElement>(
    '#jukeboxRows .jukebox-row:first-child input[type="checkbox"]',
  )!;
  includeFirst.click();
  await until(
    () =>
      button("#jukeboxRows .jukebox-row:last-child button", "Remove").disabled,
    "Pending song choices did not block a conflicting membership mutation",
  );
  if (
    !button(".jukebox-song-tools button", "Batch add").disabled ||
    !button(".jukebox-song-tools button", "Add from library").disabled
  )
    throw Error("Adding songs could silently discard unchecked choices");
  button(".jukebox-controls button", "Restore selection").click();
  await until(
    () => !button(".jukebox-song-tools button", "Batch add").disabled,
    "Restoring choices did not unlock editing",
  );
  const list = document.querySelector<HTMLElement>("#jukeboxRows")!;
  list.style.maxHeight = "90px";
  list.scrollTop = 54;
  const focus = document.querySelector<HTMLButtonElement>(
    '[aria-label="Move song 2 up"]',
  )!;
  focus.focus({ preventScroll: true });
  const scrollTop = list.scrollTop;
  await delay(2850);
  if (document.activeElement !== focus || list.scrollTop !== scrollTop)
    throw Error("Jukebox polling replaced rows, scroll or focus");
  list.style.maxHeight = "";
  button(`[data-track-key="${customKey}"] button`, "Edit").click();
  await until(
    () => !!document.querySelector('[aria-label="Edit song"]'),
    "Song editor did not open",
  );
  await until(
    () => !button('[aria-label="Edit song"] button', "Choose file").disabled,
    "Artwork editor not ready",
  );
  input('[aria-label="Song title"]', "Smoke edited song");
  input('[aria-label="Song artist"]', "Smoke edited artist");
  button('[aria-label="Edit song"] button', "Choose file").click();
  await until(() => {
    const img = document.querySelector<HTMLImageElement>(
      '[alt="Song artwork preview"]',
    );
    return !!img?.complete && img.naturalWidth === 256;
  }, "Native prepared image not decoded at256px");
  button('[aria-label="Edit song"] button', "Save song").click();
  await until(
    () =>
      !document.querySelector('[aria-label="Edit song"]') &&
      document
        .querySelector(`[data-track-key="${customKey}"]`)
        ?.textContent?.includes("Smoke edited song") === true,
    "Metadata/artwork edit not saved",
  );
  juke = await jukeGet();
  if (!juke.state?.artworkOverrides?.[customKey])
    throw Error("Artwork override absent afterSave");
  const editedTrack = juke.state.music.Tracks.find(
    (t) => `${t.Collection}:${t["Bank Index"]}:${t["Unique Id"]}` === customKey,
  )!;
  if (editedTrack.Artist !== "Smoke edited artist")
    throw Error("Artist edit not saved");
  // Share only real imported PCM; inert stock has no game-bank bytes to export.
  for (const count of [2, 1]) {
    button("#jukeboxRows .jukebox-row:first-child button", "Remove").click();
    await until(
      () => songRows().length === count,
      "Stock fixture removal not saved",
    );
  }
  button(".jukebox-playlist-row button", "Save").click();
  juke = await finishedJukeJob(juke.job?.id);
  await until(
    () => !button(".jukebox-playlist-row button", "Import").disabled,
    "Export remained busy",
  );
  button(".jukebox-playlist-row button", "Import").click();
  juke = await finishedJukeJob(juke.job?.id);
  if (
    juke.state!.playlists.length !== 2 ||
    juke.state!.playlists.some((p) => p.trackKeys.length !== 1)
  )
    throw Error("Sharing roundtrip changed song counts");
  const roundtripKey = juke.state!.playlists[1].trackKeys[0];
  const roundtrip = juke.state!.music.Tracks.find(
    (t) =>
      `${t.Collection}:${t["Bank Index"]}:${t["Unique Id"]}` === roundtripKey,
  );
  if (
    roundtrip?.Name !== "Smoke edited song" ||
    roundtrip?.Artist !== "Smoke edited artist"
  )
    throw Error("Sharing roundtrip lost edited metadata");
  if (
    !juke.state!.artworkOverrides?.[roundtripKey] &&
    !juke.state!.audio[roundtripKey]?.preview
  )
    throw Error("Sharing roundtrip lost saved artwork");
  if (juke.capabilities.nativePlayback || juke.capabilities.stockPreview)
    throw Error("Isolated smoke enabled a game capability");
  await call("capture", { name: "jukebox" });
  checks.push(
    "native-jukebox-inert-stock-search",
    "native-playlist-create-order-persistence",
    "native-jukebox-pcm-import",
    "native-jukebox-poll-scroll-focus-stability",
    "native-jukebox-checkbox-conflict-preservation",
    "native-jukebox-song-metadata-artwork-editor",
    "native-2kplaylist-edited-artwork-roundtrip",
    "native-jukebox-smoke-no-game-access",
  );
  a.page("Presentation");
  await until(
    () => !!document.querySelector("#cameraRows .camera-row"),
    "Camera table missing",
  );
  const first = (await call<Library>("library")).cameras[0].name;
  document
    .querySelector<HTMLButtonElement>(
      "#cameraRows .camera-row [data-move=down]",
    )!
    .click();
  await delay();
  input("#cameraRows .camera-row:nth-child(2) input[type=number]", "1.5");
  await delay();
  if (!button(".camera-options button", "Save camera setup").disabled)
    throw Error("Fractional camera steps were allowed for native launch");
  checks.push("camera-fractional-steps-blocked-before-save");
  input("#cameraRows .camera-row:nth-child(2) input[type=number]", "2");
  input("#cameraRows .camera-row:nth-child(2) .camera-name input", "My broadcast");
  await delay();
  button(".camera-options button", "Save camera setup").click();
  await until(
    () =>
      document
        .querySelector("#toast")
        ?.textContent?.includes("Camera names, order, height and zoom saved") ??
      false,
    "Camera save did not complete",
  );
  const cameras = (await call<Library>("library")).cameras;
  if (
    cameras[1].name !== "My broadcast" ||
    cameras[1].height !== 2 ||
    cameras[1].id !== 51
  )
    throw Error("Camera ordering/name/native identity/height not persisted");
  if (
    first !== "Broadcast Low 2K18" ||
    [...document.querySelectorAll<HTMLInputElement>(".camera-name input")].filter(
      (el) => el.disabled,
    ).length !== 2
  )
    throw Error("Known stock camera names were not locked");
  checks.push("camera-native-name-edit-and-stock-locks");
  if (!document.querySelector<HTMLDetailsElement>(".videos")?.open)
    throw Error("Startup videos not expanded by default");
  await call("capture", { name: "presentation" });
  checks.push("camera-reorder-height-persistence", "startup-videos-expanded");
  a.page("Blacktop");
  await until(
    () => !!document.querySelector(".outfit-creator"),
    "Outfit creator missing",
  );
  await call("capture", { name: "blacktop-setup" });
  document
    .querySelector<HTMLButtonElement>(".outfit-pieces .outfit-piece-button")!
    .click();
  await until(
    () => document.querySelectorAll(".clothing-picker-grid .clothing-tile").length === 16,
    "Outfit catalog missing",
  );
  if (
    !document
      .querySelector(".clothing-picker-grid .clothing-tile")
      ?.textContent?.includes("Shirtless")
  )
    throw Error("Shirtless must be the first paginated top choice");
  await call("capture", { name: "outfit-picker" });
  const catalogBody = document.querySelector<HTMLElement>(
    ".outfit-picker .clothing-picker-grid",
  )!;
  if (catalogBody.scrollHeight > catalogBody.clientHeight + 1)
    throw Error(
      `16 top choices do not fit the normal-height picker: ${catalogBody.scrollHeight}px required, ${catalogBody.clientHeight}px available`,
    );
  checks.push("outfit-page-fits-four-rows-without-inner-scroll");
  const firstAsset = document.querySelector(
    ".clothing-picker-grid .clothing-tile:last-child",
  )?.textContent;
  button(".clothing-picker-pager button", "Next").click();
  await until(
    () =>
      document.querySelector(".clothing-picker-grid .clothing-tile:last-child")?.textContent !==
      firstAsset,
    "Catalog pagination did not advance",
  );
  if (document.querySelectorAll(".clothing-picker-grid .clothing-tile").length !== 16)
    throw Error("Wrong catalog page size");
  checks.push("outfit-catalog-pagination");
  button(".clothing-category-filters button", "Tank tops").click();
  await until(
    () =>
      document.querySelector(".clothing-category-filters [aria-pressed=true]")?.textContent ===
        "Tank tops" &&
      !!document
        .querySelector(".clothing-picker-grid .clothing-tile")
        ?.textContent?.toLowerCase()
        .includes("tank"),
    "Clothing category filter did not populate",
  );
  if (document.querySelectorAll(".clothing-picker-grid .clothing-tile").length !== 16)
    throw Error("Filtered category page size is wrong");
  checks.push("garment-category-filter-native-ui");
  await call("capture", { name: "outfit-tank-category" });
  button(".clothing-picker-heading button", "Back").click();
  a.outfit({
    name: "QA outfit",
    top: fixture.asset,
    bottom: "",
    shoes: "Keep these shoes",
    pool: "male",
    accessories: [],
  });
  await until(
    () =>
      !!document.querySelector<HTMLImageElement>(".outfit-pieces img")
        ?.complete &&
      document.querySelector<HTMLImageElement>(".outfit-pieces img")!
        .naturalWidth === 256,
    "Optional ZIP cache image did not render",
  );
  checks.push("persistent-zip-exact-id-native-asset");
  button(".outfit-random button", "Random clothes + accessories").click();
  await delay(250);
  button(".outfit-library-bar button", "Save outfit").click();
  await until(
    () =>
      document
        .querySelector("#toast")
        ?.textContent?.includes("Outfit saved") ?? false,
    "Outfit did not save",
  );
  const outfit = await call<Outfit>("outfit");
  if (outfit.shoes !== "Keep these shoes" || outfit.accessories.length > 3)
    throw Error("Random draft violated shoes/accessory rules");
  const saved = (await call<Library>("library")).outfits.find(
    (o) => o.name === "QA outfit",
  );
  if (!saved) throw Error("Saved outfit not in library");
  checks.push(
    "random-shoes-unchanged-accessories-cap",
    "outfit-library-native-save",
  );
  const original = await call<Outfit>("outfit-original", {
    id: "shirtless-male",
    name: "QA shared outfit",
  });
  a.outfit(original);
  await until(
    () =>
      document.querySelector<HTMLInputElement>('[aria-label="Outfit name"]')
        ?.value === "QA shared outfit" &&
      !!document.querySelector<HTMLButtonElement>(
        "#outfitExport",
      ) &&
      !document.querySelector<HTMLButtonElement>(
        "#outfitExport",
      )!.disabled,
    "Validated fit did not enable sharing",
  );
  const inspected = await call<{ items: string[]; share: unknown }>(
    "outfit-inspect",
    original,
  );
  const imported = await call<Outfit>("outfit-decode", inspected.share);
  if (
    JSON.stringify(
      (await call<{ items: string[] }>("outfit-inspect", imported)).items,
    ) !== JSON.stringify(inspected.items)
  )
    throw Error("Shared outfit changed original piece order");
  button(".outfit-actions-menu button", "Export outfit").click();
  await until(
    () =>
      document
        .querySelector("#toast")
        ?.textContent?.includes("Outfit exported:") ?? false,
    "UI outfit export failed",
  );
  checks.push(
    "outfit-exact-variant-share-round-trip",
    "outfit-native-export-ui",
  );
  document.querySelector(".outfit-creator")!.scrollIntoView();
  await delay();
  await call("capture", { name: "outfit-library" });
  a.page("Roster Editor");
  await openLocalEditorTools("players", true);
  await until(
    () =>
      [
        ...document.querySelectorAll<HTMLButtonElement>(
          ".player-dna-editor button",
        ),
      ].some(
        (b) => b.textContent?.trim() === "New local player" && !b.disabled,
      ),
    "Player DNA schema did not enable the new-player button",
  );
  button(".player-dna-editor button", "New local player").click();
  await until(
    () => !!document.querySelector('[aria-label="DNA Birthday"]'),
    "Player info controls missing",
  );
  input('[aria-label="DNA Birthday"]', "2000-02-29");
  input('[aria-label="DNA Years pro"]', "7");
  await delay();
  button(".dna-save button", "Save local draft").click();
  await until(
    () =>
      document
        .querySelector("#toast")
        ?.textContent?.includes("Player DNA draft saved") ?? false,
    "DNA draft did not save",
  );
  const dna = await call<{ info: { birthDate: string; yearsPro: number } }>(
    "player-dna",
  );
  if (dna.info.birthDate !== "2000-02-29" || dna.info.yearsPro !== 7)
    throw Error("Native birthday or years-pro save incorrect");
  checks.push("player-dna-native-birthday-years-pro-ui-save");
  input('[aria-label="DNA Years pro"]', "8");
  await delay();
  button(".dna-save button", "Reset local edits").click();
  await until(
    () =>
      document.querySelector<HTMLInputElement>('[aria-label="DNA Years pro"]')
        ?.value === "7",
    "DNA reset failed",
  );
  checks.push("player-dna-reset-without-live-refresh");
  const webSchema = await call<any>("player-dna-schema");
  const attribute = webSchema.fields.find(
    (f: any) => f.group === "attributes" && f.label === "Driving Layup",
  );
  const salary = webSchema.fields.find((f: any) => f.money);
  const importedPlayer = await call<any>("player-dna");
  importedPlayer.fields[attribute.key] = 165; // Displays 80, but differs from the usual newly edited raw 164.
  importedPlayer.fields[salary.key] = 1234567;
  importedPlayer.info.heightCm = 181.237;
  importedPlayer.info.wingspanCm = 190.589;
  await call("smoke-player-import-file", importedPlayer);
  button(".player-dna-editor button", "Import .2kplayer").click();
  await until(
    () => !!document.querySelector(".player-dna-editor .dna-import-review"),
    "Player file review missing",
  );
  button(
    ".player-dna-editor .dna-import-review button",
    "Stage selected values",
  ).click();
  await until(
    () => !document.querySelector(".player-dna-editor .dna-import-review"),
    "Player file did not stage",
  );
  await saveSeasonDraft();
  button(".player-dna-editor nav button", "Attributes").click();
  await until(
    () =>
      document.querySelector<HTMLInputElement>(
        '[aria-label="DNA Driving Layup"]',
      )?.value === "80",
    "WPF rating display missing",
  );
  document
    .querySelector<HTMLInputElement>('[aria-label="DNA Driving Layup"]')!
    .dispatchEvent(new FocusEvent("focusout", { bubbles: true }));
  await delay();
  if (
    !button(".player-dna-editor .dna-save button", "Save local draft")
      .disabled ||
    (await call<any>("player-dna")).fields[attribute.key] !== 165
  )
    throw Error("Unchanged rating display quantized an imported raw value");
  button(".studio-category-tabs button", "Shooting").click();
  await delay();
  if (document.querySelector('[aria-label="DNA Driving Layup"]'))
    throw Error("WPF category filtering did not apply");
  button(".studio-category-tabs button", "All").click();
  await delay();
  input('[aria-label="DNA Driving Layup"]', "85");
  await delay();
  document
    .querySelector<HTMLInputElement>('[aria-label="DNA Driving Layup"]')!
    .dispatchEvent(new FocusEvent("focusout", { bubbles: true }));
  await delay();
  await saveSeasonDraft();
  if ((await call<any>("player-dna")).fields[attribute.key] !== 179)
    throw Error("WPF rating input stored an incorrect packed value");
  await call("capture", { name: "player-web-attributes" });
  checks.push(
    "player-web-import-preserves-exact-raw-display",
    "player-web-rating-category-and-native-save",
  );
  button(".player-dna-editor nav button", "Contract").click();
  await until(
    () =>
      document.querySelector<HTMLInputElement>(
        `[aria-label="DNA ${salary.label}"]`,
      )?.value === "1.234567",
    "Salary is not displayed in millions",
  );
  input(`[aria-label="DNA ${salary.label}"]`, "2.25");
  await delay();
  document
    .querySelector<HTMLInputElement>(`[aria-label="DNA ${salary.label}"]`)!
    .dispatchEvent(new FocusEvent("focusout", { bubbles: true }));
  await delay();
  await saveSeasonDraft();
  if ((await call<any>("player-dna")).fields[salary.key] !== 2250000)
    throw Error("Salary conversion did not save exact dollars");
  const beforeInvalidSalary = JSON.stringify(await call("player-dna"));
  input('[aria-label="Player DNA name"]', "Pending invalid salary");
  input(`[aria-label="DNA ${salary.label}"]`, "0.0000001");
  await delay();
  document
    .querySelector<HTMLInputElement>(`[aria-label="DNA ${salary.label}"]`)!
    .dispatchEvent(new FocusEvent("focusout", { bubbles: true }));
  await delay();
  button(".player-dna-editor .dna-save button", "Save local draft").click();
  await delay();
  if (
    JSON.stringify(await call("player-dna")) !== beforeInvalidSalary ||
    !document.querySelector<HTMLInputElement>(
      `[aria-label="DNA ${salary.label}"]`,
    )!.validity.customError
  )
    throw Error("Invalid displayed salary was silently bypassed on Save");
  document
    .querySelector<HTMLInputElement>(`[aria-label="DNA ${salary.label}"]`)!
    .dispatchEvent(
      new KeyboardEvent("keydown", { key: "Escape", bubbles: true }),
    );
  await delay();
  button(".player-dna-editor .dna-save button", "Reset local edits").click();
  await delay();
  checks.push("player-web-salary-conversion-and-invalid-input-save-block");
  button(".player-dna-editor nav button", "Appearance").click();
  await until(
    () => !!document.querySelector('[aria-label="DNA Height feet"]'),
    "Imperial measurement controls missing",
  );
  document
    .querySelector<HTMLInputElement>('[aria-label="DNA Height inches"]')!
    .dispatchEvent(new FocusEvent("focusout", { bubbles: true }));
  document
    .querySelector<HTMLInputElement>('[aria-label="DNA Wingspan inches"]')!
    .dispatchEvent(new FocusEvent("focusout", { bubbles: true }));
  await delay();
  if (
    !button(".player-dna-editor .dna-save button", "Save local draft").disabled
  )
    throw Error("Visiting imperial fields rounded original centimetres");
  input('[aria-label="DNA Height feet"]', "6");
  input('[aria-label="DNA Height inches"]', "3");
  await delay();
  document
    .querySelector<HTMLInputElement>('[aria-label="DNA Height inches"]')!
    .dispatchEvent(new FocusEvent("focusout", { bubbles: true }));
  await delay();
  input('[aria-label="DNA Wingspan inches"]', "74");
  await delay();
  document
    .querySelector<HTMLInputElement>('[aria-label="DNA Wingspan inches"]')!
    .dispatchEvent(new FocusEvent("focusout", { bubbles: true }));
  await delay();
  await saveSeasonDraft();
  const measurements = await call<any>("player-dna");
  if (
    measurements.info.heightCm !== 190.5 ||
    measurements.info.wingspanCm !== 187.96
  )
    throw Error("Imperial edits did not save WPF-equivalent centimetres");
  checks.push("player-web-imperial-exact-roundtrip-and-batched-height");
  const beforeCatalog = await call<any>("player-dna");
  button(".player-dna-editor button", "Import from NBA 2K23").click();
  await until(
    () =>
      document.querySelectorAll(".player-catalog-results button:not(:disabled)")
        .length === 40,
    "Bundled player import popup did not populate the first page",
  );
  if (
    !document.querySelector<HTMLDialogElement>(".player-catalog-dialog")?.open
  )
    throw Error("Import catalog is not a modal web dialog");
  const firstImportRow = document.querySelector(
    ".player-catalog-results button",
  )!.textContent;
  button(".player-catalog-dialog footer button", "Next").click();
  await until(
    () =>
      document.querySelectorAll(".player-catalog-results button:not(:disabled)")
        .length === 40 &&
      document.querySelector(".player-catalog-results button")!.textContent !==
        firstImportRow,
    "Import catalog did not page beyond its first 40 players",
  );
  input('[aria-label="Search import players"]', "no matches zzz");
  input('[aria-label="Search import players"]', "Stephen Curry");
  select('[aria-label="Import player category"]', "current");
  await until(
    () =>
      !![
        ...document.querySelectorAll<HTMLButtonElement>(
          ".player-catalog-results button:not(:disabled)",
        ),
      ].find((b) => b.querySelector("strong")?.textContent === "Stephen Curry"),
    "Import catalog search/category did not return Curry",
  );
  await call("capture", { name: "player-import-catalog" });
  function chooseImportCurry() {
    const row = [
      ...document.querySelectorAll<HTMLButtonElement>(
        ".player-catalog-results button:not(:disabled)",
      ),
    ].find((b) => b.querySelector("strong")?.textContent === "Stephen Curry");
    if (!row) throw Error("Import source row missing");
    row.click();
  }
  chooseImportCurry();
  await until(
    () => !!document.querySelector(".player-catalog-sections"),
    "Category review missing",
  );
  if (
    !button(".player-catalog-dialog button", "Stage selected categories")
      .disabled
  )
    throw Error("Empty import category selection was allowed");
  if (document.querySelectorAll(".player-catalog-sections input").length !== 6)
    throw Error(
      "Catalog exposed unverified mappings or lost supported categories",
    );
  button(".player-catalog-dialog button", "Select all supported").click();
  await delay();
  const availableCatalogBoxes = [
    ...document.querySelectorAll<HTMLInputElement>(
      ".player-catalog-sections input:not(:disabled)",
    ),
  ];
  if (
    !availableCatalogBoxes.length ||
    availableCatalogBoxes.some((i) => !i.checked)
  )
    throw Error("Select all did not respect available categories");
  button(".player-catalog-dialog button", "Clear categories").click();
  await delay();
  if (
    document.querySelectorAll(".player-catalog-sections input:checked").length
  )
    throw Error("Clear categories left a hidden selection");
  document
    .querySelector<HTMLDialogElement>(".player-catalog-dialog")!
    .dispatchEvent(new Event("cancel", { cancelable: true }));
  await until(
    () =>
      !document.querySelector(".player-catalog-sections") &&
      !!document.querySelector(".player-catalog-results button:not(:disabled)"),
    "Escape did not return to the source player search",
  );
  if (
    document.querySelector<HTMLInputElement>(
      '[aria-label="Search import players"]',
    )!.value !== "Stephen Curry"
  )
    throw Error("Review-back lost the current search");
  chooseImportCurry();
  await until(
    () => !!document.querySelector(".player-catalog-sections"),
    "Repeat source review failed",
  );
  document
    .querySelector<HTMLInputElement>(
      '[aria-label="Import category Ratings & potential"]',
    )!
    .click();
  await delay();
  await call("capture", { name: "player-import-categories" });
  const expectedSource = (
    await call<any>("players", { query: "Stephen Curry", category: "current" })
  ).items[0];
  const expectedCatalog = await call<any>("catalog-player-dna", {
    id: expectedSource.id,
  });
  const applyCatalog = button(
    ".player-catalog-dialog button",
    "Stage selected categories",
  );
  applyCatalog.click();
  applyCatalog.click();
  await until(
    () => !document.querySelector(".player-catalog-dialog"),
    "Category stage did not return to player",
  );
  if (
    JSON.stringify(await call("player-dna")) !== JSON.stringify(beforeCatalog)
  )
    throw Error("Category staging saved before explicit local Save");
  await saveSeasonDraft();
  const afterCatalog = await call<any>("player-dna");
  if (
    afterCatalog.name !== beforeCatalog.name ||
    afterCatalog.info.birthDate !== beforeCatalog.info.birthDate ||
    afterCatalog.info.yearsPro !== beforeCatalog.info.yearsPro ||
    afterCatalog.info.heightCm !== beforeCatalog.info.heightCm ||
    afterCatalog.fields[salary.key] !== beforeCatalog.fields[salary.key] ||
    afterCatalog.info.potential !== expectedCatalog.info.potential
  )
    throw Error(
      "Category import changed destination identity or unselected fields",
    );
  for (const f of webSchema.fields.filter(
    (f: any) => f.group === "attributes",
  )) {
    if (
      expectedCatalog.fields[f.key] !== undefined &&
      afterCatalog.fields[f.key] !== expectedCatalog.fields[f.key]
    )
      throw Error(`Category import lost exact source ${f.key}`);
  }
  checks.push(
    "player-import-popup-native-search-paging-and-categories",
    "player-import-review-empty-all-clear-and-back",
    "player-import-selected-categories-stage-save-preserves-other-fields",
  );
  button(".dna-layout nav button", "Play types").click();
  await until(
    () => document.querySelectorAll(".dna-field-grid select").length >= 4,
    "Play type choices did not render",
  );
  await call("capture", { name: "player-dna-playtypes" });
  button(".player-dna-editor button", "Export .2kplayer").click();
  await until(
    () =>
      document
        .querySelector("#toast")
        ?.textContent?.includes("Player DNA exported:") ?? false,
    "DNA export failed",
  );
  checks.push("player-dna-playtypes-ui", "player-dna-native-export-ui");
  button(".player-dna-editor nav button", "Season history").click();
  await delay();
  button(".player-dna-editor button", "Import stats").click();
  await until(
    () => !!document.querySelector(".stats-source-modal"),
    "Stats source chooser missing",
  );
  const fromCatalog = [
    ...document.querySelectorAll<HTMLButtonElement>(
      ".stats-source-modal button",
    ),
  ].find((b) => b.textContent?.startsWith("Import from NBA 2K23"));
  if (!fromCatalog) throw Error("Bundled stats source choice missing");
  fromCatalog.click();
  await until(
    () =>
      document.querySelectorAll(".season-import-modal .players-list button")
        .length > 0,
    "Stats player search did not populate",
  );
  const statsPlayer = [
    ...document.querySelectorAll<HTMLButtonElement>(
      ".season-import-modal .players-list button",
    ),
  ].find((b) => b.querySelector("strong")?.textContent === "Stephen Curry");
  if (!statsPlayer) throw Error("Stats source player missing");
  statsPlayer.click();
  await until(
    () => !!document.querySelector('[aria-label="Choose stats season 2019"]'),
    "Stats season review missing",
  );
  const normalTeams = await call<any[]>("season-teams");
  const teamSelect = document.querySelector<HTMLSelectElement>(
    '[aria-label="Import 2019 primary team"]',
  )!;
  if (
    teamSelect.options.length !== normalTeams.length + 1 ||
    teamSelect.value !== "29"
  )
    throw Error("Stats team choices are not normal-only or autofill failed");
  document
    .querySelector<HTMLInputElement>('[aria-label="Choose stats season 2019"]')!
    .click();
  await delay();
  document
    .querySelector<HTMLInputElement>('[aria-label="Choose stats season 2018"]')!
    .click();
  select('[aria-label="Import 2019 primary team"]', "0");
  document
    .querySelector<HTMLInputElement>('[aria-label="Import season awards"]')!
    .click();
  await call("capture", { name: "player-season-import" });
  button(".season-import-modal button", "Import selected seasons").click();
  await until(
    () =>
      !document.querySelector(".season-import-modal") &&
      document.querySelectorAll(".season-history-editor tbody tr").length === 2,
    "Stats did not return to populated season editor",
  );
  async function saveSeasonDraft() {
    button(".player-dna-editor .dna-save button", "Save local draft").click();
    await until(
      () =>
        document
          .querySelector(".player-dna-editor header .badge")
          ?.textContent?.trim() === "Local draft",
      "Season DNA save failed",
    );
  }
  await saveSeasonDraft();
  let seasonDNA = await call<any>("player-dna");
  if (
    seasonDNA.version !== 2 ||
    seasonDNA.seasonHistory.length !== 2 ||
    seasonDNA.seasonHistory[0].team !== "76ers" ||
    seasonDNA.info.yearsPro !== 7
  )
    throw Error(
      "Catalog seasons did not reach actual Player DNA or changed other sections",
    );
  checks.push(
    "stats-popup-search-season-review-return",
    "stats-normal-teams-autofill-and-override",
    "stats-native-dna-merge-preserves-other-fields",
  );
  button(".season-history-editor button", "2018–19").click();
  await delay();
  input('[aria-label="Season Points"]', "2000");
  select('[aria-label="Season award Most Valuable Player"]', "1");
  select('[aria-label="Season award All-Star"]', "2");
  await delay();
  await saveSeasonDraft();
  seasonDNA = await call<any>("player-dna");
  if (
    seasonDNA.seasonHistory[0].totals.points !== 2000 ||
    seasonDNA.seasonHistory[0].awards.mvp !== 1 ||
    seasonDNA.seasonHistory[0].awards.allStar !== 2
  )
    throw Error("Season totals/awards edits were not persisted");
  checks.push("season-totals-and-awards-native-save");
  const ppgInput = document.querySelector<HTMLInputElement>(
    '[aria-label="Season PPG"]',
  )!;
  // Visiting a one-decimal display must not round its exact stored total.
  ppgInput.focus();
  ppgInput.blur();
  ppgInput.dispatchEvent(new FocusEvent("focusout", { bubbles: true }));
  await delay();
  if (
    !button(".player-dna-editor .dna-save button", "Save local draft").disabled
  )
    throw Error("An untouched derived display rounded exact season totals");
  ppgInput.focus();
  input('[aria-label="Season PPG"]', "41.5");
  await delay();
  ppgInput.dispatchEvent(
    new KeyboardEvent("keydown", { key: "Escape", bubbles: true }),
  );
  ppgInput.dispatchEvent(new FocusEvent("focusout", { bubbles: true }));
  await delay();
  if (
    !button(".player-dna-editor .dna-save button", "Save local draft")
      .disabled ||
    ppgInput.value !==
      (
        seasonDNA.seasonHistory[0].totals.points /
        seasonDNA.seasonHistory[0].totals.gp
      ).toFixed(1)
  )
    throw Error("Escape committed a cancelled season average");
  checks.push("season-derived-unchanged-and-escape-preserve-exact-totals");
  const expectedAverage = await call<any>("season-math-change", {
    totals: seasonDNA.seasonHistory[0].totals,
    key: "ppg",
    value: 31.2,
  });
  ppgInput.focus();
  input('[aria-label="Season PPG"]', "31.2");
  await delay();
  ppgInput.blur();
  ppgInput.dispatchEvent(new FocusEvent("focusout", { bubbles: true }));
  await delay();
  await until(
    () =>
      !button(".player-dna-editor .dna-save button", "Save local draft")
        .disabled,
    "Changed PPG did not stage a local draft",
  );
  await saveSeasonDraft();
  seasonDNA = await call<any>("player-dna");
  if (
    JSON.stringify(seasonDNA.seasonHistory[0].totals) !==
    JSON.stringify(expectedAverage.totals)
  )
    throw Error(
      `Season PPG UI arithmetic differs from Rust/WPF: actual=${JSON.stringify(seasonDNA.seasonHistory[0].totals)} expected=${JSON.stringify(expectedAverage.totals)}`,
    );
  const expectedPercent = await call<any>("season-math-change", {
    totals: seasonDNA.seasonHistory[0].totals,
    key: "fgPercent",
    value: 55,
  });
  const fgInput = document.querySelector<HTMLInputElement>(
    '[aria-label="Season FG%"]',
  )!;
  fgInput.focus();
  input('[aria-label="Season FG%"]', "55");
  await delay();
  fgInput.blur();
  fgInput.dispatchEvent(new FocusEvent("focusout", { bubbles: true }));
  await delay();
  await until(
    () =>
      !button(".player-dna-editor .dna-save button", "Save local draft")
        .disabled,
    "Changed FG percentage did not stage a local draft",
  );
  await saveSeasonDraft();
  seasonDNA = await call<any>("player-dna");
  if (
    JSON.stringify(seasonDNA.seasonHistory[0].totals) !==
    JSON.stringify(expectedPercent.totals)
  )
    throw Error(
      "Season percentage did not preserve attempts and synchronize scoring",
    );
  checks.push("season-derived-ppg-and-percent-native-save");
  if (
    document.querySelectorAll(".season-dense-grid th").length !== 21 ||
    parseFloat(
      getComputedStyle(document.querySelector(".season-dense-grid td")!)
        .fontSize,
    ) > 11
  )
    throw Error("Compact season grid lost columns or small text");
  checks.push("season-compact-21-column-grid");
  input('[aria-label="Season Games played"]', "1");
  await delay();
  button(".player-dna-editor .dna-save button", "Save local draft").click();
  await until(
    () =>
      document
        .querySelector('.player-dna-editor [role="alert"]')
        ?.textContent?.includes("Season counts exceed games played") ?? false,
    "Invalid season counts were not rejected",
  );
  if (JSON.stringify(await call("player-dna")) !== JSON.stringify(seasonDNA))
    throw Error("Rejected season save replaced previous data");
  button(".player-dna-editor .dna-save button", "Reset local edits").click();
  await until(
    () =>
      document.querySelector<HTMLInputElement>(
        '[aria-label="Season Games played"]',
      )?.value !== "1",
    "Season reset failed",
  );
  button(".season-history-editor button", "Remove selected season").click();
  await until(
    () =>
      document.querySelectorAll(".season-history-editor tbody tr").length === 1,
    "Season removal failed",
  );
  button(".player-dna-editor .dna-save button", "Reset local edits").click();
  await until(
    () =>
      document.querySelectorAll(".season-history-editor tbody tr").length === 2,
    "Season removal reset failed",
  );
  checks.push(
    "invalid-season-save-preserves-file",
    "season-removal-reset-without-game-refresh",
  );
  await call("capture", { name: "player-season-history" });
  button(".player-dna-editor button", "Export .2kplayer").click();
  await until(
    () =>
      document
        .querySelector("#toast")
        ?.textContent?.includes("Player DNA exported:") ?? false,
    "Version 2 season export failed",
  );
  checks.push("season-awards-player-dna-v2-export");
  // Native URL fixture follows the actual source chooser and local staging path.
  button(".player-dna-editor button", "Import stats").click();
  await until(
    () => !!document.querySelector(".stats-source-modal"),
    "Stats chooser did not reopen",
  );
  const fromReference = [
    ...document.querySelectorAll<HTMLButtonElement>(
      ".stats-source-modal button",
    ),
  ].find((b) => b.textContent?.startsWith("Paste Basketball-Reference URL"));
  if (!fromReference) throw Error("Reference source choice missing");
  fromReference.click();
  await until(
    () => !!document.querySelector(".reference-import-modal"),
    "Reference modal missing",
  );
  input(
    '[aria-label="Basketball-Reference NBA player URL"]',
    "https://www.basketball-reference.com/players/m/maxeyty01.html",
  );
  await delay();
  button(".reference-import-modal button", "Load seasons").click();
  await until(
    () =>
      !!document.querySelector('[aria-label="Choose reference season 2026"]'),
    "Reference offline source did not load",
  );
  const refTeam = document.querySelector<HTMLSelectElement>(
    '[aria-label="Reference 2026 destination team"]',
  )!;
  if (
    refTeam.options.length !== normalTeams.length + 1 ||
    refTeam.value !== "0"
  )
    throw Error("Reference normal teams or exact autofill failed");
  document
    .querySelector<HTMLInputElement>(
      '[aria-label="Choose reference season 2026"]',
    )!
    .click();
  await delay();
  await call("capture", { name: "basketball-reference-import" });
  button(".reference-import-modal button", "Import selected stats").click();
  await until(
    () =>
      !document.querySelector(".reference-import-modal") &&
      document.querySelectorAll(".season-history-editor tbody tr").length === 3,
    "Reference stats did not return to populated season editor",
  );
  await saveSeasonDraft();
  const referenceDNA = await call<any>("player-dna");
  if (
    referenceDNA.seasonHistory[0].year !== 2026 ||
    referenceDNA.seasonHistory[0].totals.points !== 1980 ||
    referenceDNA.seasonHistory[1].awards.mvp !== 1 ||
    referenceDNA.info.yearsPro !== 7
  )
    throw Error(
      "Reference staging failed or changed prior awards/player fields",
    );
  let networkRefused = false;
  try {
    await call("reference-stats-load", {
      url: "https://www.basketball-reference.com/players/m/maxeyty01.html",
      acknowledgement: "fetch-public-player-page",
    });
  } catch (e) {
    networkRefused = String(e).includes("network disabled in smoke mode");
  }
  if (!networkRefused)
    throw Error("Stats importer attempted production network in smoke mode");
  checks.push(
    "stats-two-source-choice",
    "reference-url-fixture-native-parser-and-review",
    "reference-normal-teams-autofill",
    "reference-staging-preserves-existing-awards-and-player",
    "reference-no-network-in-smoke",
  );
  button(".player-dna-editor nav button", "Player info").click();
  await delay();
  input('[aria-label="DNA Years pro"]', "9");
  await delay();
  button(".player-dna-editor button", "New local player").click();
  await until(
    () => !!document.querySelector(".player-dna-editor .modal"),
    "Player discard confirmation missing",
  );
  button(".player-dna-editor .modal button", "Keep editing").click();
  a.page("Team Editor");
  await openLocalEditorTools("teams", true);
  checks.push("browser-first-team-local-tools-separated");
  await until(
    () =>
      [
        ...document.querySelectorAll<HTMLButtonElement>(
          ".team-dna-editor button",
        ),
      ].some((b) => b.textContent?.trim() === "New local team" && !b.disabled),
    "Team schema did not enable editor",
  );
  button(".team-dna-editor button", "New local team").click();
  await until(
    () => !!document.querySelector('[aria-label="Team DNA name"]'),
    "Team info controls missing",
  );
  input('[aria-label="Team DNA name"]', "QA destination");
  input('[aria-label="Team Team name"]', "Destination team");
  input('[aria-label="Team City"]', "Original city");
  button(".team-dna-editor nav button", "Arena / floor").click();
  await delay();
  button(".team-dna-editor button", "Include arena / floor").click();
  await delay();
  input('[aria-label="Arena name"]', "QA venue");
  input('[aria-label="Arena asset"]', "arena_test");
  input('[aria-label="Floor asset"]', "floor_test");
  button(".team-dna-editor nav button", "Staff & coaches").click();
  await delay();
  button(".team-dna-editor button", "Include staff slot").click();
  await delay();
  const teamSchema = await call<{ staff: { key: string; label: string }[] }>(
    "team-dna-schema",
  );
  const staffLabel = (key: string) =>
    `[aria-label="Staff ${teamSchema.staff.find((f) => f.key === key)!.label}"]`;
  input(staffLabel("faceId"), "321");
  select(staffLabel("preferredSystemId"), "15");
  input('[aria-label="Filter team DNA fields"]', "HelpDefense");
  await delay();
  input(staffLabel("HelpDefense"), "77");
  await call("capture", { name: "team-staff" });
  button(".team-dna-editor nav button", "Rotations").click();
  await delay();
  button(".team-dna-editor button", "Create 48 segments").click();
  await delay();
  if (document.querySelectorAll(".team-rotation-grid tbody tr").length !== 48)
    throw Error("Rotation grid is incomplete");
  input('[aria-label="Rotation minute 1 player 1"]', "QA player");
  button(".team-dna-editor nav button", "Uniforms").click();
  await delay();
  button(".team-dna-editor button", "Add uniform row").click();
  await delay();
  select('[aria-label="Uniform match Designation"]', "1");
  input('[aria-label="Uniform Primary"]', "4294967295");
  await delay();
  button(".team-dna-editor .dna-save button", "Save team draft").click();
  await until(
    () =>
      document
        .querySelector("#toast")
        ?.textContent?.includes("Team DNA draft saved") ?? false,
    "Team draft did not save",
  );
  let team = await call<any>("team-dna");
  if (
    team.arena.floorFile !== "floor_test" ||
    team.arena.arenaFile !== "arena_test" ||
    team.staff[0].values.faceId !== 321 ||
    team.staff[0].values.preferredSystemId !== 15 ||
    team.staff[0].values.HelpDefense !== 77 ||
    team.rotations.length !== 48 ||
    team.uniforms[0].values.primaryColor !== 4294967295
  )
    throw Error("Team draft native data mismatch");
  checks.push(
    "team-native-arena-floor-separation",
    "team-native-staff-controls",
    "team-native-48-rotations",
    "team-native-uniform-argb-save",
  );
  input('[aria-label="Uniform Primary"]', "2166572391");
  await delay(); // 0x81234567
  input('[aria-label="Uniform Primary color"]', "#abcdef");
  await delay();
  button(".team-dna-editor .dna-save button", "Save team draft").click();
  await delay();
  team = await call<any>("team-dna");
  if (team.uniforms[0].values.primaryColor !== 0x81abcdef)
    throw Error("Uniform RGB picker discarded original alpha");
  checks.push("team-web-rgb-picker-preserves-alpha-native-save");
  await call("smoke-team-import-file", {
    format: "2kteam",
    version: 1,
    game: "NBA2K27",
    name: "Source identity",
    info: { name: "Imported team", cityName: "Imported city" },
    arena: null,
    staff: [{ slot: 0, values: { faceId: 999 } }],
  });
  button(".team-dna-editor button", "Import .2kteam").click();
  await until(
    () => !!document.querySelector(".team-dna-editor .dna-import-review"),
    "Team import review missing",
  );
  const boxes = [
    ...document.querySelectorAll<HTMLInputElement>(
      '.team-dna-editor .dna-import-review input[type="checkbox"]',
    ),
  ];
  for (let i = 1; i < boxes.length; i++) boxes[i].click();
  await delay();
  button(
    ".team-dna-editor .dna-import-review button",
    "Stage team categories",
  ).click();
  await until(
    () => !document.querySelector(".team-dna-editor .dna-import-review"),
    "Team staging did not close review",
  );
  button(".team-dna-editor .dna-save button", "Save team draft").click();
  await until(
    () =>
      document
        .querySelector("#toast")
        ?.textContent?.includes("Team DNA draft saved") ?? false,
    "Staged Team DNA did not save",
  );
  team = await call<any>("team-dna");
  if (
    team.name !== "QA destination" ||
    team.info.cityName !== "Imported city" ||
    team.staff[0].values.faceId !== 321 ||
    team.arena.floorFile !== "floor_test"
  )
    throw Error(
      "Team category staging changed unselected data or destination identity",
    );
  checks.push("team-file-import-category-stage-preserves-destination");
  button(".team-dna-editor button", "Export .2kteam").click();
  await until(
    () =>
      document
        .querySelector("#toast")
        ?.textContent?.includes("Team DNA exported:") ?? false,
    "Team DNA export failed",
  );
  checks.push("team-native-export-ui");
  button(".team-dna-editor nav button", "Team info").click();
  await delay();
  input('[aria-label="Team DNA name"]', "Pending team");
  await delay();
  button(".team-dna-editor button", "New local team").click();
  await until(
    () => !!document.querySelector(".team-dna-editor .modal"),
    "Team discard confirmation missing",
  );
  button(".team-dna-editor .modal button", "Keep editing").click();
  a.page("Roster Editor");
  await openLocalEditorTools("players");
  await until(
    () =>
      document.querySelector<HTMLInputElement>('[aria-label="DNA Years pro"]')
        ?.value === "9",
    "Unsaved player draft lost on navigation",
  );
  button(".player-dna-editor .dna-save button", "Reset local edits").click();
  await delay();
  a.page("Team Editor");
  await openLocalEditorTools("teams");
  await until(
    () =>
      document.querySelector<HTMLInputElement>('[aria-label="Team DNA name"]')
        ?.value === "Pending team",
    "Unsaved team draft lost on navigation",
  );
  button(".team-dna-editor .dna-save button", "Reset team edits").click();
  await until(
    () =>
      document.querySelector<HTMLInputElement>('[aria-label="Team DNA name"]')
        ?.value === "QA destination",
    "Team reset did not restore saved draft",
  );
  await call("capture", { name: "team-dna-info" });
  checks.push(
    "player-team-unsaved-navigation-retention",
    "player-team-new-draft-discard-confirmation",
    "team-reset-without-live-refresh",
  );
  a.page("Unlocks");
  await until(
    () => !!document.querySelector(".unlocks-workspace"),
    "Unlocks did not render",
  );
  const unavailableUnlocks = document.querySelectorAll<HTMLInputElement>(
    ".unlock-choice.unavailable input",
  );
  // WNBA Blacktop and debug menus have no verified V2 route. WNBA Freestyle is
  // an experimental opt-in (mixed roster) that must default to off.
  const freestyleUnlock = document.querySelector<HTMLInputElement>(
    "#unlockWnbaFreestyle",
  );
  if (
    unavailableUnlocks.length !== 2 ||
    [...unavailableUnlocks].some((control) => !control.disabled) ||
    !freestyleUnlock ||
    freestyleUnlock.disabled ||
    freestyleUnlock.checked
  )
    throw Error("Unsupported unlock routes were not visibly guarded");
  await call("capture", { name: "unlocks" });
  checks.push("unsupported-unlock-routes-remain-disabled");
  a.page("Scaler");
  await until(
    () => !!document.querySelector(".scaler-layout"),
    "Scaler did not render",
  );
  await call("capture", { name: "scaler" });
  a.page("Settings");
  await until(
    () => !!document.querySelector(".settings-workspace"),
    "Settings did not render",
  );
  const settingsAdvanced =
    document.querySelector<HTMLDetailsElement>(".settings-advanced");
  if (!settingsAdvanced || settingsAdvanced.open)
    throw Error("Advanced settings were not collapsed by default");
  button(".settings-workspace button", "Export support bundle");
  button(".settings-workspace button", "Open support folder");
  if (
    !document
      .querySelector("#pageTitle")
      ?.textContent?.includes("Launcher settings")
  )
    throw Error("Settings shared header missing");
  await call("capture", { name: "settings" });
  checks.push("compact-settings-support-and-collapsed-advanced");
  await call("smoke-resize", { width: 1060, height: 700 });
  await until(
    () => innerWidth <= 1061,
    "Compact workflow window did not resize",
  );
  for (const page of [
    "Profiles",
    "Mod library",
    "Unlocks",
    "Scaler",
    "Settings",
  ]) {
    a.page(page);
    await delay();
    if (document.documentElement.scrollWidth > innerWidth + 1)
      throw Error(`${page} overflows the compact window`);
    const content = document.querySelector<HTMLElement>(".page")!;
    if (content.scrollWidth > content.clientWidth + 1)
      throw Error(`${page} content overflows the compact window`);
  }
  await call("smoke-resize", { width: 1440, height: 900 });
  await until(() => innerWidth >= 1439, "Workflow window did not restore");
  checks.push("restyled-workflows-fit-compact-window");
  a.page("Play session");
  await delay();
  return checks;
}
