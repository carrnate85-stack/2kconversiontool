CHARACTER MOD TOOL v1.0.117-beta

Windows GUI for reading, editing, validating, and converting ZIP-style NBA 2K
character IFF packages. This is a private-beta release. Always preserve original
files and test completed packages in game.

REQUIREMENTS

- Windows 10 or 11
- A legitimate NBA 2K26 installation
- Blender 5.1 or a compatible build for mesh workflows
- Photoshop is optional
- The portable EXE does not require Python

FIRST RUN

1. Launch CharacterModTool.exe or Launch Character Mod Tool.bat.
2. Open Settings.
3. Click Auto Detect.
4. Verify the NBA 2K26 folder and Blender executable.
5. Save Settings.

Settings are stored per Windows user at:
%LOCALAPPDATA%\CharacterModTool\settings.json

All generated IFFs, reports, body-fit exports, and editable texture exports default
to the outputs folder beside CharacterModTool.exe. Use Open Output Folder on the toolbar.

WORKFLOW TABS

FULL SWAP

- Select a legacy or NBA 2K25 source png####.iff and either browse for a modern
  target or choose a clean target directly from the NBA 2K26 manifest.
- The manifest target picker supports player-name/PNG search and shows whether each
  player has hair, facial-hair, and headband assets before selection.
- Face textures can come from a matching face####.iff or, for NBA 2K25 players,
  a matching png####_config_*.iff beside the source player.
- Run Full Swap performs the background Blender head/body transfer.
- Face color/normal textures, supported tattoos, and target configurations are
  rebuilt and validated.
- Face and tattoo swaps detect resolution differences, resize and encode to the
  target texture profile, and synchronize the final DDS/TXTR metadata.
- Shrinkwrap Body is experimental and disabled by default. Appearance/body-fit
  transfer only runs when this option is enabled.
- NBA 2K25-style bodies with matching baseBodyShape, arms_shader,
  torso_shader, and legs_shader topology use active-UV group transfer and skip
  the legacy abdomen/groin anchors.
- Older body layouts automatically retain the legacy shrinkwrap fallback.
- Full Swap always retains the target player's original png#### identity and writes
  the completed package into the app's outputs folder.
- Existing target companion IFFs are copied into the output package under that same
  target number. Manifest-selected targets stage their native non-config companions
  automatically.
- Native manifest companions may intentionally reference shared game DDS resources;
  Full Swap recognizes those references without weakening validation for edited files.
- Full Swap staging folders are removed after success or failure with automatic
  retries for temporary OneDrive locks. Abandoned staging folders are cleaned later.
- Open Output in Blender is disabled until a Full Swap succeeds. It launches visible
  Blender and imports the completed player for inspection and small adjustments.

HEAD SWAP

- Transfers the head shape, eyes, mouth, and eyelashes without changing body geometry.
- Runs only the Blender head-transfer and target-IFF rebuild stages.
- Check Swap Setup and Test Background Link verify the bundled add-ons.

BODY SWAP

- Transfers body geometry without changing the head, eyes, mouth, or eyelashes.
- Matching baseBodyShape, arms_shader, torso_shader, and legs_shader topology uses
  active-UV transfer and skips abdomen/groin anchors.
- Older body layouts use the established arms/legs shrinkwrap and anchor fallback.
- Check Swap Setup and Test Background Link verify the bundled add-ons.

FACE

- Select legacy source and modern target characters.
- Choose the target configuration.
- Swap face color and face normal while preserving the target bent normal, detail
  normal, wrinkle color, and wrinkle normal textures.
- Source textures with different dimensions are automatically resized to the
  selected target configuration's dimensions, format, and mip profile.
- DDS and TXTR files can be inspected, edited, replaced, and saved.

TATTOOS

- Swaps chest_color and legs_color pairs from source to target.
- Source tattoos with different dimensions are automatically resized to their
  target texture slots.
- If a source tattoo area is absent, the corresponding target pair is removed.
- torso_color is protected because the glasses bake may depend on it.
- DDS and TXTR files can be inspected, edited, replaced, or removed where allowed.

APPEARANCE

