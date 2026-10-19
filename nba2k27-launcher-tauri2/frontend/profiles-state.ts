// Profile rules ported from the WPF renderer (app.js profile dialog,
// removeProfileShortcut, profilesLocked). Folders are the real identity in
// WPF; the Rust library also keys the active profile by name, so names stay
// unique too.
type Profile = { name: string; folder: string };

const trimPath = (value: string) => value.trim().replace(/[\\/]+$/, "");
export const samePath = (a: string, b: string) =>
  !!trimPath(a) && trimPath(a).toLowerCase() === trimPath(b).toLowerCase();

/** The game's own `<game>\mods` folder: never edited or removed (WPF). */
export const defaultModsFolder = (gameRoot: string) =>
  `${trimPath(gameRoot)}\\mods`;
export const isDefaultProfile = (profile: Profile, gameRoot: string) =>
  !!trimPath(gameRoot) && samePath(profile.folder, defaultModsFolder(gameRoot));

export function folderLeafName(folder: string) {
  return trimPath(folder).split(/[\\/]/).at(-1) || "Mods";
}

/** WPF names a new profile after its folder; Rust keeps names unique. */
export function newProfileName(profiles: Profile[], folder: string) {
  const base = folderLeafName(folder).slice(0, 80);
  const names = new Set(profiles.map((p) => p.name.trim().toLowerCase()));
  if (!names.has(base.toLowerCase())) return base;
  for (let i = 2; ; i++) {
    const suffix = ` ${i}`;
    const name = `${base.slice(0, 80 - suffix.length)}${suffix}`;
    if (!names.has(name.toLowerCase())) return name;
  }
}

export const PROFILES_LOCKED =
  "Profiles are locked while your game session is active.";
/** Native session phases during which WPF locks every profile control. */
export const profilesLocked = (phase: string | undefined) =>
  phase === "waiting-for-game" || phase === "game-running";

export type ProfileDialog = {
  mode: "create" | "edit";
  original: Profile | null;
  folder: string;
  name: string;
};

/** Returns the WPF dialog error for a submit, or "" when it can be saved. */
export function profileDialogError(
  profiles: Profile[],
  dialog: ProfileDialog,
  locked: boolean,
) {
  if (locked) return PROFILES_LOCKED;
  const name = dialog.name.trim();
  if (!name || name.length > 80)
    return "Enter a profile name of 1–80 characters.";
  if (!dialog.folder.trim()) return "Choose a mod folder for this profile.";
  const original = dialog.original;
  if (
    profiles.some(
      (profile) =>
        samePath(profile.folder, dialog.folder) &&
        (dialog.mode === "create" || profile.name !== original?.name),
    )
  )
    return "That folder is already saved in another profile. Choose a different folder.";
  if (
    dialog.mode === "edit" &&
    !profiles.some((profile) => profile.name === original?.name)
  )
    return "This profile is no longer saved. Cancel and choose another profile.";
  if (
    profiles.some(
      (profile) =>
        profile.name !== original?.name &&
        profile.name.trim().toLowerCase() === name.toLowerCase(),
    )
  )
    return "A profile with that name already exists.";
  return "";
}

/** Applies a validated dialog to the saved profile list. */
export function profilesAfterDialog(
  profiles: Profile[],
  dialog: ProfileDialog,
) {
  const next = { name: dialog.name.trim(), folder: dialog.folder.trim() };
  return dialog.mode === "create"
    ? [...profiles, next]
    : profiles.map((profile) =>
        profile.name === dialog.original?.name ? next : profile,
      );
}
