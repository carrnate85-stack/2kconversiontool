CHARACTER MOD TOOL - BETA

Extract the complete ZIP before running Character-Mod-Tool.exe.
Do not run the application directly inside an archive viewer.
Use a reasonably short extraction location, for example C:\ModTools, to avoid
Windows path-length limits in Blender's supporting files.

STANDARD DOWNLOAD
Select an installed Blender 5.1 in Settings. The app can discover common installs.

WITH BLENDER DOWNLOAD
Includes the unmodified official Blender 5.1.2 portable Windows x64 distribution.
Keep the Blender folder beside Character-Mod-Tool.exe. New setups automatically
use it. A valid Blender path you explicitly saved in Settings remains preferred.
To switch to the bundled copy, browse to Blender/blender.exe in Settings, or clear
the Blender field and save. Automatic selection follows the release if moved.
Blender is not installed system-wide and this package does not replace your
existing Blender installation.

FIRST LAUNCH
The EXE prepares its conversion engine under LocalAppData/CharacterModToolTauri.
Later launches reuse it. Settings and outputs remain in the app's user-data area
unless you choose a different output folder. Blender is used from this extracted
folder, not copied into each runtime cache.

NBA 2K18 TO NBA 2K23 BETA ROUTE
Full Swap can now use an NBA 2K18 source with an NBA 2K23 target player.
The route transfers the player into the target's packed geometry, updates all six
body/head LOD models, rebuilds the native face texture companion and preserves
the target 2K23 hair, facial hair, headbands and other accessories. Source hair
and facial-hair conversion are intentionally unavailable for this route until
their 2K23 target packaging is separately verified. Keep a backup and verify
eyes, blinking, expressions, body fit and distance LODs in NBA 2K23.

CLOSE SEAMS
Full Swap to NBA 2K27 automatically closes safely matching neck/chest and
chest/torso border strips after chest correction, before staging the result.
Preview > Close Seams remains available for older conversions. Unsafe matches
are skipped with a warning while the successful conversion is kept.
The seam pass changes positions only, preserving
vertex count, triangle indices, UVs, skin weights, morph data and textures.
A recovery IFF is saved under LocalAppData/CharacterModToolTauri/Player Backups/seams.
It does not blend texture colors or repair arbitrary holes, and ambiguous or
excessive gaps are left unchanged. Finish Blender edits and save body-fit edits
before using it. The saved staged IFF is updated and Preview reloads.

REQUIREMENTS
Windows x64, Microsoft Edge WebView2 Runtime, Visual C++ v14 x64 runtime,
.NET Framework 4.x for compatibility helpers, and your relevant game files.
The Standard package also requires a separate Blender 5.1 installation.
No Node or Rust development tools are required.

BLENDER NOTICES AND SOURCE (WITH BLENDER DOWNLOAD)
Blender is a separate, unmodified application distributed under GNU GPL terms.
Its original license notices are retained in Blender/. The matching official
Blender 5.1.2 source archive is included in Sources/blender-5.1.2.tar.xz.
Official binary: https://download.blender.org/release/Blender5.1/
Official source: https://download.blender.org/source/blender-5.1.2.tar.xz
Additional official Blender dependency sources: https://svn.blender.org/svnroot/bf-blender/trunk/lib/packages/
This tool is not an official Blender Foundation or NBA 2K product.

BETA STATUS
Keep backups. Automated conversion, preview and Blender checks do not replace
in-game animation/LOD/config/roster testing or clean-PC validation.
