const PROVIDERS = Object.freeze({
  local: {
    id: 'local',
    name: 'Local unique-asset loader',
    state: 'available',
    description: 'Uses the launcher-owned current-user provider and stock-first in-game resource device.',
    supportsUnlistedAssets: true,
  },
  folder: {
    id: 'folder',
    name: 'Game mods folder',
    state: 'limited',
    description: 'Catalogs ordinary game-supported mods. Unique IDs outside the manifest are not guaranteed.',
    supportsUnlistedAssets: false,
  },
});

async function providerStatus(id, settings) {
  const provider = PROVIDERS[id];
  if (!provider) throw new Error(`Unknown asset provider: ${id}`);
  if (id === 'local') return { ...provider, ready: true };
  if (id === 'folder') return { ...provider, ready: true };
  return { ...provider, ready: false };
}

module.exports = { PROVIDERS, providerStatus };
