import type { CharacterTool } from '../web/tauri-bridge';

declare const api: CharacterTool;
api.onJobComplete(event => { const path: string = event.result.output; void path; });
api.previewNativeHair({ assetType: 'facialhair', assetId: 'test' });
// @ts-expect-error Catalog categories use the engine's actual names.
api.previewNativeHair({ assetType: 'beard' });
// @ts-expect-error The conversion request requires a complete source and target.
api.startSwap({ source: 'png4100.iff' });
// @ts-expect-error A file picker resolves to a path, not a numeric identifier.
const identifier: Promise<number> = api.chooseCharacter('source');
