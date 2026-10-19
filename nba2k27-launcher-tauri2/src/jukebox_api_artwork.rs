//! Opaque, song/revision-bound artwork capabilities. No renderer paths/URLs,
//! automatic network request, audio ownership change or native game write.
use super::*;
pub(super) const TTL: Duration = Duration::from_secs(10 * 60);
const MAX_IMAGES: usize = 128;
const MAX_IMAGE_BYTES: usize = 32 * 1024 * 1024;
const MAX_PREPARED: usize = 16;
pub(super) enum Kind {
    Audio,
    Artwork {
        key: String,
        revision: String,
        saved: bool,
    },
}
pub(super) struct PreparedLease {
    key: String,
    revision: String,
    expires: Instant,
    pub native: Arc<[u8]>,
    pub preview: Arc<[u8]>,
}
fn png(bytes: &[u8]) -> Result<()> {
    ensure!(
        bytes.len() >= 33
            && bytes.len() <= 4 * 1024 * 1024
            && bytes.starts_with(b"\x89PNG\r\n\x1a\n")
            && &bytes[12..16] == b"IHDR"
            && u32::from_be_bytes(bytes[16..20].try_into()?) == 256
            && u32::from_be_bytes(bytes[20..24].try_into()?) == 256,
        "Artwork preview must be a bounded 256x256 PNG"
    );
    Ok(())
}
fn prune(s: &mut Status) {
    let now = Instant::now();
    s.leases.retain(|_, l| l.expires > now);
    s.prepared_art.retain(|_, l| l.expires > now);
}
impl Api {
    fn image_lease(
        &self,
        key: &str,
        revision: &str,
        bytes: Vec<u8>,
        prepared: Option<Vec<u8>>,
        matched_title: Option<String>,
        matched_artist: Option<String>,
        source: Option<String>,
    ) -> Result<Value> {
        png(&bytes)?;
        if let Some(native) = &prepared {
            crate::jukebox_artwork::validate_native(native)?;
        }
        let token = nonce()?;
        let url = self
            .shared
            .host
            .preview_url(&token)
            .context("Owned artwork preview endpoint is unavailable")?;
        let now = Instant::now();
        let data: Arc<[u8]> = bytes.into();
        let mut s = lock(&self.shared.status)?;
        prune(&mut s);
        ensure!(
            !self.shared.closing.load(Ordering::Acquire) && s.view.revision == revision,
            "Artwork library changed or is closing; retry explicitly"
        );
        ensure!(
            s.leases
                .values()
                .filter(|l| matches!(l.kind, Kind::Artwork { .. }))
                .count()
                < MAX_IMAGES
                && s.leases
                    .values()
                    .filter(|l| matches!(l.kind, Kind::Artwork { .. }))
                    .map(|l| l.data.len())
                    .sum::<usize>()
                    + data.len()
                    <= MAX_IMAGE_BYTES,
            "Release earlier artwork previews before opening more"
        );
        if let Some(native) = prepared {
            ensure!(
                s.prepared_art.len() < MAX_PREPARED,
                "Release earlier prepared artwork before choosing more"
            );
            s.prepared_art.insert(
                token.clone(),
                PreparedLease {
                    key: key.into(),
                    revision: revision.into(),
                    expires: now + TTL,
                    native: native.into(),
                    preview: data.clone(),
                },
            );
        }
        let saved = !s.prepared_art.contains_key(&token);
        s.leases.insert(
            token.clone(),
            Lease {
                mime: "image/png",
                data,
                expires: now + TTL,
                kind: Kind::Artwork {
                    key: key.into(),
                    revision: revision.into(),
                    saved,
                },
            },
        );
        let expires_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis() as u64
            + TTL.as_millis() as u64;
        Ok(
            json!({"token":token,"url":url,"mimeType":"image/png","matchedTitle":matched_title,"matchedArtist":matched_artist,"source":source,"expiresAt":expires_at}),
        )
    }
    pub(super) fn dispatch_artwork(
        &self,
        channel: &str,
        a: &Args,
        revision: &str,
    ) -> Result<Option<Value>> {
        if !matches!(
            channel,
            "jukebox:get-artwork"
                | "jukebox:artwork-preview"
                | "jukebox:choose-artwork"
                | "jukebox:lookup-artwork"
        ) {
            return Ok(None);
        }
        let key = a
            .track_key
            .as_deref()
            .context("Track identity is required")?;
        let readonly = matches!(channel, "jukebox:get-artwork" | "jukebox:artwork-preview");
        // Lazy visible rows issue parallel reads. They need a snapshot/CAS,
        // not the exclusive mutation/picker reservation.
        let _reservation = if readonly {
            None
        } else {
            Some(self.reserve(revision)?)
        };
        {
            let s = lock(&self.shared.status)?;
            ensure!(
                !self.shared.closing.load(Ordering::Acquire) && s.view.revision == revision,
                "Artwork library changed or is closing"
            );
        }
        {
            let service = lock(&self.shared.service)?;
            ensure!(
                service.fingerprint()? == revision,
                "Artwork snapshot differs; refresh before reading"
            );
            ensure!(
                service.view().music.tracks.iter().any(|t| t.key() == key),
                "Song is missing"
            );
        }
        if !self.shared.host.preview_enabled() {
            return Ok(Some(json!({"artwork":null})));
        }
        cancelled(&self.shared.closing)?;
        match channel {
            "jukebox:get-artwork" | "jukebox:artwork-preview" => {
                // Every UI consumer owns a distinct lease. Sharing the same
                // token would let one row/editor cleanup invalidate another.
                let bytes = lock(&self.shared.service)?.artwork_preview(key)?;
                let art = bytes
                    .map(|b| self.image_lease(key, revision, b, None, None, None, None))
                    .transpose()?;
                Ok(Some(json!({"artwork":art})))
            }
            "jukebox:choose-artwork" => {
                let Some(path) = self.shared.host.choose_artwork(&self.shared.closing)? else {
                    return Ok(Some(json!({"artwork":null,"kind":"cancelled"})));
                };
                let mut source = Source::open(&path, 4 * 1024 * 1024)?;
                let bytes = source.read_all(4 * 1024 * 1024, &self.shared.closing)?;
                let prepared = self
                    .shared
                    .host
                    .prepare_artwork(&bytes, &self.shared.closing)?;
                source.verify()?;
                cancelled(&self.shared.closing)?;
                Ok(Some(
                    json!({"artwork":self.image_lease(key,revision,prepared.preview,Some(prepared.native),None,None,Some("Local file".into()))?}),
                ))
            }
            "jukebox:lookup-artwork" => {
                ensure!(
                    self.shared.host.artwork_lookup_enabled(),
                    "Artwork lookup is unavailable in this isolated fixture"
                );
                let artist = a
                    .artist
                    .as_deref()
                    .context("Artwork search artist is required")?;
                let title = a
                    .title
                    .as_deref()
                    .context("Artwork search title is required")?;
                ensure!(
                    !artist.trim().is_empty()
                        && !title.trim().is_empty()
                        && artist.chars().count() <= 120
                        && title.chars().count() <= 120
                        && !artist.chars().chain(title.chars()).any(char::is_control),
                    "Use an explicit printable artist/title artwork query"
                );
                let found = self
                    .shared
                    .host
                    .lookup_artwork(artist, title, &self.shared.closing)?;
                ensure!(
                    found.len() <= 6,
                    "Artwork lookup exceeded its bounded candidate limit"
                );
                cancelled(&self.shared.closing)?;
                // Current UI owns one result, not a gallery. Do not create
                // inaccessible candidate leases that it cannot release.
                let art = found
                    .into_iter()
                    .next()
                    .map(|c| {
                        self.image_lease(
                            key,
                            revision,
                            c.artwork.preview,
                            Some(c.artwork.native),
                            Some(c.matched_title),
                            Some(c.matched_artist),
                            Some(c.source),
                        )
                    })
                    .transpose()?;
                Ok(Some(json!({"artwork":art})))
            }
            _ => unreachable!(),
        }
    }
    pub(super) fn consume_artwork(
        &self,
        key: &str,
        revision: &str,
        token: Option<&str>,
    ) -> Result<Option<PreparedLease>> {
        let Some(token) = token else { return Ok(None) };
        ensure!(catalog::hex(token, 48), "Invalid prepared artwork token");
        let mut s = lock(&self.shared.status)?;
        prune(&mut s);
        let art = s
            .prepared_art
            .get(token)
            .context("Prepared artwork expired, was released or was already used")?;
        ensure!(
            art.key == key && art.revision == revision,
            "Prepared artwork belongs to another song or revision"
        );
        Ok(s.prepared_art.remove(token))
    }
    pub(super) fn release_artwork(&self, token: &str) -> Result<()> {
        ensure!(catalog::hex(token, 48), "Invalid artwork token");
        let mut s = lock(&self.shared.status)?;
        ensure!(
            s.leases
                .get(token)
                .is_none_or(|l| matches!(l.kind, Kind::Artwork { .. })),
            "Not an artwork preview token"
        );
        s.leases.remove(token);
        s.prepared_art.remove(token);
        Ok(())
    }
}
