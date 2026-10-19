// Read-only lineup refreshes must never silently discard a user action.
// Reserve foreground work synchronously, then drain the already-running read.
// Subsequent background reads are skipped until the foreground caller finishes.
// This queue only serializes transport; it never retries a game write.
export class OutfitRequestQueue {
  private tail: Promise<void> = Promise.resolve();
  private foreground = 0;
  private background = false;

  get busy() {
    return this.foreground > 0 || this.background;
  }

  run<T>(background: boolean, operation: () => Promise<T>): Promise<T | null> {
    if (background && this.busy) return Promise.resolve(null);
    // Match WPF's exclusive operation guard. Wait behind a read, not behind
    // another user write (a double-click must never become a second Apply).
    if (!background && this.foreground > 0)
      return Promise.reject(
        new Error("An outfit operation is already running."),
      );
    if (background) this.background = true;
    else this.foreground++;
    const previous = this.tail;
    const result = previous.then(operation);
    this.tail = result.then(
      () => {},
      () => {},
    );
    return result.finally(() => {
      if (background) this.background = false;
      else this.foreground--;
    });
  }
}
