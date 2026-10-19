# How to use the WPF reference

The frozen WPF build (`nba2k27-launcher-wpf\release\quality-pass-qa-20261010`) is the **behavior spec** for the Rust + Tauri port.
The code doesn't have to match line for line. Behavior, timing and look do.

## Before you write any code for a feature

1. Open [index.md](index.md) and find the file for your feature area.
2. Read the whole sub-feature section. Pay most attention to **2. Timing and gating** and **3. Mechanism**, because that is where earlier ports failed live.
3. Before coding, write a short **Divergence list** in your plan or first message: every place where your Rust design will differ from WPF (different trigger, poll interval, readiness check, write order, label, default, or restore). For each one, give the reason. If there's no good reason, match WPF.
4. Use the **5. Rust must match** checklist as your acceptance test. Tick each bullet off, or explain why it doesn't apply (for example, V2 build only).

## When the reference is wrong or silent

- Items marked **UNVERIFIED** or "Not found in WPF sources" are gaps. Check the cited WPF source yourself, then **update the reference file** with what you found and a `path:line` citation. Don't just fix your own code.
- If the WPF behavior is known to be broken or abandoned (see `nba2k27-mod-launcher\ABANDONED_FEATURES.md`), say so in the file and don't copy the bug.
- Don't ask the user "how did WPF do X?" until you've checked the matching file and its cited sources.

## V2 / new game builds

WPF only supported the old game builds. Each file's **4. Build profiles** section says what WPF supported and the V2 equivalent where one is known.
When the V2 mechanism has to differ (new offsets, new structures), keep the **user-visible behavior and timing** the same as WPF. Change only the mechanism.

## Keeping it current

Add new findings to the matching file under the right sub-feature, with citations and the date.
Put large raw research in `artifacts\` and link to it from the reference file.
