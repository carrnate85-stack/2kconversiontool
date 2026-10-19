import { describe, expect, it } from "vitest";
import {
  initialModDecisions,
  modInstallArgs,
  modInstallConflicts,
  modInstallCounts,
  modInstallIssueSummary,
  modInstallReplaceMessage,
  modInstallSource,
  modInstallSummary,
  setAllModDecisions,
  type ModInstallPlan,
} from "./mod-install";

const plan: ModInstallPlan = {
  cancelled: false,
  planId: "abc",
  root: "D:\\Mods",
  items: [
    {
      name: "arena_999.iff",
      logicalPath: "arenas/arena_999.iff",
      routing: "manifest",
      size: 5,
      existing: { size: 3, modifiedMs: 1 },
    },
    {
      name: "english.iff",
      logicalPath: "loc/english.iff",
      routing: "manifest",
      size: 7,
      existing: { size: 7, modifiedMs: 2 },
    },
    {
      name: "png1234.iff",
      logicalPath: "char/sig/png1234.iff",
      routing: "player-char-sig",
      size: 9,
      existing: null,
    },
  ],
  issues: [],
  ignored: [
    { name: "readme.txt", reason: "Package documentation or preview file." },
  ],
  totalBytes: 21,
  replacements: 2,
  archives: 1,
  folders: 0,
};

describe("mod install review", () => {
  it("defaults every conflict to WPF's replace and counts installs", () => {
    const decisions = initialModDecisions(plan);
    expect(modInstallConflicts(plan)).toHaveLength(2);
    expect(Object.values(decisions)).toEqual(["replace", "replace"]);
    expect(modInstallCounts(plan, decisions)).toEqual({
      added: 1,
      replace: 2,
      keep: 0,
      install: 3,
    });
    expect(modInstallReplaceMessage(1)).toBe(
      "1 destination already contains a mod.",
    );
    expect(modInstallReplaceMessage(2)).toBe(
      "2 destinations already contain mods.",
    );
  });
  it("puts each conflict in exactly one commit list", () => {
    const decisions = {
      ...setAllModDecisions(plan, "keep"),
      "arenas/arena_999.iff": "replace" as const,
    };
    expect(modInstallArgs(plan, decisions)).toEqual({
      planId: "abc",
      replace: ["arenas/arena_999.iff"],
      keep: ["loc/english.iff"],
    });
    const all = setAllModDecisions(plan, "replace");
    expect(modInstallCounts(plan, all)).toEqual({
      added: 1,
      replace: 2,
      keep: 0,
      install: 3,
    });
    expect(modInstallArgs(plan, all).keep).toEqual([]);
  });
  it("summarises results like the WPF toasts", () => {
    expect(
      modInstallSummary({
        installed: 2,
        replaced: 1,
        kept: 1,
        ignored: 1,
        archives: 1,
        folders: 0,
      }),
    ).toBe(
      "Installed 2 mod files · replaced 1 · kept 1 · ignored 1 non-mod file.",
    );
    expect(
      modInstallSummary({
        installed: 1,
        replaced: 0,
        kept: 0,
        ignored: 0,
        archives: 0,
        folders: 0,
      }),
    ).toBe("Installed 1 mod file.");
    const issue = (name: string) => ({ name, reason: "Bad." });
    expect(modInstallIssueSummary([issue("a.iff")])).toBe(
      "Nothing installed. a.iff: Bad.",
    );
    expect(modInstallIssueSummary(["a", "b", "c", "d"].map(issue))).toBe(
      "Nothing installed. a: Bad. · b: Bad. · c: Bad. · More files were rejected.",
    );
    expect(modInstallSource(plan)).toBe("1 archive");
    expect(modInstallSource({ ...plan, archives: 0, folders: 1 })).toBe(
      "1 folder",
    );
    expect(modInstallSource({ ...plan, archives: 0 })).toBe("3 selected files");
  });
});
