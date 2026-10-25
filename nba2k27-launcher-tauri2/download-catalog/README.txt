2K Launcher curated download library — QA133 dev

This folder is the editable seed catalog and artwork. The dev launcher has a
Downloads page with Startup videos, Playlists, and Blacktop Previews tabs, plus a Curator panel.

Normal users:
- Opening Downloads automatically lists public files in the two approved MediaFire folders and loads
  editorial metadata from GitHub. No downloads or catalog requests at startup.
- Download & Add verifies size and SHA-256, then uses the existing media importer.
- Added content does not automatically become the active video or playlist.
- MP4 only for startup playback. Native game MOV files are shown as unsupported.

Curator (dev-curator feature only):
- Uses the installed GitHub CLI at C:\Program Files\GitHub CLI\gh.exe and its
  existing sign-in. No GitHub token is exposed to the web UI or bundled in the EXE.
- Load Curator, select any download, edit title/description/order/art,
  optionally hide it, and Preview locally.
- Upload cover to GitHub accepts PNG/JPG/WebP up to 4 MB. It uploads an immutable
  content-addressed image immediately; Publish to GitHub links it into the catalog.
- Publish to GitHub writes ONLY launcher-downloads/catalog.json and referenced
  cover artwork in carrnate85-stack/2kconversiontool, branch main.
- The catalog uses the loaded GitHub file SHA to reject concurrent edits.
  After a conflict/timeout, reload before retrying. Do not blindly overwrite.
- Initial publication also uploads the included game covers. It never publishes
  the application, installs a launcher, or promotes Main/Share.
- Video/playlist upload remains in MediaFire's website; authenticated MediaFire
  uploading is not part of this build. Upload a file there, Refresh, reopen Curator
  and edit the newly discovered entry. Use the same folder links below.

Public catalog URL after publication:
https://raw.githubusercontent.com/carrnate85-stack/2kconversiontool/main/launcher-downloads/catalog.json
Artwork uses art/<filename> relative to that catalog, or a raw GitHub image URL.
Before first publication, a missing remote catalog falls back to this bundled
catalog with an explicit notice. Other refresh failures preserve the prior listing.

MediaFire videos:
https://www.mediafire.com/folder/42pf1wsmrcs2t/2k+Launcher+Startup+Videos
MediaFire playlists:
https://www.mediafire.com/folder/3d9j5r16tk85y/2k+Launcher+Playlists

Curator has a MediaFire download link field for every item. Paste a public file
link, click Check download link, then Preview locally or Publish to GitHub.
Native code obtains filename, size and SHA-256 from MediaFire; publishing rechecks
them before GitHub writes. Actual downloads verify the bytes against that pin
and use the existing type-specific importer. Use MP4, .2kplaylist, or a complete
preview ZIP. Do not substitute an accessory-only archive for the complete pack.
Changing a display name does not change identity. Replacement links preserve
the editorial item. Older launchers need updating before using replacement links.

Artwork source pages and original image URLs are in ART-SOURCES.txt.
Draft edits in the panel are local until Publish. Closing the panel/page discards
unsaved edits. Refresh replaces a local preview with the published catalog.

Production/sharing builds must omit --features dev-curator. The normal library
works without GitHub CLI; only curator operations use it.
