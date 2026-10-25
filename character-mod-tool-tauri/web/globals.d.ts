interface Window {
  __TAURI__: {
    core: { invoke<T>(command: string, args: Record<string, unknown>): Promise<T> };
    event: { listen<T>(event: string, callback: (event: { payload: T }) => void): Promise<import('./contracts').Unsubscribe> };
  };
  characterTool: import('./tauri-bridge').CharacterTool;
}
