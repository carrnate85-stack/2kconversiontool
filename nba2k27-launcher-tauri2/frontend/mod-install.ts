// Mod installer review helpers (WPF mods:install parity). The backend routes
// every file and refuses unsafe archives; this file only shapes the review.
export type ModInstallMode = "files" | "folder";
export type ModInstallItem = {
  name: string;
  logicalPath: string;
  routing: string;
  size: number;
  existing: { size: number; modifiedMs: number } | null;
};
export type ModInstallNote = { name: string; reason: string };
export type ModInstallPlan = {
  cancelled: false;
  planId: string | null;
  root: string;
  items: ModInstallItem[];
  issues: ModInstallNote[];
  ignored: ModInstallNote[];
  totalBytes: number;
  replacements: number;
  archives: number;
  folders: number;
};
export type ModInstallResult = {
  installed: number;
  replaced: number;
  kept: number;
  ignored: number;
  archives: number;
  folders: number;
};
export type ModInstallDecision = "replace" | "keep";
export type ModInstallDecisions = Record<string, ModInstallDecision>;

export const modInstallConflicts = (plan: ModInstallPlan) =>
  plan.items.filter((item) => item.existing);

/**
 * WPF asks once ("Replace and install" replaces every existing file); the
 * review starts there and lets single files be kept instead.
 */
export function initialModDecisions(plan: ModInstallPlan): ModInstallDecisions {
  return setAllModDecisions(plan, "replace");
}

export function setAllModDecisions(
  plan: ModInstallPlan,
  decision: ModInstallDecision,
): ModInstallDecisions {
  return Object.fromEntries(
    modInstallConflicts(plan).map((item) => [item.logicalPath, decision]),
  );
}

export function modInstallCounts(
  plan: ModInstallPlan,
  decisions: ModInstallDecisions,
) {
  const conflicts = modInstallConflicts(plan);
  const replace = conflicts.filter(
    (item) => decisions[item.logicalPath] === "replace",
  ).length;
  const added = plan.items.length - conflicts.length;
  return {
    added,
    replace,
    keep: conflicts.length - replace,
    install: added + replace,
  };
}

/** Commit arguments: every conflict appears in exactly one list. */
export function modInstallArgs(
  plan: ModInstallPlan,
  decisions: ModInstallDecisions,
) {
  const replace: string[] = [],
    keep: string[] = [];
  for (const item of modInstallConflicts(plan))
    (decisions[item.logicalPath] === "replace" ? replace : keep).push(
      item.logicalPath,
    );
  return { planId: plan.planId, replace, keep };
}

const plural = (count: number, word: string) =>
  `${count.toLocaleString()} ${word}${count === 1 ? "" : "s"}`;

/** WPF success toast, plus the kept count when single files were kept. */
export function modInstallSummary(result: ModInstallResult) {
  let text = `Installed ${plural(result.installed, "mod file")}`;
  if (result.replaced)
    text += ` · replaced ${result.replaced.toLocaleString()}`;
  if (result.kept) text += ` · kept ${result.kept.toLocaleString()}`;
  if (result.ignored)
    text += ` · ignored ${plural(result.ignored, "non-mod file")}`;
  return `${text}.`;
}

/** WPF toast when routing issues block the install. */
export function modInstallIssueSummary(issues: ModInstallNote[]) {
  const detail = issues
    .slice(0, 3)
    .map((issue) => `${issue.name}: ${issue.reason}`)
    .join(" · ");
  return `Nothing installed. ${detail}${issues.length > 3 ? " · More files were rejected." : ""}`;
}

/** WPF replace prompt message. */
export function modInstallReplaceMessage(count: number) {
  return count === 1
    ? "1 destination already contains a mod."
    : `${count.toLocaleString()} destinations already contain mods.`;
}

export function modInstallSource(plan: ModInstallPlan) {
  if (plan.folders) return "1 folder";
  if (plan.archives) return plural(plan.archives, "archive");
  return plural(plan.items.length + plan.issues.length, "selected file");
}
