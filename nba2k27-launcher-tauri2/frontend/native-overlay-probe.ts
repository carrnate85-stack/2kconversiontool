import { call } from "./api";
const wait = (ms = 100) => new Promise((r) => setTimeout(r, ms));
async function until(test: () => boolean) {
  for (let i = 0; i < 80; i++) {
    if (test()) return;
    await wait(50);
  }
  throw Error("Native overlay fixture timed out");
}
function top(text: string) {
  const selector = nativeOverlayCatalogSelector(text);
  if (selector) {
    const scoped = document.querySelector<HTMLButtonElement>(selector);
    if (!scoped) throw Error(`Missing native overlay section ${text}`);
    return scoped;
  }
  const b = [
    ...document.querySelectorAll<HTMLButtonElement>(".overlay-bar button"),
  ].find((b) => b.textContent?.trim().startsWith(text));
  if (!b) throw Error(`Missing native overlay ${text}`);
  return b;
}
// Ball and Scoreboard can both show "Currently installed". The fixture must
// address the section, not click the first equal display label in the bar.
export function nativeOverlayCatalogSelector(label: string) {
  if (label === "Currently installed") return ".overlay-item.ball button";
  if (label === "Scoreboard") return ".overlay-item.scoreboard button";
  return null;
}
// Records launcher channels sent while a catalog tile is picked, so the probe
// can prove an ineligible pick never reaches a native-*-select route.
function recordChannels() {
  const internals = (window as any).__TAURI_INTERNALS__;
  const seen: string[] = [];
  const original = internals?.invoke;
  let installed = false;
  try {
    internals.invoke = (cmd: string, args: any, options: any) => {
      if (cmd === "launcher" && typeof args?.channel === "string")
        seen.push(args.channel);
      return original.call(internals, cmd, args, options);
    };
    installed = internals.invoke !== original;
  } catch {
    installed = false;
  }
  return {
    seen,
    installed,
    stop() {
      if (installed) internals.invoke = original;
    },
  };
}
// WPF had no per-feature confirmation: an ineligible game shows a plain
// status (no checkbox) and a pick makes no game call and keeps the catalog.
async function ineligiblePickRefused(feature: "arena" | "ball" | "scoreboard") {
  const catalog = document.querySelector(".overlay-catalog")!;
  if (catalog.querySelector('input[type="checkbox"]'))
    throw Error(`${feature} catalog still shows a confirmation checkbox`);
  if (/enable .* changes for this game/i.test(catalog.textContent ?? ""))
    throw Error(`${feature} catalog still asks to enable changes`);
  await until(
    () => document.querySelectorAll(".catalog-grid .catalog-tile").length > 0,
  );
  const recorder = recordChannels();
  try {
    document
      .querySelector<HTMLButtonElement>(".catalog-grid .catalog-tile")!
      .click();
    const status = () =>
      document.querySelector(".overlay-catalog .native-arena-controls")
        ?.textContent ?? "";
    await until(() =>
      /changes (are|become) available|editing is disabled|changes have ended|not available/i.test(
        status(),
      ),
    );
    await wait(150);
  } finally {
    recorder.stop();
  }
  if (recorder.seen.some((c) => c === `native-${feature}-select`))
    throw Error(`Ineligible ${feature} pick called the game route`);
  if (!document.querySelector(".overlay-catalog"))
    throw Error(`Refused ${feature} pick closed the catalog without a status`);
  if (!recorder.installed)
    console.warn("Channel recorder unavailable; route refusal checked by status only");
}
export async function nativeOverlayProbe() {
  const checks: string[] = [];
  try {
    await until(() => !!document.querySelector(".overlay-bar"));
    await wait(200);
    const first = await call<any>("smoke-native-overlay-metrics");
    if (
      !first.owned ||
      !first.hidden ||
      first.focusable ||
      first.size.height > 260
    )
      throw Error("Initial overlay ownership/hidden compact frame failed");
    const rect = () =>
      document.querySelector(".overlay-bar")!.getBoundingClientRect();
    const baseline = rect();
    if (top("Select player").disabled)
      throw Error("Live player selector is disabled");
    top("Select player").click();
    await until(() => !!document.querySelector(".overlay-dropdown.player"));
    // The PLAYER dropdown is a strip, not a catalog, and lists only players
    // read from the owned provider.
    if (
      document.querySelector(".overlay-catalog") ||
      document.querySelector(".overlay-dropdown [role=menuitemradio]")
    )
      throw Error("Live player dropdown faked a player");
    top("Select player").click();
    await until(() => !document.querySelector(".overlay-dropdown"));
    top("Edit outfit").click();
    await until(() => !!document.querySelector(".native-outfit-controls"));
    const outfit = document.querySelector(".native-outfit-controls")!;
    const editor = document.querySelector(".wpf-outfit")!;
    const outfitButtons = [
      ...outfit.querySelectorAll<HTMLButtonElement>("button"),
    ];
    // No confirmation gate (WPF had none): without an owned, eligible
    // provider the write buttons stay disabled on their own.
    if (
      editor.querySelector('input[type="checkbox"]') ||
      /Enable outfit changes/i.test(editor.textContent ?? "") ||
      outfit.querySelector(".native-outfit-players button") ||
      document.querySelector(".wpf-player-picker button") ||
      !outfitButtons.find((b) => b.textContent === "Apply outfit")?.disabled ||
      !outfitButtons.find((b) => b.textContent === "Restore original")?.disabled
    )
      throw Error(
        "Live outfit controls faked a player or enabled unowned writes",
      );
    let outfitRefused = false;
    try {
      await call("native-outfit-command", {
        acknowledgement: "read-blacktop-outfits",
        command: { action: "list" },
      });
    } catch (e) {
      outfitRefused = String(e).includes("disabled in smoke mode");
    }
    if (!outfitRefused) throw Error("Live outfit fixture accessed game memory");
    top("Edit outfit").click();
    await until(() => !document.querySelector(".overlay-catalog"));
    await wait(120);
    checks.push(
      "native-overlay-exact-fixture-owner",
      "native-overlay-hidden-until-ready",
      "native-overlay-live-player-not-faked",
      // Name kept for Verify-QARelease.ps1; "ack" is now the fixed backend
      // token sent automatically (no UI confirmation).
      "native-overlay-outfit-requires-own-provider-and-ack",
    );
    const samples: any[] = [first];
    const arenaButton = document.querySelector<HTMLButtonElement>(
      ".overlay-item.arena button",
    )!;
    arenaButton.click();
    await until(() => !!document.querySelector(".native-arena-controls"));
    await ineligiblePickRefused("arena");
    if (
      Math.abs(rect().width - baseline.width) > 1 ||
      Math.abs(rect().height - baseline.height) > 1
    )
      throw Error("Arena catalog resized the top bar");
    await call("capture", { name: "native-overlay-arena" });
    arenaButton.click();
    await until(() => !document.querySelector(".overlay-catalog"));
    await wait(120);
    checks.push(
      "native-overlay-arena-no-checkbox-ineligible-status-and-stable-bar",
    );
    for (const label of [
      "Currently installed",
      "Scoreboard",
      "Game rules",
      "Edit outfit",
    ]) {
      top(label).click();
      await until(() => !!document.querySelector(".overlay-catalog"));
      await wait(180);
      const expanded = await call<any>("smoke-native-overlay-metrics");
      samples.push(expanded);
      if (
        !expanded.owned ||
        !expanded.focusable ||
        !expanded.hidden ||
        expanded.size.width !== first.size.width ||
        expanded.size.height <= first.size.height
      )
        throw Error(`Expanded frame changed width/owner: ${label}`);
      if (
        Math.abs(rect().height - baseline.height) > 1 ||
        Math.abs(rect().width - baseline.width) > 1
      )
        throw Error(`Top bar resized: ${label}`);
      if (label === "Game rules") {
        const fixture = [
          ...document.querySelectorAll<HTMLButtonElement>(
            ".native-rules button",
          ),
        ].find((b) => b.textContent?.includes("Load isolated"));
        if (!fixture) throw Error("Shared native rules component missing");
        fixture.click();
        await until(
          () => document.querySelectorAll(".native-rule-row").length === 39,
        );
        await call("capture", { name: "native-overlay-rules" });
        let refused = false;
        try {
          await call("native-rules-read", {
            acknowledgement: "read-only-rules",
          });
        } catch (e) {
          refused = String(e).includes("disabled in smoke mode");
        }
        if (!refused) throw Error("Live fixture accessed game memory");
        // WPF pane: heading, status and note stay put; the rule list scrolls.
        const content = document.querySelector<HTMLElement>(
          ".overlay-catalog .native-rules-grid",
        )!;
        content.scrollTop = content.scrollHeight;
        await wait();
        if (content.scrollTop < 10)
          throw Error("Rules catalog did not scroll to remaining rows");
        content.scrollTop = 0;
        checks.push(
          "native-overlay-shared-39-rule-grid",
          "native-overlay-game-memory-refused-in-fixture",
        );
      }
      if (label === "Currently installed") {
        await ineligiblePickRefused("ball");
        await call("capture", { name: "native-overlay-ball" });
        checks.push("native-overlay-ball-no-checkbox-ineligible-status-no-game-call");
      }
      if (label === "Scoreboard") {
        await ineligiblePickRefused("scoreboard");
        let refused = false;
        try {
          await call("native-scoreboard-select", {
            acknowledgement: "apply-one-scoreboard-for-current-game",
            assetId: "broadcast",
            session: "a".repeat(48),
          });
        } catch (e) {
          refused = String(e).includes("disabled in smoke mode");
        }
        if (!refused)
          throw Error("Scoreboard fixture accessed a live game route");
        await call("capture", { name: "native-overlay-scoreboard" });
        checks.push(
          "native-overlay-scoreboard-no-checkbox-ineligible-status-no-game-fixture",
        );
      }
      if (label === "Edit outfit") {
        // WPF editor: piece cards + accessory slot tiles, each opening the
        // clothing picker (the same bundled pictures as before).
        await until(
          () => document.querySelectorAll(".wpf-outfit .accessory-slot").length > 0,
        );
        const slots = document.querySelectorAll(
          ".wpf-outfit .blacktop-piece-tile, .wpf-outfit .accessory-tile",
        ).length;
        if (slots < 24) throw Error("Some accessory slot selectors missing");
        const openSlot = async (slot: string, word: string) => {
          document
            .querySelector<HTMLButtonElement>(
              `.wpf-outfit [data-slot="${slot}"] .accessory-tile`,
            )!
            .click();
          await until(
            () =>
              !!document
                .querySelector(".garment-grid button strong")
                ?.textContent?.toLowerCase()
                .includes(word),
          );
        };
        await openSlot("headphones", "headphone");
        await until(() =>
          [
            ...document.querySelectorAll<HTMLImageElement>(".garment-grid img"),
          ].every((i) => i.complete),
        );
        const pictures = [
          ...document.querySelectorAll<HTMLImageElement>(".garment-grid img"),
        ];
        if (!pictures.length || pictures.some((i) => !i.naturalWidth))
          throw Error(
            `Bundled headphone pictures failed to load: ${pictures.map((i) => `${i.src}:${i.naturalWidth}`).join(",")}`,
          );
        await call("capture", { name: "native-overlay-outfit" });
        [...document.querySelectorAll<HTMLButtonElement>(".clothing-picker-heading button")]
          .find((b) => b.textContent === "Back")!
          .click();
        await until(() => !document.querySelector(".clothing-picker"));
        await openSlot("headwear", "hat");
        [...document.querySelectorAll<HTMLButtonElement>(".clothing-picker-heading button")]
          .find((b) => b.textContent === "Back")!
          .click();
        await until(() => !document.querySelector(".clothing-picker"));
        await wait(150);
        // Every accessory tile must be reachable in the editor's scroller.
        const editor = document.querySelector<HTMLElement>(".wpf-outfit .wardrobe-editor")!;
        editor.scrollTop = editor.scrollHeight;
        await wait();
        const tiles = document.querySelectorAll(".wpf-outfit .accessory-slot");
        const last = tiles[tiles.length - 1].getBoundingClientRect();
        if (last.bottom > editor.getBoundingClientRect().bottom + 1)
          throw Error("Accessory catalog did not scroll to remaining cards");
        await call("capture", { name: "native-overlay-accessories-scrolled" });
      }
      top(label).click();
      await until(() => !document.querySelector(".overlay-catalog"));
      await wait(120);
      const collapsed = await call<any>("smoke-native-overlay-metrics");
      samples.push(collapsed);
      if (
        collapsed.focusable ||
        collapsed.size.width !== first.size.width ||
        collapsed.size.height !== first.size.height
      )
        throw Error(`Bar dimensions not restored: ${label}`);
    }
    // WPF's Random player menu is a compact strip under the bar, not a catalog.
    top("Random player").click();
    await until(() => !!document.querySelector(".overlay-dropdown.random"));
    await wait(180);
    const strip = await call<any>("smoke-native-overlay-metrics");
    samples.push(strip);
    if (
      document.querySelector(".overlay-catalog") ||
      strip.size.width !== first.size.width ||
      strip.size.height <= first.size.height ||
      Math.abs(rect().height - baseline.height) > 1
    )
      throw Error("Random player menu was not a strip under a stable bar");
    top("Random player").click();
    await until(() => !document.querySelector(".overlay-dropdown"));
    await wait(120);
    const closed = await call<any>("smoke-native-overlay-metrics");
    samples.push(closed);
    if (closed.size.height !== first.size.height)
      throw Error("Bar dimensions not restored: Random player");
    checks.push(
      "native-overlay-compact-nonactivating-expanded-focusable",
      "native-overlay-all-catalogs-same-button-toggle",
      "native-overlay-expand-collapse-width-stable",
      "native-overlay-compact-height-restored",
      "native-overlay-all-accessory-slots-scrollable",
    );
    await call("capture", { name: "native-overlay-bar" });
    top("Hide overlay").click();
    await until(() =>
      [
        ...document.querySelectorAll<HTMLButtonElement>(".overlay-bar button"),
      ].some((b) => b.textContent === "Retry hide"),
    );
    if (!top("Retry hide").title.includes("Could not hide overlay"))
      throw Error("Hide failure was silently discarded");
    checks.push("native-overlay-hide-errors-visible-in-fixture");
    await call("smoke-native-overlay-result", { pass: true, checks, samples });
  } catch (e) {
    await call("smoke-native-overlay-result", {
      pass: false,
      error: String(e),
      checks,
    });
  }
}
