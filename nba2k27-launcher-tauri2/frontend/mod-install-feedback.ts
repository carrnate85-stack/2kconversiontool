export async function finishModInstall(summary: string, scan: () => Promise<void>, status: (message: string) => void) {
  status(summary);
  try { await scan(); status(summary); }
  catch (error) { status(`${summary} Library refresh failed: ${String(error)}`); }
}
