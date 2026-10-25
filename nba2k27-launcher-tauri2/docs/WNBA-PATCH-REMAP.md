# WNBA priority remap after a game patch

Run the disk-only mapper after Steam replaces `NBA2K27.exe`:

```powershell
& .\scripts\Map-WNBA-Priority-Profile.ps1
```

The mapper never opens the game process and performs no game writes. It reconstructs the retained verified WNBA source methods, searches the installed executable with relocation fields masked, resolves the two required MSVC RTTI vtables, and requires one exact `.pdata` boundary for every method. Its report is written under `artifacts/wnba-priority-remap/` and is named with the executable SHA prefix.

The report is evidence for review, not launch authorization. Copy reviewed values into `native/wnba_priority_v2_profile.hpp` and the duplicated read-only vtable checks in `src/wnba_priority_categories.rs`, rebuild the combined native payload, update its pinned SHA, and run the native/Rust profile tests before staging a dev launcher. If any method, wrapper, vtable, or unwind boundary is ambiguous, the mapper exits with an error and the old launcher remains fail-closed.
