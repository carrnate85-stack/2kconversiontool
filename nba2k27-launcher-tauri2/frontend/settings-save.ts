// One serialized settings stream. Backend events cannot hide a newer local patch.
export class SettingsSaver<T extends object> {
  private confirmed: T | null = null;
  private pending: { patch: Partial<T> }[] = [];
  private tail: Promise<void> = Promise.resolve();
  constructor(private readonly io: {
    publish: (value: T) => void;
    save: (patch: Partial<T>) => Promise<unknown>;
    load: () => Promise<T>;
    saved: () => void;
    failed: (error: unknown) => void;
  }) {}
  accept(value: T) {
    this.confirmed = value;
    this.publish();
  }
  private publish() {
    if (this.confirmed) this.io.publish(Object.assign({}, this.confirmed, ...this.pending.map(p => p.patch)));
  }
  save(patch: Partial<T>): Promise<void> {
    const item = { patch };
    this.pending.push(item);
    this.publish();
    this.tail = this.tail.then(async () => {
      try {
        await this.io.save(patch);
        this.confirmed = { ...this.confirmed!, ...patch };
        this.pending = this.pending.filter(p => p !== item);
        this.publish();
        this.io.saved();
      } catch (error) {
        this.pending = this.pending.filter(p => p !== item);
        try { this.confirmed = await this.io.load(); }
        catch (reload) { error = new Error(`${String(error)}; saved settings could not be reloaded: ${String(reload)}`); }
        this.publish();
        this.io.failed(error);
      }
    });
    return this.tail;
  }
}
