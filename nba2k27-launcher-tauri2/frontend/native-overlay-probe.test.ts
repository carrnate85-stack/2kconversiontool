import { expect, test } from "vitest";
import { nativeOverlayCatalogSelector } from "./native-overlay-probe";
test("overlay fixture scopes equal installed labels to distinct ball and scoreboard sections", () => {
  expect(nativeOverlayCatalogSelector("Currently installed")).toBe(
    ".overlay-item.ball button",
  );
  expect(nativeOverlayCatalogSelector("Scoreboard")).toBe(
    ".overlay-item.scoreboard button",
  );
  expect(nativeOverlayCatalogSelector("Game rules")).toBeNull();
  expect(nativeOverlayCatalogSelector("Currently installed")).not.toBe(
    nativeOverlayCatalogSelector("Scoreboard"),
  );
});
