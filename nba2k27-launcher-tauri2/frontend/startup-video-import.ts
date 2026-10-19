export type StartupVideoImportProgress = {
  operationId: string;
  phase: "checking" | "copying" | "verifying" | "committing" | "done";
  bytes: number;
  total: number;
};

const phases = new Set<StartupVideoImportProgress["phase"]>([
  "checking",
  "copying",
  "verifying",
  "committing",
  "done",
]);

const uuidPattern =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

export function startupVideoImportOperationId(uuid: string): string {
  if (!uuidPattern.test(uuid))
    throw new Error("Secure startup-video import identity is unavailable");
  return `video-import-${uuid.toLowerCase()}`;
}

export function startupVideoImportProgress(
  value: unknown,
): StartupVideoImportProgress | null {
  if (!value || typeof value !== "object") return null;
  const row = value as Record<string, unknown>;
  if (
    typeof row.operationId !== "string" ||
    row.operationId.length < 1 ||
    row.operationId.length > 128 ||
    !/^[\x20-\x7e]+$/.test(row.operationId) ||
    typeof row.phase !== "string" ||
    !phases.has(row.phase as StartupVideoImportProgress["phase"]) ||
    typeof row.bytes !== "number" ||
    !Number.isSafeInteger(row.bytes) ||
    row.bytes < 0 ||
    typeof row.total !== "number" ||
    !Number.isSafeInteger(row.total) ||
    row.total < 1 ||
    row.bytes > row.total
  )
    return null;
  return row as StartupVideoImportProgress;
}

export function startupVideoImportForOperation(
  value: unknown,
  operationId: string,
): StartupVideoImportProgress | null {
  const progress = startupVideoImportProgress(value);
  return progress?.operationId === operationId ? progress : null;
}

export function startupVideoImportPercent(
  progress: StartupVideoImportProgress,
): number {
  if (progress.phase === "done") return 100;
  return Math.min(99, Math.floor((progress.bytes / progress.total) * 100));
}

export function startupVideoImportCanCancel(
  progress: StartupVideoImportProgress,
): boolean {
  return !["committing", "done"].includes(progress.phase);
}

export function startupVideoImportLabel(
  phase: StartupVideoImportProgress["phase"],
): string {
  return {
    checking: "Checking MP4",
    copying: "Copying into portable library",
    verifying: "Verifying copied file",
    committing: "Saving video catalog",
    done: "Startup video ready",
  }[phase];
}