- Reads modern appearance_info.json and legacy appearance_info.RDAT.
- Double-click editable values or use Edit Selected Value.
- Appearance/body-fit swap only changes fields with matching names.
- Apply and Save writes only appearance_info and preserves all other archive entries.

HAIR

- Reads the game manifest and native hair/facial-hair catalog from the configured
  NBA 2K26 installation.
- Install Selected preserves the target player's current png#### identity.
- Hair uses background Blender tangent fitting; facial hair keeps native tangent-space
  behavior.
- Matching item hair is copied by default.
- Full-detail LOD0 is used at every distance for transferred hair.

HEADBAND SWAP

- Transfers a legacy headband shape onto a modern target through background Blender.
- Requires a successful background-link test.

VALIDATOR

- Checks ZIP structure, appearance data, SCNE references, texture metadata, buffers,
  and other known game-readiness conditions.
- A passing validator cannot guarantee rendering; final in-game testing is required.
- Save Report creates a text report.

RENAME CHARACTER PACKAGE

- Use this only after Full Swap, hair, facial hair, glasses, headband, and other work
  is finished.
- Select the finished png####.iff and enter a new number or png#### name.
- The tool finds the player, configs, face, geo, item, hair, facial-hair, headband,
  glasses, and other same-number companion IFFs in that folder.
- It creates a complete renamed copy in the outputs folder, updates safe text
  references, verifies every output archive, and keeps the original package by default.
- Delete Original Package After Rename is enabled by default. When
  selected, the complete old-number package is removed only after the renamed package
  commits and validates successfully; failed deletion restores all original files.

ADVANCED

- Select a Character IFF independently with Open IFF.
- Apply Dynamic Body patches hihead.SCNE and installs/replaces the bundled morphs.
- The compatibility report shows marker and morph status before applying changes.
- Save Dynamic Body IFF writes the edited archive through the normal safe output flow.
- Dynamic Body may be unavailable in public builds that exclude review-required assets.

GLASSES (EXPERIMENTAL)

- Loads player-fitted glasses geometry and optional texture packages.
- Built-in glasses are available only when their reviewed asset bundle is present.
- Bake Into Hihead is experimental and uses the target torso texture/shader host.

DIAGNOSTICS

The Diagnostics toolbar button creates a ZIP containing rotating application logs,
path settings, system information, and the most recent validation result. It does not
include game archives or textures.

Logs are stored at:
%LOCALAPPDATA%\CharacterModTool\logs

SAFE FILE HANDLING

- Main, related, and appearance-only saves use staged temporary files.
- Full Swap stages and validates the complete output package before committing it.
- If a package commit fails, existing destination files are restored from temporary
  rollback copies. No permanent .bak files are created beside game files.
- Rename Character Package creates copies and never deletes the original package.

KNOWN BETA LIMITATIONS

- Shrinkwrap Body, Dynamic Body, glasses hihead bake, and headband transfer remain
  advanced or experimental workflows.
- Some legacy or unusual hair assets may still require manual testing.
- Manifest extraction requires the configured NBA 2K26 installation to contain
  manifest, mod.exe, and the game Oodle DLL.
- Close NBA 2K and tools that lock mod files before replacing live files.
- Open Output in Blender imports the completed IFF. Manual edits still need to be
  exported through the Blender add-on before they affect the game file.

SHARING AND BUILDING

- build_release.ps1 creates a clean portable EXE folder and ZIP.
- Public mode removes review-required game-derived assets.
- PrivateBeta mode requires -AcknowledgeGameDerivedAssets.
- CharacterModTool.iss can produce an installer when Inno Setup 6 is installed.
- Read ASSET_DISTRIBUTION_POLICY.txt and THIRD_PARTY_NOTICES.txt before sharing.

PRIVATE BETA TEST CHECKLIST

1. Test on a Windows account that has never run the source version.
2. Test with NBA 2K26 installed on a non-C drive or secondary Steam library.
3. Confirm Settings auto-detection and manual browsing.
4. Run Validator on a known-good player.
5. Run Full Swap into an empty folder and inspect every planned output.
6. Open the successful result in Blender.
7. Install hair while the target identity is unchanged.
8. Run Rename Character Package as the final step.
9. Test the final renamed package in game.
10. Use Diagnostics to verify a tester can produce a support ZIP.
