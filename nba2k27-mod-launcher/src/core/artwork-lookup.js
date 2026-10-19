const https = require('node:https');

const MAX_JSON_BYTES = 2 * 1024 * 1024;
const MAX_IMAGE_BYTES = 20 * 1024 * 1024;
const USER_AGENT = 'NBA2K27ModLauncher/0.35.7 (desktop app; user-initiated requests only)';
let nextMusicBrainzRequestAt = 0;
let musicBrainzQueue = Promise.resolve();

function cleanQuery(value, label) {
  const result = String(value || '').replace(/\s+/g, ' ').trim();
  if (!result || result.length > 120) throw new Error(`${label} is required for artwork lookup.`);
  return result;
}

function lucene(value) {
  return String(value).replace(/([+\-&|!(){}\[\]^"~*?:\\/])/g, '\\$1');
}

function allowedHost(hostname) {
  const host = String(hostname || '').toLowerCase();
  return host === 'musicbrainz.org' || host === 'coverartarchive.org'
    || host === 'archive.org' || host.endsWith('.archive.org');
}

function download(url, { maximum = MAX_JSON_BYTES, redirects = 5 } = {}) {
  return new Promise((resolve, reject) => {
    const parsed = new URL(url);
    if (parsed.protocol === 'http:' && allowedHost(parsed.hostname)) parsed.protocol = 'https:';
    if (parsed.protocol !== 'https:' || !allowedHost(parsed.hostname)) {
      reject(new Error('Artwork lookup returned an untrusted URL.'));
      return;
    }
    const request = https.get(parsed, {
      headers: { 'User-Agent': USER_AGENT, Accept: '*/*' },
      timeout: 15000,
    }, (response) => {
      const status = Number(response.statusCode || 0);
      if ([301, 302, 303, 307, 308].includes(status) && response.headers.location && redirects > 0) {
        response.resume();
        const destination = new URL(response.headers.location, parsed).href;
        download(destination, { maximum, redirects: redirects - 1 }).then(resolve, reject);
        return;
      }
      if (status < 200 || status >= 300) {
        response.resume();
        const error = new Error(`Artwork service returned HTTP ${status}.`);
        error.status = status;
        reject(error);
        return;
      }
      const announced = Number(response.headers['content-length'] || 0);
      if (announced > maximum) {
        response.destroy();
        reject(new Error('Artwork service response exceeded the safe size limit.'));
        return;
      }
      const chunks = [];
      let length = 0;
      response.on('data', (chunk) => {
        length += chunk.length;
        if (length > maximum) {
          response.destroy(new Error('Artwork service response exceeded the safe size limit.'));
          return;
        }
        chunks.push(chunk);
      });
      response.on('end', () => resolve({
        bytes: Buffer.concat(chunks),
        contentType: String(response.headers['content-type'] || '').split(';')[0].toLowerCase(),
        finalUrl: parsed.href,
      }));
    });
    request.on('timeout', () => request.destroy(new Error('Artwork lookup timed out.')));
    request.on('error', reject);
  });
}

async function rateLimitedMusicBrainzJson(url) {
  const operation = musicBrainzQueue.then(async () => {
    for (let attempt = 0; attempt < 3; attempt += 1) {
      const delay = Math.max(0, nextMusicBrainzRequestAt - Date.now());
      if (delay) await new Promise((resolve) => setTimeout(resolve, delay));
      nextMusicBrainzRequestAt = Date.now() + 1100;
      try {
        const response = await download(url, { maximum: MAX_JSON_BYTES });
        return JSON.parse(response.bytes.toString('utf8'));
      } catch (error) {
        if (![429, 503].includes(error.status) || attempt === 2) {
          if ([429, 503].includes(error.status)) {
            throw new Error('The online artwork service is busy. Wait a moment and try Find online again.');
          }
          throw error;
        }
        await new Promise((resolve) => setTimeout(resolve, 1200 * (attempt + 1)));
      }
    }
    throw new Error('The online artwork service is busy. Try again shortly.');
  });
  musicBrainzQueue = operation.catch(() => {});
  return operation;
}

function artworkCandidates(payload) {
  const candidates = [];
  const seen = new Set();
  for (const recording of Array.isArray(payload?.recordings) ? payload.recordings : []) {
    for (const release of Array.isArray(recording.releases) ? recording.releases : []) {
      const groupId = release['release-group']?.id;
      const values = [
        groupId ? { kind: 'release-group', id: groupId } : null,
        release.id ? { kind: 'release', id: release.id } : null,
      ].filter(Boolean);
      for (const value of values) {
        const key = `${value.kind}:${value.id}`;
        if (seen.has(key)) continue;
        seen.add(key);
        candidates.push({
          ...value,
          title: String(recording.title || ''),
          artist: String(recording['artist-credit']?.map((item) => item.name).join('') || ''),
          album: String(release.title || release['release-group']?.title || ''),
          score: Number(recording.score || 0),
        });
      }
    }
  }
  return candidates.slice(0, 8);
}

// Session-only LRU. Share in-flight work; never retain failed requests.
class ArtworkCache {
  constructor({ maxBytes = 32 * 1024 * 1024, maxEntries = 64, ttl = 10 * 60 * 1000, now = Date.now } = {}) {
    Object.assign(this, { maxBytes, maxEntries, ttl, now });
    this.values = new Map();
    this.pending = new Map();
    this.bytes = 0;
  }
  drop(key) {
    const entry = this.values.get(key);
    if (entry) this.bytes -= entry.size;
    this.values.delete(key);
  }
  get(key, load) {
    for (const [oldKey, entry] of this.values) if (entry.expires <= this.now()) this.drop(oldKey);
    const hit = this.values.get(key);
    if (hit) {
      this.values.delete(key);
      this.values.set(key, hit);
      return Promise.resolve(hit.value);
    }
    if (this.pending.has(key)) return this.pending.get(key);
    const operation = Promise.resolve().then(load).then(value => {
      const size = value.bytes?.length || 0;
      if (size <= this.maxBytes && this.maxEntries > 0) {
        while (this.values.size && (this.bytes + size > this.maxBytes || this.values.size >= this.maxEntries)) {
          this.drop(this.values.keys().next().value);
        }
        this.values.set(key, { value, size, expires: this.now() + this.ttl });
        this.bytes += size;
      }
      return value;
    }).finally(() => this.pending.delete(key));
    this.pending.set(key, operation);
    return operation;
  }
}

class ArtworkLookup {
  constructor({ search = rateLimitedMusicBrainzJson, fetchImage = (url) => download(url, { maximum: MAX_IMAGE_BYTES }) } = {}) {
    this.search = search;
    this.fetchImage = fetchImage;
    this.matches = new ArtworkCache();
    this.images = new ArtworkCache();
  }

  async find({ artist, title }) {
    const safeArtist = cleanQuery(artist, 'Artist');
    const safeTitle = cleanQuery(title, 'Song title');
    if (safeArtist.toLowerCase() === 'custom') throw new Error('Name the artist before searching for artwork.');
    const key = JSON.stringify([safeArtist.toLowerCase(), safeTitle.toLowerCase()]);
    return this.matches.get(key, () => this.findUncached(safeArtist, safeTitle));
  }

  async findUncached(safeArtist, safeTitle) {
    const query = `recording:\"${lucene(safeTitle)}\" AND artist:\"${lucene(safeArtist)}\"`;
    const url = `https://musicbrainz.org/ws/2/recording?query=${encodeURIComponent(query)}&fmt=json&limit=5`;
    let candidates = artworkCandidates(await this.search(url));
    if (!candidates.length) {
      const broadQuery = `recording:${lucene(safeTitle)} AND artist:${lucene(safeArtist)}`;
      const broadUrl = `https://musicbrainz.org/ws/2/recording?query=${encodeURIComponent(broadQuery)}&fmt=json&limit=5`;
      candidates = artworkCandidates(await this.search(broadUrl));
    }
    if (!candidates.length) throw new Error(`No online music match was found for ${safeArtist} - ${safeTitle}.`);
    for (const candidate of candidates) {
      const artworkUrl = `https://coverartarchive.org/${candidate.kind}/${candidate.id}/front-500`;
      try {
        let result;
        for (let attempt = 0; attempt < 3; attempt += 1) {
          try {
            result = await this.images.get(artworkUrl, async () => {
              const image = await this.fetchImage(artworkUrl);
              if (!image.bytes?.length || !/^image\/(jpeg|png|webp)$/.test(image.contentType)) {
                throw new Error('The artwork service returned an unsupported image.');
              }
              return image;
            });
            break;
          } catch (error) {
            if (![429, 503].includes(error.status) || attempt === 2) throw error;
            await new Promise((resolve) => setTimeout(resolve, 700 * (attempt + 1)));
          }
        }
        if (!result.bytes?.length || !/^image\/(jpeg|png|webp)$/.test(result.contentType)) {
          throw new Error('The artwork service returned an unsupported image.');
        }
        return {
          name: `${safeArtist} - ${safeTitle}.jpg`,
          bytes: result.bytes,
          match: { ...candidate, provider: 'MusicBrainz / Cover Art Archive' },
        };
      } catch (error) {
        if (error.status === 404) continue;
        throw error;
      }
    }
    throw new Error(`A song match was found, but no online cover artwork is available for ${safeArtist} - ${safeTitle}.`);
  }
}

module.exports = { ArtworkLookup, ArtworkCache, allowedHost, artworkCandidates };
