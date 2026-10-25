# Dev roster fixes and cleanup audit

## Ratings import

The inherited browser Proxy treated every name starting with `on` as an event subscription. `onlineRatings` therefore returned a function without calling the authenticated native RPC. A Launcher-local bridge override now explicitly routes this method. Capture identity, origin, size, destination and cancellation checks remain intact. The supplied Tayshaun Prince 2K17 page was downloaded and parsed as text; its selected height and wingspan are both 205.74 cm.

## Saving and measurements

A read-only comparison of the running game showed one changed four-byte value at ClassicYear descriptor RVA 0x89BFAE8+4. Native reflection initialization replaces provisional type hash 0x12D46C62 with signed-32 type hash 0x1451DAB1. The field offset, width, mask and other descriptor bytes match disk exactly. The writer now accepts only those two states, pins the initializer instructions, and still hashes the entire normalized descriptor. Mutation tests reject changes to every byte.

The optional native appearance reader was not connected to opening a player. The browser now obtains a tab-owned, identity-checked measurement read before constructing the edit baseline. Late replies cannot reopen a closed player or replace another selection. Imports use the existing review/staging path. Unavailable or shared appearance data remain read-only, with the native reason retained for diagnostics.

The separate in-process measurement writer also had old code RVAs and hashes. Its eleven method pins, root references, vtable targets and image bounds now match the current, previously verified Rust proof. The new DLL is embedded in the dev launcher; the old exact DLL remains admitted for recovery only. Native transaction, mask, alias, journal and compare/exchange checks are unchanged.

Checks include JavaScript bridge, mapping, staging and stale-reply tests; Rust roster policy and authenticated route tests; all ten disk profile groups; native core/exception tests; DLL load/export and exception-unwind checks. The game closed before the final live diagnostic, so an actual live save/import is still a user acceptance test. No game data was changed by this work.

## Other cleanup holds reviewed

- Player source replacement/removal and loose override replacement are covered by the previous verified-backup fix. The old linked bytes are preserved before removing the owned link.
- Ordinary unrelated mod files do not participate in session cleanup. Removing a session-owned file already skips its absent target.
- Replacing launcher-generated selectors, camera localization, Jukebox output, or loader DLL/config files can still hold recovery when their recorded identity/contents differ. Cleanup preserves those replacements instead of deleting them. Jukebox also retains an original music archive when a replacement occupies its restoration path.
- Replacing the mods directory, a profile junction, a receipt, or the saved original directory remains an ownership conflict. A running game/provider also correctly prevents cleanup.
- Cleanup attempts independent categories even when one category fails, so one conflict does not strand every other owned file.

This is an audit of those remaining cases, not a claim that all ownership conflicts were removed. The renderer snapshot, Exporter worker and installed release build are outside this update.
