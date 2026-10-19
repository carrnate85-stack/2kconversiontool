import { finishModInstall } from "./mod-install-feedback";
import { Fragment, useEffect, useRef, useState, type ReactNode } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { call, garmentImage } from "./api";
import { BlacktopCatalog } from "./BlacktopCatalog";
import type { CatalogFeature } from "./NativeArena";
import type { Library, Settings, Media, Mod, Outfit, Garment } from "./types";
import { formatBytes, formatVideoLength, initialOutfit, label } from "./state";
import {
  filterModGroups,
  modGroups,
  move,
  normalizeModTags,
  sortModGroups,
  uniqueName,
  withGroupTags,
  type ModGroup,
  type ModSortKey,
} from "./library-state";
import { JukeboxNative } from "./JukeboxNative";
import { CameraLiveStatus } from "./CameraLiveStatus";
import {
  initialModDecisions,
  modInstallArgs,
  modInstallConflicts,
  modInstallCounts,
  modInstallIssueSummary,
  modInstallReplaceMessage,
  modInstallSource,
  modInstallSummary,
  setAllModDecisions,
  type ModInstallDecisions,
  type ModInstallMode,
  type ModInstallPlan,
  type ModInstallResult,
} from "./mod-install";
import {
  cameraNameSlot,
  cameraSetupError,
  renameCamera,
  resetCameraName,
} from "./camera-editing";
import {
  startupVideoImportCanCancel,
  startupVideoImportForOperation,
  startupVideoImportLabel,
  startupVideoImportOperationId,
  startupVideoImportPercent,
  type StartupVideoImportProgress,
} from "./startup-video-import";
import {
  PROFILES_LOCKED,
  isDefaultProfile,
  newProfileName,
  profileDialogError,
  profilesAfterDialog,
  profilesLocked,
  samePath,
  type ProfileDialog,
} from "./profiles-state";
import { ModalDialog, OutfitItemIcon, UiIcon } from "./wpf/WpfShell";
import { FitPreviewDialog } from "./FitPreview";
import { GarmentImg, useClothingCache } from "./PreviewCache";
import { outfitAccessorySlots, outfitPieceSlots } from "./OverlayOutfitEditor";
import "./ModLibrary.css";

type Shared = {
  library: Library;
  save: (patch: Partial<Library>) => Promise<void>;
  settings: Settings;
  change: (patch: Partial<Settings>) => void;
  status: (text: string) => void;
};
function Box({
  title,
  children,
  actions,
}: {
  title: string;
  children: React.ReactNode;
  actions?: React.ReactNode;
}) {
  return (
    <section className="card">
      <header>
        <h3>{title}</h3>
        {actions}
      </header>
      <div className="card-content">{children}</div>
    </section>
  );
}
function useActions(status: (text: string) => void) {
  const [error, setError] = useState(""),
    [busy, setBusy] = useState(false);
  const running = useRef(false);
  async function run(fn: () => Promise<void>) {
    if (running.current) return;
    running.current = true;
    setBusy(true);
    setError("");
    try {
      await fn();
    } catch (e) {
      setError(String(e));
      status("Action not completed");
    } finally {
      running.current = false;
      setBusy(false);
    }
  }
  return { run, busy, error };
}
function Feedback({ error }: { error: string }) {
  return error ? (
    <p className="error" role="alert">
      {error}
    </p>
  ) : null;
}
// WPF Profiles page: a radio list where choosing a row makes it the active
// profile, a picker-first New profile dialog, an Edit dialog, and Remove for
// inactive non-default rows. Every control locks during a game session.
export function Profiles(p: Shared) {
  const [dialog, setDialog] = useState<ProfileDialog | null>(null),
    [dialogError, setDialogError] = useState(""),
    [picking, setPicking] = useState(false),
    [phase, setPhase] = useState<string | undefined>();
  const a = useActions(p.status);
  const profiles = p.library.profiles;
  const locked = profilesLocked(phase);
  const active = profiles.find((x) => x.name === p.settings.profile) ?? null;
  const nameInput = useRef<HTMLInputElement>(null);
  useEffect(() => {
    let alive = true,
      polling = false;
    async function poll() {
      if (polling) return;
      polling = true;
      try {
        const next = await call<{ phase?: string }>("native-session-status");
        if (alive) setPhase(next.phase);
      } catch {
        // Advisory only; Play shows session errors.
      } finally {
        polling = false;
      }
    }
    void poll();
    const timer = setInterval(() => void poll(), 1500);
    return () => {
      alive = false;
      clearInterval(timer);
    };
  }, []);
  const dialogOpen = !!dialog;
  useEffect(() => {
    if (!dialogOpen) return;
    nameInput.current?.focus();
    nameInput.current?.select();
  }, [dialogOpen]);
  useEffect(() => {
    if (!dialog) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !a.busy && !picking) setDialog(null);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [dialog, a.busy, picking]);
  function activate(profile: (typeof profiles)[number]) {
    if (locked || profile.name === p.settings.profile) return;
    if (!profile.folder.trim()) {
      p.status("Choose a mod folder before using this profile");
      return;
    }
    p.change({ profile: profile.name, modsRoot: profile.folder });
    p.status(`${profile.name} selected.`);
  }
  async function pickFolder() {
    setPicking(true);
    try {
      return await call<string | null>("pick-folder");
    } finally {
      setPicking(false);
    }
  }
  const create = () =>
    void a.run(async () => {
      if (locked || picking || dialog) return;
      const folder = await pickFolder();
      if (!folder) return;
      if (profiles.some((x) => samePath(x.folder, folder))) {
        p.status("That folder is already saved. Select it from the list.");
        return;
      }
      setDialogError("");
      setDialog({
        mode: "create",
        original: null,
        folder,
        name: newProfileName(profiles, folder),
      });
    });
  function edit(profile: (typeof profiles)[number]) {
    if (locked || dialog) return;
    setDialogError("");
    setDialog({
      mode: "edit",
      original: profile,
      folder: profile.folder,
      name: profile.name,
    });
  }
  const changeFolder = () =>
    void a.run(async () => {
      if (!dialog || locked || picking) return;
      const folder = await pickFolder();
      if (folder) {
        setDialog((old) => (old ? { ...old, folder } : old));
        setDialogError("");
      }
    });
  const submit = () =>
    void a.run(async () => {
      if (!dialog || picking) return;
      const problem = profileDialogError(profiles, dialog, locked);
      if (problem) {
        setDialogError(problem);
        return;
      }
      const name = dialog.name.trim(),
        folder = dialog.folder.trim();
      await p.save({ profiles: profilesAfterDialog(profiles, dialog) });
      const creating = dialog.mode === "create";
      if (creating || dialog.original?.name === p.settings.profile)
        p.change({ profile: name, modsRoot: folder });
      setDialog(null);
      p.status(creating ? `${name} created and selected.` : "Profile updated.");
    });
  const remove = (profile: (typeof profiles)[number]) =>
    void a.run(async () => {
      if (
        locked ||
        profile.name === p.settings.profile ||
        isDefaultProfile(profile, p.settings.gameRoot)
      )
        return;
      await p.save({
        profiles: profiles.filter((x) => x.name !== profile.name),
      });
      p.status("Profile shortcut removed. No mod files were deleted.");
    });
  return (
    <>
      <Feedback error={a.error} />
      <div className="toolbar">
        <div>
          <p>Keep separate mod collections and choose one for Play.</p>
        </div>
      </div>
      <div className="grid two profiles-grid">
        <article className="card form-card profiles-saved-card">
          <div className="card-title profile-card-heading">
            <div className="profile-heading-main">
              <span className="profile-heading-icon" aria-hidden="true">
                <UiIcon name="profiles" />
              </span>
              <div>
                <h2>Saved profiles</h2>
                <p>Your mod collections</p>
              </div>
            </div>
            <span className="quiet" id="profileCount">
              {profiles.length} PROFILE{profiles.length === 1 ? "" : "S"}
            </span>
          </div>
          <div
            id="profileRows"
            className="profile-list"
            role="radiogroup"
            aria-label="Active mod profile"
          >
            {profiles.map((profile) => {
              const isActive = profile.name === p.settings.profile,
                isDefault = isDefaultProfile(profile, p.settings.gameRoot);
              return (
                <div key={profile.name} className="profile-row">
                  <label className="profile-choice">
                    <input
                      type="radio"
                      name="activeModsProfile"
                      value={profile.folder}
                      checked={isActive}
                      disabled={locked || a.busy}
                      onChange={() => activate(profile)}
                    />
                    <span>
                      <strong title={profile.name}>{profile.name}</strong>
                      <small title={profile.folder}>
                        {profile.folder ||
                          "Folder not selected — edit this profile"}
                      </small>
                    </span>
                  </label>
                  <span className="profile-row-actions">
                    <span className="profile-row-buttons">
                      {!isDefault && (
                        <button
                          type="button"
                          className="secondary small profile-edit"
                          disabled={locked || a.busy}
                          aria-label={`Edit ${profile.name} profile`}
                          onClick={() => edit(profile)}
                        >
                          Edit
                        </button>
                      )}
                      {!isActive && !isDefault && (
                        <button
                          type="button"
                          className="secondary small profile-remove"
                          disabled={locked || a.busy}
                          aria-label={`Remove ${profile.name} profile`}
                          onClick={(e) => {
                            e.preventDefault();
                            e.stopPropagation();
                            remove(profile);
                          }}
                        >
                          Remove
                        </button>
                      )}
                    </span>
                  </span>
                </div>
              );
            })}
          </div>
          <button
            type="button"
            className="primary"
            id="addModsProfile"
            disabled={locked || a.busy || picking || !!dialog}
            onClick={create}
          >
            <UiIcon name="plus" />
            New profile
          </button>
          <p
            className="camera-help profile-selection-note"
            id="profilesSelectionStatus"
            role="status"
          >
            {locked
              ? PROFILES_LOCKED
              : "Choose a profile to make it active. Your selection is saved for the next Play session."}
          </p>
        </article>
        <article className="card profile-guide-card">
          <div className="card-title profile-card-heading">
            <div className="profile-heading-main">
              <span className="profile-heading-icon" aria-hidden="true">
                <UiIcon name="book" />
              </span>
              <div>
                <h2>How profiles work</h2>
                <p>A quick guide to keep things clear.</p>
              </div>
            </div>
            <span className="pill amber">
              <UiIcon name="clock" />
              SESSION ONLY
            </span>
          </div>
          <div className="notice profile-active-path">
            <small>Currently selected profile</small>
            <strong id="activeProfileName">
              {active?.name || "Active profile"}
            </strong>
            <span id="activeProfilePath">
              {p.settings.modsRoot || "No profile selected."}
            </span>
          </div>
          <div className="boundary profile-guide-list">
            {[
              [
                "No merging",
                "Each folder remains an independent mod collection.",
              ],
              [
                "Native game support",
                "The selected folder is temporarily presented at the game's normal mods path.",
              ],
              [
                "Automatic restoration",
                "Your original mods folder returns after NBA 2K27 closes.",
              ],
              [
                "Safe removal",
                "Remove forgets the list entry and never deletes its files.",
              ],
            ].map(([title, copy]) => (
              <div key={title}>
                <span className="check">✓</span>
                <p>
                  <strong>{title}</strong>
                  <small>{copy}</small>
                </p>
              </div>
            ))}
          </div>
          {!active && (
            <p className="camera-help profiles-warning" role="status">
              The saved active profile is missing. Choose a profile on the left
              to repair it.
            </p>
          )}
        </article>
      </div>
      {dialog && (
        <ModalDialog
          id="profileDialog"
          className="profile-dialog"
          labelledBy="profileDialogTitle"
          onCancel={() => {
            if (!a.busy && !picking) setDialog(null);
          }}
        >
          <form
            id="profileDialogForm"
            onSubmit={(e) => {
              e.preventDefault();
              submit();
            }}
          >
            <h2 id="profileDialogTitle">
              {dialog.mode === "create" ? "New profile" : "Edit profile"}
            </h2>
            <label id="profileDialogFolderLabel">Mod folder</label>
            <div className="profile-dialog-folder">
              <p
                id="profileDialogPath"
                className="profile-dialog-path"
                aria-labelledby="profileDialogFolderLabel"
              >
                {dialog.folder}
              </p>
              <button
                type="button"
                className="secondary"
                id="chooseProfileDialogFolder"
                disabled={locked || a.busy || picking}
                onClick={changeFolder}
              >
                Change folder
              </button>
            </div>
            <label htmlFor="profileDialogName">Profile name</label>
            <input
              id="profileDialogName"
              ref={nameInput}
              aria-label="Profile name"
              maxLength={80}
              required
              autoComplete="off"
              disabled={a.busy}
              value={dialog.name}
              onChange={(e) =>
                setDialog((old) =>
                  old ? { ...old, name: e.target.value } : old,
                )
              }
            />
            {dialogError && (
              <p
                id="profileDialogError"
                className="profile-dialog-error"
                role="alert"
              >
                {dialogError}
              </p>
            )}
            <div className="profile-dialog-actions">
              <button
                type="button"
                className="secondary"
                id="cancelProfileDialog"
                disabled={a.busy || picking}
                onClick={() => setDialog(null)}
              >
                Cancel
              </button>
              <button
                type="submit"
                className="primary"
                id="saveProfileDialog"
                disabled={locked || a.busy || picking}
              >
                {dialog.mode === "create" ? "Create profile" : "Save changes"}
              </button>
            </div>
          </form>
        </ModalDialog>
      )}
    </>
  );
}
function ModIcon({ name, size = 18 }: { name: string; size?: number }) {
  const paths: Record<string, ReactNode> = {
    folder: <path d="M3 7h7l2-3h9v16H3Z" />,
    file: <path d="M6 2h9l5 5v15H6Zm9 0v5h5" />,
    player: (
      <>
        <circle cx="12" cy="7" r="4" />
        <path d="M4 22v-4a8 8 0 0 1 16 0v4" />
      </>
    ),
    arena: (
      <>
        <rect x="2" y="5" width="20" height="14" />
        <path d="M12 5v14M2 9h4v6H2m20-6h-4v6h4" />
        <circle cx="12" cy="12" r="3" />
      </>
    ),
    uniform: <path d="M7 3 2 6l3 5 2-1v12h10V10l2 1 3-5-5-3-2 3H9Z" />,
    ball: (
      <>
        <circle cx="12" cy="12" r="9" />
        <path d="M3 12h18M12 3v18M5.6 5.6c3.5 3.5 3.5 9.3 0 12.8m12.8-12.8c-3.5 3.5-3.5 9.3 0 12.8" />
      </>
    ),
    search: (
      <>
        <circle cx="11" cy="11" r="7" />
        <path d="m20 20-4-4" />
      </>
    ),
    refresh: <path d="M20 11a8 8 0 1 0-2 6m2-12v6h-6" />,
    download: <path d="M12 3v12m-5-5 5 5 5-5M4 21h16" />,
    open: <path d="M14 3h7v7m0-7L10 14M19 14v7H3V5h7" />,
  };
  return (
    <svg
      className="mod-icon"
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {paths[name] ?? paths.file}
    </svg>
  );
}
function modGroupIcon(group: ModGroup) {
  if (group.category === "Player") return "player";
  if (group.category === "Arena") return "arena";
  if (group.category === "Uniform") return "uniform";
  return /^balls[\\/]/i.test(group.title) ? "ball" : "file";
}
// app.js createLauncherIcon names for each mod category.
const WPF_MOD_ICONS: Record<string, string> = {
  player: "players",
  arena: "blacktop",
  uniform: "uniform",
  ball: "ball",
  file: "file",
};
const MOD_SORTS: [ModSortKey, string, string][] = [
  ["title", "Mod / asset tree", "mod name"],
  ["category", "Category", "category"],
  ["tags", "Search tags", "search tags"],
  ["size", "Size", "file size"],
];
// WPF lists the first 5,000 matching entries.
const MOD_PAGE = 5000;
export function Mods(
  p: Shared & {
    mods: Mod[];
    busy: boolean;
    scan: () => Promise<void>;
    scanned?: boolean;
    scanError?: string;
  },
) {
  const [query, setQuery] = useState(""),
    [expanded, setExpanded] = useState(new Set<string>()),
    [sort, setSort] = useState<{ key: ModSortKey; direction: 1 | -1 }>({
      key: "title",
      direction: 1,
    }),
    [editing, setEditing] = useState<ModGroup | null>(null),
    [draft, setDraft] = useState(""),
    [limit, setLimit] = useState(MOD_PAGE),
    [install, setInstall] = useState<ModInstallState | null>(null);
  const a = useActions(p.status);
  const tagInput = useRef<HTMLInputElement>(null);
  const tableBody = useRef<HTMLDivElement>(null);
  // WPF list-view-state: a new search, sort or profile starts at the top;
  // re-renders in the same context keep the scroll position.
  useEffect(() => {
    tableBody.current?.scrollTo?.({ top: 0, left: 0 });
  }, [query, sort.key, sort.direction, p.settings.modsRoot]);
  const root = p.settings.modsRoot;
  useEffect(() => {
    setExpanded(new Set());
    setLimit(MOD_PAGE);
    setEditing(null);
  }, [root]);
  useEffect(() => {
    if (!editing) return;
    tagInput.current?.focus();
    tagInput.current?.select();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !a.busy) setEditing(null);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [editing, a.busy]);
  useEffect(() => {
    if (!install) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !a.busy) closeInstall();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });
  // WPF mods:install: choose files/archives or one folder, route every file
  // through the stock manifest, then review conflicts before anything is
  // replaced. Plans with routing issues install nothing.
  function closeInstall() {
    const reviewing = install?.stage === "review";
    setInstall(null);
    if (reviewing) void call("mod-install-cancel").catch(() => undefined);
  }
  const chooseInstall = (mode: ModInstallMode) =>
    void a.run(async () => {
      const plan = await call<ModInstallPlan | { cancelled: true }>(
        "mod-install-plan",
        { mode },
      );
      if (plan.cancelled) return;
      if (plan.issues.length || !plan.planId) {
        setInstall({ stage: "issues", plan });
        p.status(modInstallIssueSummary(plan.issues));
        return;
      }
      if (plan.replacements) {
        setInstall({
          stage: "review",
          plan,
          decisions: initialModDecisions(plan),
        });
        return;
      }
      await commitInstall(plan, {});
    });
  async function commitInstall(
    plan: ModInstallPlan,
    decisions: ModInstallDecisions,
  ) {
    // The backend consumes the reviewed plan whatever the outcome, so a
    // failure closes the review and the error shows on the page.
    const result = await call<ModInstallResult>(
      "mod-install-commit",
      modInstallArgs(plan, decisions),
    ).finally(() => setInstall(null));
    await finishModInstall(modInstallSummary(result), p.scan, p.status);
  }
  const groups = sortModGroups(
    modGroups(p.mods, p.library.tags, root),
    sort.key,
    sort.direction,
  );
  const matching = filterModGroups(groups, query);
  const shown = matching.slice(0, limit);
  const scanning = p.busy || a.busy;
  const openItem = (mod: Mod, action: "file" | "folder") =>
    void a.run(async () => {
      await call("open-mod-item", { path: mod.path, action });
    });
  const toggle = (group: ModGroup) => {
    if (group.files.length === 1) return openTags(group);
    setExpanded((old) => {
      const next = new Set(old);
      next.has(group.id) ? next.delete(group.id) : next.add(group.id);
      return next;
    });
  };
  function openTags(group: ModGroup) {
    setDraft(group.tags.join(", "));
    setEditing(group);
  }
  const saveTags = () =>
    void a.run(async () => {
      if (!editing) return;
      const value = normalizeModTags(draft);
      await p.save({
        tags: withGroupTags(p.library.tags, root, editing, value),
      });
      setEditing(null);
      p.status("Mod tags saved.");
    });
  const actions = (mod: Mod) => (
    <div className="mod-file-actions">
      {(
        [
          ["file", "Open", "Open this mod file", "file"],
          ["folder", "Folder", "Show this mod in its folder", "profiles"],
        ] as const
      ).map(([action, text, title, icon]) => (
        <button
          key={action}
          type="button"
          className="secondary small"
          title={title}
          aria-label={`${title}: ${mod.path}`}
          onClick={(e) => {
            e.stopPropagation();
            openItem(mod, action);
          }}
        >
          <UiIcon name={icon} />
          {text}
        </button>
      ))}
    </div>
  );
  return (
    <>
      <Feedback error={a.error} />
      <section className="card mod-library-controls" aria-label="Mod library controls">
        <div className="mod-folder-toolbar">
          <span className="mod-folder-label" data-ui-icon="profiles">
            <UiIcon name="profiles" />
            Current mod folder
          </span>
          <p
            id="modsRootLabel"
            className={root ? "mod-folder-path" : "mod-folder-path missing"}
            title={root}
          >
            {root || "No mod folder selected — choose one in Profiles"}
          </p>
          <div className="mod-library-actions">
            <button
              className="secondary"
              id="openMods"
              data-ui-icon="profiles"
              disabled={!root}
              onClick={() =>
                void a.run(async () => {
                  await call("open-mods-folder");
                })
              }
            >
              <UiIcon name="profiles" />
              Open folder
            </button>
            <button
              className="secondary"
              id="installMods"
              data-ui-icon="download"
              disabled={!root || scanning}
              title="Install individual files, a whole folder, ZIP, or RAR package"
              onClick={() =>
                void a.run(async () => {
                  // WPF refuses before any dialog while the game is running.
                  await call("mod-install-ready");
                  setInstall({ stage: "choose" });
                })
              }
            >
              <UiIcon name="download" />
              <span id="installModsLabel">
                {a.busy && install ? "Installing…" : "Install mods"}
              </span>
            </button>
            <button
              className="primary"
              id="scanMods"
              data-ui-icon="refresh"
              disabled={scanning}
              onClick={() => void a.run(p.scan)}
            >
              <UiIcon name="refresh" />
              <span id="scanModsLabel">
                {scanning ? "Scanning…" : "Scan mods"}
              </span>
            </button>
          </div>
        </div>
        <div className="mod-search-toolbar">
          <label className="mod-search-field" htmlFor="modSearch">
            <span data-ui-icon="search" aria-hidden="true">
              <UiIcon name="search" />
            </span>
            <input
              id="modSearch"
              aria-label="Search mods"
              placeholder="Try png10000, arena_999, english, Player…"
              autoComplete="off"
              value={query}
              onChange={(e) => {
                setQuery(e.target.value);
                setLimit(MOD_PAGE);
              }}
            />
          </label>
          <p id="modResultSummary" role="status">
            {p.scanError
              ? `Scan failed: ${p.scanError}`
              : !p.mods.length && !p.scanned
                ? "Scan the folder to search its files."
                : matching.length > shown.length
                  ? `${matching.length.toLocaleString()} matches · showing the first ${shown.length.toLocaleString()}`
                  : `${matching.length.toLocaleString()} matching mod entries · ${p.mods.length.toLocaleString()} files total`}
          </p>
        </div>
      </section>
      <div className="card table-card">
        <div className="table-head">
          {MOD_SORTS.map(([key, text, aria]) => {
            const active = sort.key === key;
            return (
              <button
                key={key}
                type="button"
                data-mod-sort={key}
                aria-label={`Sort by ${aria}`}
                aria-pressed={active}
                data-direction={
                  active
                    ? sort.direction === 1
                      ? "ascending"
                      : "descending"
                    : undefined
                }
                onClick={() =>
                  setSort((old) => ({
                    key,
                    direction:
                      old.key === key ? (old.direction === 1 ? -1 : 1) : 1,
                  }))
                }
              >
                {text} <span aria-hidden="true">↕</span>
              </button>
            );
          })}
          <span>Actions</span>
        </div>
        <div id="modRows" className="table-body" ref={tableBody}>
          {shown.map((group) => {
            const open = expanded.has(group.id),
              bundle = group.files.length > 1;
            return (
              <Fragment key={group.id}>
                <div
                  className="mod-row"
                  data-view-key={group.id}
                  onClick={() => toggle(group)}
                >
                  <div className="mod-asset-cell">
                    <button
                      type="button"
                      className="mod-tree-toggle"
                      disabled={!bundle}
                      aria-label={`${open ? "Collapse" : "Expand"} ${group.title}`}
                      aria-expanded={bundle ? open : undefined}
                      onClick={(e) => {
                        e.stopPropagation();
                        toggle(group);
                      }}
                    >
                      {bundle ? (open ? "▼" : "▶") : "•"}
                    </button>
                    <UiIcon
                      name={WPF_MOD_ICONS[modGroupIcon(group)]}
                      className="mod-asset-icon"
                    />
                    <div>
                      <div className="mod-asset-name" title={group.title}>
                        {group.title}
                      </div>
                      <small>
                        {bundle
                          ? `${group.files.length} bundled files`
                          : "Single file"}
                      </small>
                    </div>
                  </div>
                  <span className="category">{group.category}</span>
                  <button
                    type="button"
                    className={
                      group.tags.length
                        ? "mod-tag-button"
                        : "mod-tag-button untagged"
                    }
                    title={
                      group.tags.length
                        ? group.tags.join(", ")
                        : "Add searchable tags"
                    }
                    aria-label={`Edit tags for ${group.title}`}
                    onClick={(e) => {
                      e.stopPropagation();
                      openTags(group);
                    }}
                  >
                    {group.tags.length ? group.tags.join(", ") : "Add tags"}
                  </button>
                  <span className="size">{formatBytes(group.bytes)}</span>
                  {actions(group.base)}
                </div>
                {bundle &&
                  open &&
                  group.files.map((file) => (
                    <div
                      key={file.path}
                      className="mod-dependency-row"
                      data-view-key={`dependency:${file.path}`}
                    >
                      <span
                        className="dependency-path mono"
                        title={file.path.replaceAll("\\", "/")}
                      >
                        └ {file.path.replaceAll("\\", "/")}
                      </span>
                      <span>
                        {file === group.base ? "Base asset" : "Dependency"}
                      </span>
                      <span />
                      <span>{formatBytes(file.bytes)}</span>
                      {actions(file)}
                    </div>
                  ))}
              </Fragment>
            );
          })}
          {!matching.length && (
            <div className="empty">
              {!root
                ? "Choose a mod folder in Profiles, then scan."
                : p.mods.length
                  ? "No mod files match that search."
                  : p.scanned && !p.scanError
                    ? "The selected folder is empty."
                    : "Scan the folder to build the local mod catalog."}
            </div>
          )}
          {matching.length > limit && (
            <button
              className="secondary mod-show-more"
              onClick={() => setLimit(limit + MOD_PAGE)}
            >
              Show {Math.min(MOD_PAGE, matching.length - limit)} more entries
            </button>
          )}
        </div>
        <div className="mod-library-footer">
          <span id="modShownSummary">
            {p.mods.length
              ? `${shown.length.toLocaleString()} of ${matching.length.toLocaleString()} mod entries listed`
              : "No catalog loaded"}
          </span>
          <span>Click tags to edit · expand grouped files</span>
        </div>
      </div>
      {editing && (
        <div
          className="modal-overlay"
          id="modTagModal"
          onMouseDown={(e) => {
            if (e.target === e.currentTarget && !a.busy) setEditing(null);
          }}
        >
          <section
            className="team-editor mod-tag-editor"
            role="dialog"
            aria-modal="true"
            aria-labelledby="modTagTitle"
          >
            <div className="team-editor-head">
              <div>
                <p className="eyebrow">MOD LIBRARY TAGS</p>
                <h2 id="modTagTitle">{editing.stem || editing.base.name}</h2>
                <small className="mono" id="modTagIdentity">
                  {editing.title}
                </small>
              </div>
              <button
                className="icon-button"
                id="closeModTagEditor"
                aria-label="Close"
                disabled={a.busy}
                onClick={() => setEditing(null)}
              >
                ×
              </button>
            </div>
            <label className="mod-tag-field">
              Tags
              <input
                id="modTagInput"
                ref={tagInput}
                maxLength={500}
                placeholder="Player name, team, era, project…"
                value={draft}
                onChange={(e) => setDraft(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") saveTags();
                }}
              />
              <small>
                Separate tags with commas. Search matches these tags and every
                dependency filename in the bundle.
              </small>
            </label>
            {a.error && (
              <p className="error" role="alert">
                {a.error}
              </p>
            )}
            <div className="notice" id="modTagBundleInfo">
              {editing.files.length > 1
                ? `${editing.files.length} files are bundled under this mod entry. The tags apply to the whole bundle.`
                : "This mod entry contains one file."}
            </div>
            <div className="team-editor-actions">
              <button
                className="secondary"
                id="cancelModTagEdit"
                disabled={a.busy}
                onClick={() => setEditing(null)}
              >
                Cancel
              </button>
              <button
                className="primary"
                id="saveModTagEdit"
                disabled={a.busy}
                onClick={saveTags}
              >
                {a.busy ? "Saving…" : "Save tags"}
              </button>
            </div>
          </section>
        </div>
      )}
      {install && (
        <div
          className="modal-backdrop"
          onMouseDown={(e) => {
            if (e.target === e.currentTarget && !a.busy) closeInstall();
          }}
        >
          <ModInstallDialog
            state={install}
            busy={a.busy}
            error={a.error}
            close={closeInstall}
            choose={chooseInstall}
            decide={(decisions) =>
              install.stage === "review" &&
              setInstall({ ...install, decisions })
            }
            commit={() =>
              install.stage === "review" &&
              void a.run(() => commitInstall(install.plan, install.decisions))
            }
          />
        </div>
      )}
    </>
  );
}
export type ModInstallState =
  | { stage: "choose" }
  | { stage: "review"; plan: ModInstallPlan; decisions: ModInstallDecisions }
  | { stage: "issues"; plan: ModInstallPlan };
export function ModInstallDialog(p: {
  state: ModInstallState;
  busy: boolean;
  error: string;
  close: () => void;
  choose: (mode: ModInstallMode) => void;
  decide: (decisions: ModInstallDecisions) => void;
  commit: () => void;
}) {
  const s = p.state;
  const plural = (count: number, word: string) =>
    `${count.toLocaleString()} ${word}${count === 1 ? "" : "s"}`;
  const header = (title: string, detail: string) => (
    <header>
      <div>
        <p className="eyebrow">INSTALL MODS</p>
        <h2 id="mod-install-title">{title}</h2>
        <small className="mod-tag-identity">{detail}</small>
      </div>
      <button
        className="mod-tag-close"
        aria-label="Close"
        disabled={p.busy}
        onClick={p.close}
      >
        ×
      </button>
    </header>
  );
  const error = p.error ? (
    <p className="error" role="alert">
      {p.error}
    </p>
  ) : null;
  if (s.stage === "choose")
    return (
      <section
        className="modal mod-install-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="mod-install-title"
      >
        {header(
          "Choose what you want to install.",
          "Files go into the active mods profile only; game files are never changed.",
        )}
        <div className="modal-body">
          <div className="mod-install-choices">
            <button disabled={p.busy} onClick={() => p.choose("files")}>
              <ModIcon name="file" size={22} />
              <strong>Files or archives</strong>
              <small>IFF, BIN, DDS, ZIP or RAR — several at once</small>
            </button>
            <button disabled={p.busy} onClick={() => p.choose("folder")}>
              <ModIcon name="folder" size={22} />
              <strong>Folder</strong>
              <small>One mod package folder, scanned recursively</small>
            </button>
          </div>
          {error}
          <div className="notice">
            ZIP and RAR archives are inspected before extraction. A folder is
            scanned recursively.
          </div>
        </div>
        <footer>
          <p>
            {p.busy
              ? "Checking the selection…"
              : "Nothing is installed until you confirm."}
          </p>
          <div className="mod-tag-actions">
            <button disabled={p.busy} onClick={p.close}>
              Cancel
            </button>
          </div>
        </footer>
      </section>
    );
  const plan = s.plan;
  const skipped = plan.ignored.length
    ? ` ${plural(plan.ignored.length, "documentation, preview or program file")} skipped.`
    : "";
  if (s.stage === "issues")
    return (
      <section
        className="modal mod-install-dialog"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="mod-install-title"
      >
        {header(
          `${plural(plan.issues.length, "file")} could not be routed`,
          `${modInstallSource(plan)} · nothing was installed`,
        )}
        <div className="modal-body">
          <ul className="mod-install-list">
            {plan.issues.map((issue, index) => (
              <li key={`${issue.name}-${index}`}>
                <span className="mod-install-path">{issue.name}</span>
                <small>{issue.reason}</small>
              </li>
            ))}
          </ul>
          <div className="notice">
            Every file must match the stock manifest, a known package folder
            structure, or a PNG player file. Fix or remove these files, then
            install again.{skipped}
          </div>
        </div>
        <footer>
          <p>The mods folder was not changed.</p>
          <div className="mod-tag-actions">
            <button className="primary" onClick={p.close}>
              Close
            </button>
          </div>
        </footer>
      </section>
    );
  const conflicts = modInstallConflicts(plan),
    counts = modInstallCounts(plan, s.decisions),
    added = plan.items.filter((item) => !item.existing);
  return (
    <section
      className="modal mod-install-dialog"
      role="dialog"
      aria-modal="true"
      aria-labelledby="mod-install-title"
    >
      {header(
        "Replace existing mod files?",
        `${modInstallReplaceMessage(conflicts.length)} ${modInstallSource(plan)} → ${plan.root}`,
      )}
      <div className="modal-body">
        <div className="mod-install-bulk">
          <span>Choose Replace or Keep existing for each file.</span>
          <button
            disabled={p.busy}
            onClick={() => p.decide(setAllModDecisions(plan, "replace"))}
          >
            Replace all
          </button>
          <button
            disabled={p.busy}
            onClick={() => p.decide(setAllModDecisions(plan, "keep"))}
          >
            Keep all
          </button>
        </div>
        <ul className="mod-install-list">
          {conflicts.map((item) => {
            const choice = s.decisions[item.logicalPath] ?? "keep";
            return (
              <li key={item.logicalPath} className="mod-install-conflict">
                <span className="mod-install-path" title={item.logicalPath}>
                  {item.logicalPath}
                </span>
                <small>
                  Existing {formatBytes(item.existing?.size ?? 0)} · new{" "}
                  {formatBytes(item.size)}
                </small>
                <div
                  className="mod-install-choice"
                  role="group"
                  aria-label={`Decision for ${item.logicalPath}`}
                >
                  {(["replace", "keep"] as const).map((value) => (
                    <button
                      key={value}
                      aria-pressed={choice === value}
                      disabled={p.busy}
                      onClick={() =>
                        p.decide({ ...s.decisions, [item.logicalPath]: value })
                      }
                    >
                      {value === "replace" ? "Replace" : "Keep existing"}
                    </button>
                  ))}
                </div>
              </li>
            );
          })}
        </ul>
        {added.length > 0 && (
          <details className="mod-install-new">
            <summary>{plural(added.length, "new file")} will be added</summary>
            <ul className="mod-install-list">
              {added.slice(0, 200).map((item) => (
                <li key={item.logicalPath}>
                  <span className="mod-install-path">{item.logicalPath}</span>
                  <small>{formatBytes(item.size)}</small>
                </li>
              ))}
            </ul>
          </details>
        )}
        {error}
        <div className="notice">
          The package was routed from its verified folder structure, the stock
          manifest, and safe player-file rules. Replaced files cannot be
          recovered by the launcher.{skipped}
        </div>
      </div>
      <footer>
        <p>
          {counts.install.toLocaleString()} to install · {counts.replace}{" "}
          replaced · {counts.keep} kept
        </p>
        <div className="mod-tag-actions">
          <button disabled={p.busy} onClick={p.close}>
            Cancel
          </button>
          <button
            className="primary"
            disabled={p.busy || counts.install === 0}
            onClick={p.commit}
          >
            {p.busy
              ? "Installing…"
              : counts.replace
                ? "Replace and install"
                : "Install"}
          </button>
        </div>
      </footer>
    </section>
  );
}
export function Jukebox(
  p: Shared & { media: Media[]; setMedia: (m: Media[]) => void },
) {
  return (
    <JukeboxNative
      status={p.status}
      freestylePlaylist={p.settings.usePlaylistInFreestyle !== false}
      onFreestylePlaylist={(on) => {
        p.change({ usePlaylistInFreestyle: on });
        p.status(
          on
            ? "Playlist in Freestyle enabled for next launch."
            : "Freestyle will use its stock music next launch.",
        );
      }}
    />
  );
}
export function Presentation(
  p: Shared & { media: Media[]; setMedia: (m: Media[]) => void },
) {
  const [cameras, setCameras] = useState(p.library.cameras),
    [playing, setPlaying] = useState<Media | null>(null),
    [videoImport, setVideoImport] = useState<StartupVideoImportProgress | null>(
      null,
    ),
    [videoImportBusy, setVideoImportBusy] = useState(false),
    [videoImportError, setVideoImportError] = useState(""),
    [videoEdit, setVideoEdit] = useState<{
      action: "rename" | "remove";
      video: Media;
      name: string;
    } | null>(null),
    [videoEditBusy, setVideoEditBusy] = useState(false),
    [videoEditError, setVideoEditError] = useState("");
  const acceptVideoImportEvents = useRef(false);
  const videoImportId = useRef("");
  const a = useActions(p.status);
  useEffect(() => setCameras(p.library.cameras), [p.library.cameras]);
  useEffect(() => {
    let disposed = false;
    let off: (() => void) | undefined;
    listen<unknown>("startup-video-import-progress", (event) => {
      if (!acceptVideoImportEvents.current) return;
      const update = startupVideoImportForOperation(
        event.payload,
        videoImportId.current,
      );
      if (!update) return;
      setVideoImport(update);
    }).then((unlisten) => {
      if (disposed) unlisten();
      else off = unlisten;
    });
    return () => {
      disposed = true;
      acceptVideoImportEvents.current = false;
      off?.();
    };
  }, []);
  async function importStartupVideo() {
    if (videoImportBusy) return;
    let operationId: string;
    try {
      operationId = startupVideoImportOperationId(crypto.randomUUID());
    } catch (error) {
      setVideoImportError(String(error));
      p.status("Startup video import not started");
      return;
    }
    setVideoImportBusy(true);
    setVideoImportError("");
    setVideoImport(null);
    videoImportId.current = operationId;
    acceptVideoImportEvents.current = true;
    try {
      const result = await call<Media[] | null>("import-media", {
        kind: "Videos",
        operationId,
      });
      if (result) {
        p.setMedia(result);
        p.status("Startup video imported into the portable library");
      }
    } catch (error) {
      const text = String(error);
      if (text.toLowerCase().includes("cancel")) {
        p.status("Startup video import cancelled; existing videos unchanged");
      } else {
        setVideoImportError(text);
        p.status("Startup video import not completed");
      }
    } finally {
      acceptVideoImportEvents.current = false;
      videoImportId.current = "";
      setVideoImport(null);
      setVideoImportBusy(false);
    }
  }
  async function cancelStartupVideoImport() {
    if (!videoImportId.current) return;
    try {
      const result = await call<{ cancelled: boolean }>("cancel-video-import", {
        operationId: videoImportId.current,
      });
      p.status(
        result.cancelled
          ? "Cancelling startup video import…"
          : "Startup video is already being saved; it will finish safely",
      );
    } catch (error) {
      setVideoImportError(String(error));
    }
  }
  async function saveVideoEdit() {
    if (!videoEdit || videoEditBusy || !videoEdit.video.id) return;
    const name = videoEdit.name.trim();
    if (videoEdit.action === "rename" && !name) {
      setVideoEditError("Enter a video name between 1 and 120 characters.");
      return;
    }
    setVideoEditBusy(true);
    setVideoEditError("");
    try {
      const id = videoEdit.video.id;
      const result = await call<{ media: Media[]; message: string }>(
        "video-library-edit",
        { action: videoEdit.action, id, name },
      );
      p.setMedia(result.media);
      if (videoEdit.action === "remove") {
        if (playing?.id === id) setPlaying(null);
        if (p.settings.video === id)
          p.change({ video: "", startupVideo: false });
      }
      p.status(result.message);
      setVideoEdit(null);
    } catch (error) {
      setVideoEditError(String(error));
    } finally {
      setVideoEditBusy(false);
    }
  }
  const videoLocked = videoImportBusy || videoEditBusy;
  const selectedCameras = cameras.filter((camera) => camera.enabled).length;
  const cameraError = cameraSetupError(cameras, p.settings.camerasEnabled);
  return (
    <>
      <Feedback error={a.error} />
      {videoEdit && (
        <ModalDialog
          className="presentation-rename-dialog"
          labelledBy="presentationRenameTitle"
          onCancel={() => {
            if (!videoEditBusy) setVideoEdit(null);
          }}
        >
          <form
            className="video-edit-dialog"
            onSubmit={(e) => {
              e.preventDefault();
              void saveVideoEdit();
            }}
          >
            <h3 id="presentationRenameTitle">
              {videoEdit.action === "rename" ? "Edit video" : "Remove video"}
            </h3>
            {videoEdit.action === "rename" ? (
              <>
                <label htmlFor="presentationRenameInput">Video name</label>
                <input
                  id="presentationRenameInput"
                  autoFocus
                  required
                  maxLength={120}
                  autoComplete="off"
                  value={videoEdit.name}
                  onChange={(e) =>
                    setVideoEdit({ ...videoEdit, name: e.target.value })
                  }
                />
              </>
            ) : (
              <p>
                Remove “{videoEdit.video.name}” from the video library? Its
                launcher copy in Library/Videos/mp4 is deleted; the original
                imported file is not touched.
              </p>
            )}
            <Feedback error={videoEditError} />
            <div className="presentation-rename-actions">
              <button
                type="button"
                className="secondary small"
                disabled={videoEditBusy}
                onClick={() => setVideoEdit(null)}
              >
                Cancel
              </button>
              <button
                type="submit"
                className="primary small"
                disabled={videoEditBusy}
              >
                {videoEditBusy
                  ? "Saving…"
                  : videoEdit.action === "rename"
                    ? "Save"
                    : "Remove video"}
              </button>
            </div>
          </form>
        </ModalDialog>
      )}
      <details
        className="presentation-fold"
        id="presentationCamerasSection"
        open
      >
        <summary>
          <span className="presentation-fold-icon" aria-hidden="true">
            <UiIcon name="cameras" />
          </span>
          <span className="presentation-fold-copy">
            <strong>Cameras</strong>
            <small>Choose and arrange cameras for Play</small>
          </span>
          <span className="pill" id="cameraCount">
            {selectedCameras} SELECTED
          </span>
        </summary>
        <div className="presentation-fold-body">
          <div className="camera-grid">
            <article className="card camera-library">
              <div className="camera-list-head">
                <span>Use</span>
                <span>Camera</span>
                <span>Height</span>
                <span>Zoom</span>
                <span>Order</span>
              </div>
              <div id="cameraRows" className="camera-list">
                {cameras.map((camera, i) => {
                  const slot = cameraNameSlot(camera);
                  return (
                    <div
                      className="camera-row"
                      data-index={i}
                      key={slot?.id ?? i}
                    >
                      <input
                        type="checkbox"
                        data-field="selected"
                        aria-label={`Use ${camera.name}`}
                        checked={camera.enabled}
                        onChange={(e) =>
                          setCameras(
                            cameras.map((c, n) =>
                              n === i ? { ...c, enabled: e.target.checked } : c,
                            ),
                          )
                        }
                      />
                      <div
                        className="camera-name"
                        title={
                          slot?.locked
                            ? "This camera retains its stock name"
                            : "In-game camera name"
                        }
                      >
                        <div className="camera-name-editor">
                          <input
                            type="text"
                            data-field="name"
                            spellCheck={false}
                            aria-label={`Camera name · ${slot?.name ?? camera.name}`}
                            value={camera.name}
                            maxLength={slot?.max}
                            disabled={!slot || slot.locked}
                            onChange={(e) =>
                              setCameras(
                                cameras.map((c, n) =>
                                  n === i ? renameCamera(c, e.target.value) : c,
                                ),
                              )
                            }
                          />
                          <button
                            type="button"
                            data-reset-name
                            title={
                              slot?.locked
                                ? "Stock camera name is locked"
                                : `Reset to ${slot?.name ?? camera.name}`
                            }
                            aria-label={
                              slot?.locked
                                ? "Stock camera name is locked"
                                : `Reset to ${slot?.name ?? camera.name}`
                            }
                            disabled={!slot || slot.locked}
                            onClick={() =>
                              setCameras(
                                cameras.map((c, n) =>
                                  n === i ? resetCameraName(c) : c,
                                ),
                              )
                            }
                          >
                            {slot?.locked ? "—" : "↺"}
                          </button>
                        </div>
                      </div>
                      {(["height", "zoom"] as const).map((key) => (
                        <input
                          key={key}
                          type="number"
                          data-field={key}
                          aria-label={`${camera.name} ${key}`}
                          min="-100"
                          max="100"
                          step="1"
                          value={camera[key]}
                          onChange={(e) =>
                            setCameras(
                              cameras.map((c, n) =>
                                n === i
                                  ? { ...c, [key]: Number(e.target.value) }
                                  : c,
                              ),
                            )
                          }
                        />
                      ))}
                      <div className="camera-order-buttons">
                        <button
                          data-move="up"
                          aria-label={`Move ${camera.name} up`}
                          disabled={i === 0}
                          onClick={() => setCameras(move(cameras, i, -1))}
                        >
                          ↑
                        </button>
                        <button
                          data-move="down"
                          aria-label={`Move ${camera.name} down`}
                          disabled={i === cameras.length - 1}
                          onClick={() => setCameras(move(cameras, i, 1))}
                        >
                          ↓
                        </button>
                      </div>
                    </div>
                  );
                })}
              </div>
            </article>
            <div className="camera-side-stack">
              <article className="card camera-options">
                <div className="card-title">
                  <span data-ui-icon="settings">
                    <UiIcon name="settings" />
                    Camera setup
                  </span>
                </div>
                <label className="toggle-row">
                  <input
                    type="checkbox"
                    checked={p.settings.camerasEnabled}
                    onChange={(e) =>
                      p.change({ camerasEnabled: e.target.checked })
                    }
                  />
                  <span>
                    <strong>Enable cameras</strong>
                    <small>
                      Version 2 QA · applies on the next modded Play.
                    </small>
                  </span>
                </label>
                <label className="toggle-row">
                  <input
                    type="checkbox"
                    id="lockCameraValues"
                    checked={p.settings.lockCamera}
                    onChange={(e) => p.change({ lockCamera: e.target.checked })}
                  />
                  <span>
                    <strong>Lock height and zoom</strong>
                    <small>
                      Reapply saved values while the game is running.
                    </small>
                  </span>
                </label>
                <div className="camera-names-box">
                  <div>
                    <strong>In-game camera names</strong>
                    <small>Saved names apply when the game starts.</small>
                  </div>
                </div>
                {cameraError && (
                  <p role="alert" className="camera-help">
                    {cameraError}
                  </p>
                )}
                <button
                  className="secondary camera-wide"
                  id="saveCameraSetup"
                  disabled={a.busy || !!cameraError}
                  onClick={() =>
                    void a.run(async () => {
                      if (cameraError) throw new Error(cameraError);
                      await p.save({
                        cameras: cameras.map((c) => ({
                          ...c,
                          name: c.name.trim(),
                        })),
                      });
                      p.status("Camera names, order, height and zoom saved");
                    })
                  }
                >
                  Save camera setup
                </button>
              </article>
              <CameraLiveStatus gameRoot={p.settings.gameRoot} />
            </div>
          </div>
        </div>
      </details>
      <details
        className="presentation-fold videos"
        id="presentationVideosSection"
        open
      >
        <summary>
          <span className="presentation-fold-icon" aria-hidden="true">
            <UiIcon name="presentation" />
          </span>
          <span className="presentation-fold-copy">
            <strong>Startup videos</strong>
            <small>Choose intro playback for Play</small>
          </span>
          <button
            className="secondary small"
            id="openPresentationFolder"
            data-ui-icon="profiles"
            onClick={(e) => {
              e.preventDefault();
              void a.run(async () => {
                await call("open-library", { kind: "Videos" });
              });
            }}
          >
            <UiIcon name="profiles" />
            Open folder
          </button>
        </summary>
        <div className="presentation-fold-body">
          <article className="card jukebox-table-card presentation-library-card">
            {videoImport && (
              <div id="presentationImportProgress" role="status">
                <progress
                  id="presentationImportBar"
                  max={100}
                  value={startupVideoImportPercent(videoImport)}
                />
                <p id="presentationImportMessage">
                  {startupVideoImportLabel(videoImport.phase)} ·{" "}
                  {startupVideoImportPercent(videoImport)}% ·{" "}
                  {formatBytes(videoImport.bytes)} /{" "}
                  {formatBytes(videoImport.total)}
                </p>
                {startupVideoImportCanCancel(videoImport) && (
                  <button
                    className="secondary"
                    id="cancelPresentationImport"
                    onClick={() => void cancelStartupVideoImport()}
                  >
                    Cancel import
                  </button>
                )}
              </div>
            )}
            <div className="presentation-table-head">
              <span>Video name</span>
              <span>Length / size</span>
              <span>Actions</span>
            </div>
            <div id="presentationLibrary">
              {!p.media.some((m) => m.kind === "Videos") && (
                <p>
                  Add an MP4 to start your video library. Choose it on Play to
                  use it at launch.
                </p>
              )}
              {p.media
                .filter((m) => m.kind === "Videos")
                .map((video) => {
                  const id = video.id ?? video.path;
                  const year = video.name.match(/\b(?:NBA\s*)?2K(\d{2})\b/i);
                  return (
                    <div
                      key={video.path}
                      className={`video-library-row${p.settings.video === id ? " selected" : ""}`}
                    >
                      <div className="video-library-identity">
                        <span
                          className="video-library-poster"
                          aria-hidden="true"
                        >
                          {year ? `2K${year[1]}` : "VIDEO"}
                        </span>
                        <span className="video-library-name" title={video.name}>
                          {video.name}
                        </span>
                      </div>
                      <small>
                        {formatVideoLength(video.durationMs)} ·{" "}
                        {formatBytes(video.bytes)}
                      </small>
                      <div className="video-library-actions">
                        <button
                          className="secondary small"
                          disabled={videoLocked}
                          onClick={() =>
                            setPlaying(
                              playing?.path === video.path ? null : video,
                            )
                          }
                        >
                          Preview
                        </button>
                        <button
                          className="secondary small"
                          disabled={videoLocked || p.settings.video === id}
                          aria-pressed={p.settings.video === id}
                          onClick={() =>
                            p.change({ video: id, startupVideo: true })
                          }
                        >
                          {p.settings.video === id ? "Selected" : "Use"}
                        </button>
                        {(["rename", "remove"] as const).map((action) => (
                          <button
                            key={action}
                            className="secondary small"
                            disabled={videoLocked || !video.id}
                            aria-label={`${action === "rename" ? "Edit" : "Remove"} ${video.name}`}
                            onClick={() => {
                              setVideoEditError("");
                              setVideoEdit({ action, video, name: video.name });
                            }}
                          >
                            {action === "rename" ? "Edit" : "Remove"}
                          </button>
                        ))}
                      </div>
                    </div>
                  );
                })}
            </div>
            <div className="presentation-library-footer">
              <div className="presentation-library-buttons">
                <button
                  className="primary small"
                  id="choosePresentationMovie"
                  disabled={a.busy || videoLocked}
                  onClick={() => void importStartupVideo()}
                >
                  {videoImportBusy ? "Importing MP4…" : "+ Add MP4"}
                </button>
              </div>
              <p>
                Fullscreen MP4 playback · Click or Xbox A/B to skip · H.264, AV1
                or VP9 + AAC · Up to 10 minutes / 2 GB
              </p>
            </div>
          </article>
          <Feedback error={videoImportError} />
          {playing && (
            <video
              className="presentation-inline-preview"
              key={playing.path}
              src={convertFileSrc(playing.path)}
              controls
              autoPlay
            />
          )}
          <p id="presentationMovieDetail" role="status">
            Videos stay in Library / Videos. Playback does not replace or patch
            the game’s startup movies.
          </p>
        </div>
      </details>
    </>
  );
}
// WPF clothing-picker.js: 16 tiles per page in the launcher outfit creator.
const PICKER_PAGE = 16;
const accessorySlots = [
  "headband",
  "eyewear",
  "faceMask",
  "backpack",
  "leftArm",
  "rightArm",
  "leftLeg",
  "rightLeg",
  "headwear",
  "headphones",
  "leftHand",
  "rightHand",
  "leftWrist",
  "rightWrist",
  "neckwear",
  "waist",
  "bodywear",
  "leftPiercing",
  "rightPiercing",
  "other",
];
export function Blacktop(
  p: Shared & {
    outfit: Outfit;
    setOutfit: (o: Outfit) => void;
    preview: (panel?: string) => void;
    topPreview: () => void;
  },
) {
  const a = useActions(p.status);
  const [catalog, setCatalog] = useState<CatalogFeature | null>(null);
  const [mannequin, setMannequin] = useState(false);
  const clothingCache = useClothingCache();
  const [picker, setPicker] = useState<string | null>(null),
    [query, setQuery] = useState(""),
    [group, setGroup] = useState("All"),
    [groups, setGroups] = useState<string[]>([]),
    [offset, setOffset] = useState(0),
    [items, setItems] = useState<Garment[]>([]),
    [total, setTotal] = useState(0),
    [loading, setLoading] = useState(false),
    [pictures, setPictures] = useState<Garment[]>([]),
    [pack, setPack] = useState<{
      name: string;
      count: number;
      error?: string;
    } | null>(null),
    [progress, setProgress] = useState<{
      done: number;
      total: number;
      phase: string;
    } | null>(null),
    [packRevision, setPackRevision] = useState(0);
  const [variants, setVariants] = useState<
    { id: string; original: string[]; layers: string[]; manualOnly: boolean }[]
  >([]);
  const [composition, setComposition] = useState<{
    key?: string;
    items?: string[];
    error?: string;
  }>({});
  useEffect(() => {
    let alive = true;
    call<typeof variants>("outfit-variants", {
      top: p.outfit.top,
      pool: p.outfit.pool ?? "male",
    })
      .then((v) => {
        if (alive) setVariants(v);
      })
      .catch((e) => {
        if (alive) setComposition({ error: String(e) });
      });
    return () => {
      alive = false;
    };
  }, [p.outfit.top, p.outfit.pool]);
  useEffect(() => {
    let alive = true;
    const key = JSON.stringify(p.outfit);
    setComposition({});
    if (p.outfit.streetId)
      call<{ items: string[] }>("outfit-inspect", p.outfit)
        .then((v) => {
          if (alive) setComposition({ ...v, key });
        })
        .catch((e) => {
          if (alive) setComposition({ error: String(e), key });
        });
    return () => {
      alive = false;
    };
  }, [p.outfit]);
  const generation = useRef(0);
  useEffect(() => {
    call<typeof pack>("preview-status")
      .then(setPack)
      .catch((e) => p.status(String(e)));
    let disposed = false;
    let off: (() => void) | undefined;
    listen<{ done: number; total: number; phase: string }>(
      "preview-progress",
      (e) => setProgress(e.payload),
    ).then((fn) => {
      if (disposed) fn();
      else off = fn;
    });
    return () => {
      disposed = true;
      off?.();
    };
  }, []);
  useEffect(() => {
    if (!picker) return;
    const id = ++generation.current;
    setLoading(true);
    const timer = setTimeout(
      () => {
        call<{ items: Garment[]; total: number; groups: string[] }>(
          "wardrobe",
          {
            slot: picker,
            query,
            group,
            offset,
            limit: PICKER_PAGE,
            pool: p.outfit.pool ?? "male",
          },
        )
          .then((r) => {
            if (id === generation.current) {
              setItems(r.items);
              setTotal(r.total);
              setGroups(r.groups);
            }
          })
          .catch((e) => p.status(String(e)))
          .finally(() => {
            if (id === generation.current) setLoading(false);
          });
      },
      query ? 100 : 0,
    );
    return () => {
      clearTimeout(timer);
      generation.current++;
    };
  }, [picker, query, group, offset, packRevision, p.outfit.pool]);
  useEffect(() => {
    let alive = true;
    call<Garment[]>("garments", {
      ids: [
        p.outfit.top,
        p.outfit.bottom,
        p.outfit.shoes,
        p.outfit.socks ?? "",
        ...p.outfit.accessories,
      ],
    })
      .then((r) => {
        if (alive) setPictures(r);
      })
      .catch((e) => p.status(String(e)));
    return () => {
      alive = false;
    };
  }, [p.outfit, packRevision]);
  function open(slot: string) {
    setPicker(slot);
    setQuery("");
    setGroup("All");
    setOffset(0);
    setItems([]);
    setGroups([]);
  }
  const current = (slot: string) =>
    ["top", "bottom", "shoes", "socks"].includes(slot)
      ? (p.outfit[slot as "top"] ?? "")
      : (pictures.find(
          (g) => g.category === slot && p.outfit.accessories.includes(g.asset),
        )?.asset ?? "");
  function choose(asset: string) {
    if (!picker) return;
    void a.run(async () => {
      const next = await call<Outfit>("select-outfit-piece", {
        outfit: p.outfit,
        slot: picker,
        asset,
      });
      p.setOutfit(next);
      setPicker(null);
    });
  }

  const selectorRows = [
    ["scoreboard", "Scoreboard", "presentation"],
    ["arena", "Arena", "blacktop"],
    ["ball", "Ball", "ball"],
  ] as const;
  const pieceIds: Record<string, string> = {
    top: "Top",
    bottom: "Bottom",
    socks: "Sock",
    shoes: "Shoe",
  };
  const accessoryCount = p.outfit.accessories.length;
  const savedName = p.library.outfits.some((o) => o.name === p.outfit.name)
    ? p.outfit.name
    : "";
  function pieceTile([slot, name, icon]: readonly [string, string, string]) {
    const asset = current(slot),
      g = pictures.find((g) => g.asset === asset),
      image = g && garmentImage(g),
      id = pieceIds[slot];
    return (
      <div className="outfit-piece" key={slot}>
        <button
          type="button"
          id={`outfit${id}Browse`}
          className="outfit-piece-button"
          aria-label={`Change ${name.toLowerCase()}: ${asset ? label(asset) : "Choose an item"}`}
          title={asset ? label(asset) : "Choose an item"}
          onClick={() => open(slot)}
        >
          <span className="outfit-piece-category">{name}</span>
          <span id={`outfit${id}Preview`} className="outfit-picture">
            <GarmentImg
              asset={asset || undefined}
              garment={image ? g : undefined}
              fallback={<OutfitItemIcon d={icon} />}
            />
          </span>
          <span id={`outfit${id}Label`} className="outfit-piece-name">
            {asset ? label(asset) : "Choose an item"}
          </span>
          <span className="outfit-change">
            Change <span aria-hidden="true">↗</span>
          </span>
        </button>
      </div>
    );
  }
  function accessoryTile([slot, name, icon]: readonly [
    string,
    string,
    string,
  ]) {
    const asset = current(slot),
      g = pictures.find((g) => g.asset === asset),
      image = g && garmentImage(g);
    return (
      <div className="accessory-slot" data-slot={slot} key={slot}>
        <button
          type="button"
          className="accessory-tile"
          aria-label={`${asset ? "Change" : "Add"} ${name.toLowerCase()}`}
          title={asset ? label(asset) : "Add item"}
          onClick={() => open(slot)}
        >
          <span className="accessory-tile-preview">
            <GarmentImg
              asset={asset || undefined}
              garment={image ? g : undefined}
              fallback={<OutfitItemIcon d={icon} />}
            />
          </span>
          <span className="accessory-tile-category">{name}</span>
          <span className="accessory-tile-name">
            {asset ? label(asset) : "Add item"}
          </span>
        </button>
        <button
          type="button"
          className="accessory-tile-remove"
          aria-label={`Remove ${name.toLowerCase()}`}
          hidden={!asset}
          onClick={() =>
            p.setOutfit({
              ...p.outfit,
              accessories: p.outfit.accessories.filter((x) => x !== asset),
            })
          }
        >
          ×
        </button>
      </div>
    );
  }
  return (
    <>
      <Feedback error={a.error} />
      <details
        className="presentation-section game-setup"
        id="blacktopScoreboardSection"
        open
      >
        <summary>Game setup</summary>
        <div className="blacktop-top-grid">
          <section
            className="card blacktop-selector-card"
            aria-labelledby="blacktopPresentationTitle"
          >
            <div className="settings-card-heading">
              <span className="scene-heading-icon" aria-hidden="true">
                <UiIcon name="blacktop" />
              </span>
              <span className="scene-heading-copy">
                <strong id="blacktopPresentationTitle">
                  Scoreboard, arena &amp; ball
                </strong>
              </span>
            </div>
            {selectorRows.map(([slot, name, icon]) => (
              <div className="blacktop-selector-row" key={slot}>
                <span
                  className={`blacktop-selector-icon${slot === "ball" ? " blacktop-ball-icon" : ""}`}
                  aria-hidden="true"
                >
                  <UiIcon name={icon} />
                </span>
                <div className="blacktop-selector-copy">
                  <strong>{name}</strong>
                  <span id={`${slot}Selected`} title={p.settings[slot]}>
                    {p.settings[slot]}
                  </span>
                  <small id={`${slot}Status`} data-phase="ready" role="status">
                    Applies at Blacktop player selection.
                  </small>
                </div>
                <button
                  id={`${slot}CatalogButton`}
                  className="secondary blacktop-catalog-button"
                  type="button"
                  onClick={() => setCatalog(slot)}
                >
                  Catalog
                </button>
              </div>
            ))}
          </section>
          {catalog && (
            <BlacktopCatalog
              kind={catalog}
              settings={p.settings}
              change={p.change}
              onClose={() => setCatalog(null)}
            />
          )}
          <section
            className="card blacktop-scene-settings"
            title="Applies on the next mod-profile launch · crowd is Blacktop-only."
          >
            <div className="settings-card-heading">
              <span className="scene-heading-icon" aria-hidden="true">
                <UiIcon name="presentation" />
              </span>
              <span className="scene-heading-copy">
                <strong>Court presentation</strong>
              </span>
            </div>
            <div className="settings-switch-list">
              {(
                [
                  ["removeDrone", "Remove drone", "drone", "blacktopRemoveDrone"],
                  ["removeDj", "Remove DJ", "jukebox", "blacktopRemoveDj"],
                  ["removeMc", "Remove MC", "microphone", "blacktopRemoveMc"],
                  ["removeCrowd", "Remove crowd", "blacktop", "blacktopCrowdEnabled"],
                ] as const
              ).map(([key, text, icon, id]) => (
                <label className="settings-switch-row" key={key}>
                  <input
                    className="settings-switch"
                    id={id}
                    type="checkbox"
                    checked={p.settings[key]}
                    onChange={(e) => p.change({ [key]: e.target.checked })}
                  />
                  <span className="scene-row-icon" aria-hidden="true">
                    <UiIcon name={icon} />
                  </span>
                  <span>{text}</span>
                </label>
              ))}
            </div>
          </section>
          <section className="card blacktop-overlay-settings">
            <div className="settings-card-heading">
              <span className="scene-heading-icon" aria-hidden="true">
                <UiIcon name="blacktop" />
              </span>
              <span className="scene-heading-copy">
                <strong>Blacktop overlay</strong>
              </span>
            </div>
            <label className="settings-switch-row">
              <input
                className="settings-switch"
                type="checkbox"
                id="autoOpenBlacktop"
                checked={p.settings.autoShowOverlay}
                onChange={(e) =>
                  p.change({ autoShowOverlay: e.target.checked })
                }
              />
              <span>
                <strong>Open overlay automatically</strong>
              </span>
            </label>
            <button
              id="previewBlacktopOverlay"
              className="secondary blacktop-preview-trigger"
              type="button"
              data-overlay-preview
              onClick={p.topPreview}
            >
              Preview F8 overlay
            </button>
            <details className="blacktop-overlay-more">
              <summary>More overlay options</summary>
              <label className="settings-switch-row">
                <input
                  className="settings-switch"
                  type="checkbox"
                  checked={p.settings.autoHideOverlay}
                  onChange={(e) =>
                    p.change({ autoHideOverlay: e.target.checked })
                  }
                />
                <span>
                  <strong>Hide overlay when a match starts</strong>
                </span>
              </label>
              <button
                className="secondary small blacktop-inline-preview"
                type="button"
                onClick={() => p.preview()}
              >
                Preview inside launcher
              </button>
            </details>
          </section>
        </div>
      </details>
      <details className="presentation-section" id="blacktopOutfitSection" open>
        <summary id="outfitCreatorTitle">Outfit creator</summary>
        <section
          className="card outfit-creator"
          id="outfitCreator"
          aria-labelledby="outfitCreatorTitle"
        >
          <div className="outfit-library-bar">
            <label>
              Saved outfits
              <select
                id="outfitSaved"
                aria-label="Saved outfits"
                value={savedName}
                onChange={(e) => {
                  const o = p.library.outfits.find(
                    (o) => o.name === e.target.value,
                  );
                  if (o) p.setOutfit({ ...o, accessories: [...o.accessories] });
                  else
                    p.setOutfit({
                      ...initialOutfit,
                      name: uniqueName(
                        "New outfit",
                        p.library.outfits.map((o) => o.name),
                      ),
                    });
                }}
              >
                <option value="">New outfit</option>
                {p.library.outfits.map((o) => (
                  <option key={o.name}>{o.name}</option>
                ))}
              </select>
            </label>
            <label>
              Name
              <input
                id="outfitName"
                aria-label="Outfit name"
                maxLength={64}
                placeholder="Name your outfit"
                value={p.outfit.name}
                onChange={(e) =>
                  p.setOutfit({ ...p.outfit, name: e.target.value })
                }
              />
            </label>
            <label className="outfit-fit">
              Clothing fit
              <select
                id="outfitPool"
                aria-label="Clothing fit"
                value={p.outfit.pool ?? "male"}
                onChange={(e) =>
                  p.setOutfit({
                    ...initialOutfit,
                    name: p.outfit.name,
                    pool: e.target.value as "male" | "female",
                    shoes: p.outfit.shoes,
                  })
                }
              >
                <option value="male">Men</option>
                <option value="female">Women</option>
              </select>
            </label>
            <button
              id="outfitSave"
              type="button"
              className="primary"
              disabled={a.busy}
              onClick={() =>
                void a.run(async () => {
                  await p.save({
                    outfits: [
                      ...p.library.outfits.filter(
                        (o) =>
                          o.name.toLowerCase() !== p.outfit.name.toLowerCase(),
                      ),
                      p.outfit,
                    ],
                  });
                  await call("save-outfit", p.outfit);
                  p.status("Outfit saved inside prototype Data");
                })
              }
            >
              Save outfit
            </button>
            <button
              id="outfitMannequin"
              type="button"
              className="secondary"
              disabled={(p.outfit.pool ?? "male") !== "male"}
              title={
                (p.outfit.pool ?? "male") === "male"
                  ? "Preview this outfit on the 3D mannequin"
                  : "The mannequin preview currently supports men’s clothing."
              }
              onClick={() => setMannequin(true)}
            >
              View on mannequin
            </button>
            <details className="outfit-actions" id="outfitActions">
              <summary aria-label="More outfit actions">
                More <span aria-hidden="true">▾</span>
              </summary>
              <div className="outfit-actions-menu">
                <button
                  id="outfitImport"
                  type="button"
                  disabled={a.busy}
                  onClick={() =>
                    void a.run(async () => {
                      const o = await call<Outfit | null>("import-outfit");
                      if (o) {
                        p.setOutfit(o);
                        p.status(
                          "Outfit imported into draft · review and Save outfit",
                        );
                      }
                    })
                  }
                >
                  Import outfit
                </button>
                <button
                  id="outfitExport"
                  type="button"
                  disabled={
                    a.busy ||
                    !composition.items ||
                    composition.key !== JSON.stringify(p.outfit)
                  }
                  onClick={() =>
                    void a.run(async () => {
                      const r = await call<{ path: string }>(
                        "export-outfit",
                        p.outfit,
                      );
                      p.status(`Outfit exported: ${r.path}`);
                    })
                  }
                >
                  Export outfit
                </button>
                <button
                  type="button"
                  onClick={() =>
                    void a.run(async () => {
                      await call("open-library", { kind: "Outfits" });
                    })
                  }
                >
                  Open outfit folder
                </button>
              </div>
            </details>
          </div>
          <div className="outfit-pieces">{outfitPieceSlots.map(pieceTile)}</div>
          <div className="outfit-library-bar outfit-variant-panel">
            <label>
              Observed fit variant
              <select
                aria-label="Observed fit variant"
                value={
                  variants.some((v) => v.id === p.outfit.streetId)
                    ? p.outfit.streetId
                    : ""
                }
                disabled={a.busy || !variants.length}
                onChange={(e) => {
                  const id = e.target.value;
                  if (id)
                    void a.run(async () => {
                      p.setOutfit(
                        await call<Outfit>("outfit-original", {
                          id,
                          name: p.outfit.name,
                        }),
                      );
                      p.status(
                        "Original variant loaded into draft · adjust its pieces, then save",
                      );
                    });
                }}
              >
                <option value="">Choose original fit…</option>
                {variants.map((v) => (
                  <option key={v.id} value={v.id}>
                    {v.id} · {v.original.length} pieces
                    {v.layers.filter((l) => l !== "none").length
                      ? " · layered"
                      : ""}
                  </option>
                ))}
              </select>
            </label>
            {!!p.outfit.streetId &&
              (variants.find((v) => v.id === p.outfit.streetId)?.layers
                .length ?? 0) > 1 && (
                <label>
                  Layer
                  <select
                    aria-label="Outfit layer"
                    value={p.outfit.layer || ""}
                    onChange={(e) =>
                      p.setOutfit({ ...p.outfit, layer: e.target.value })
                    }
                  >
                    <option value="">Original layer</option>
                    {variants
                      .find((v) => v.id === p.outfit.streetId)
                      ?.layers.map((l) => (
                        <option key={l} value={l}>
                          {label(l)}
                        </option>
                      ))}
                  </select>
                </label>
              )}
            <small id="outfitDetails">
              {(composition.key === JSON.stringify(p.outfit)
                ? composition.error
                : undefined) ??
                (composition.key === JSON.stringify(p.outfit) &&
                composition.items
                  ? `${composition.items.length} / 7 pieces · exact order validated for sharing`
                  : "Choose an observed fit variant to preserve the original top identity when sharing.")}
            </small>
          </div>
          <div
            id="outfitAccessories"
            className="accessory-cards accessory-slots"
          >
            <div className="accessory-slots-heading">
              <strong>Accessories</strong>
              <span
                role="status"
                title="Seven total clothing pieces, including top, bottom, socks, shoes and required layers."
              >
                {accessoryCount} / 3 equipped
              </span>
              <button
                type="button"
                disabled={!accessoryCount}
                onClick={() => p.setOutfit({ ...p.outfit, accessories: [] })}
              >
                Remove all
              </button>
            </div>
            <div className="accessory-slots-grid accessory-primary-grid">
              {outfitAccessorySlots.map(accessoryTile)}
            </div>
          </div>
          <div className="outfit-random">
            {[
              ["clothes", "Random clothes"],
              ["accessories", "Random accessories"],
              ["both", "Random clothes + accessories"],
            ].map(([mode, text]) => (
              <button
                key={mode}
                type="button"
                className="secondary small"
                disabled={a.busy}
                onClick={() =>
                  void a.run(async () => {
                    p.setOutfit(
                      await call<Outfit>("random-outfit", {
                        mode,
                        outfit: p.outfit,
                      }),
                    );
                    p.status(
                      "Draft randomized · shoes unchanged · at most 3 accessories",
                    );
                  })
                }
              >
                <UiIcon name="shuffle" />
                {text}
              </button>
            ))}
          </div>
          <div className="wardrobe-tools-footer">
            <div className="wardrobe-cache-control">
              <div className="wardrobe-cache-status">
                <small
                  id="clothingCacheStatus"
                  role="status"
                  aria-live="polite"
                >
                  {clothingCache.text}
                </small>
                <small id="previewPackStatus" role="status" aria-live="polite">
                  {pack?.error
                    ? `Preview pack unavailable: ${pack.error}. Import a replacement ZIP.`
                    : pack
                      ? `${pack.count.toLocaleString()} renders · ${pack.name}`
                      : "Optional rendered previews can be imported from a ZIP."}
                </small>
                {progress && (
                  <progress
                    id="clothingCacheProgress"
                    aria-label="Wardrobe preview caching progress"
                    max={progress.total || 1}
                    value={progress.done}
                    title={`${progress.phase} · ${Math.round((progress.done / Math.max(1, progress.total)) * 100)}%`}
                  />
                )}
              </div>
              <button
                className="secondary small cache-primary-action"
                id="cacheClothingPreviews"
                type="button"
                disabled={clothingCache.busy}
                onClick={clothingCache.start}
              >
                <span aria-hidden="true">↻</span> Prepare all previews
              </button>
              <button
                className="secondary small"
                id="importPreviewPack"
                type="button"
                disabled={a.busy}
                onClick={() =>
                  void a.run(async () => {
                    setProgress({
                      done: 0,
                      total: 1,
                      phase: "Choose preview ZIP",
                    });
                    try {
                      const result = await call<{ count: number } | null>(
                        "import-previews",
                      );
                      if (result) {
                        setPack(await call("preview-status"));
                        setPackRevision((x) => x + 1);
                        p.status(
                          `${result.count.toLocaleString()} previews imported · persistent ZIP, thumbnails cached as needed`,
                        );
                      }
                    } finally {
                      setProgress(null);
                    }
                  })
                }
              >
                Import preview ZIP
              </button>
              <button
                className="secondary small"
                id="cancelClothingCache"
                type="button"
                hidden={!clothingCache.busy}
                disabled={clothingCache.state?.phase === "cancelling"}
                onClick={clothingCache.cancel}
              >
                Cancel
              </button>
              <button
                className="secondary small"
                type="button"
                onClick={() =>
                  void a.run(async () => {
                    await call("open-library", { kind: "Previews" });
                  })
                }
              >
                Open preview folder
              </button>
            </div>
          </div>
        </section>
      </details>
      {mannequin && (
        <FitPreviewDialog
          outfit={p.outfit}
          setOutfit={p.setOutfit}
          pictures={pictures}
          onBrowse={(slot) => open(slot)}
          onClose={(message) => {
            setMannequin(false);
            if (message) p.status(message);
          }}
        />
      )}
      {picker && (
        <ModalDialog
          className="clothing-picker outfit-picker"
          labelledBy="clothing-picker-title"
          onCancel={() => setPicker(null)}
        >
          <div className="clothing-picker-heading">
            <h2 id="clothing-picker-title">{label(picker)} · Outfit creator</h2>
            <button type="button" onClick={() => setPicker(null)}>
              Back
            </button>
          </div>
          <div className="clothing-picker-filters">
            <input
              type="search"
              aria-label="Search clothing"
              placeholder="Search clothing or brand"
              value={query}
              onChange={(e) => {
                setQuery(e.target.value);
                setOffset(0);
              }}
            />
            <button
              type="button"
              disabled={!query}
              onClick={() => {
                setQuery("");
                setOffset(0);
              }}
            >
              Clear search
            </button>
            <div
              className="clothing-category-filters"
              aria-label="Clothing categories"
            >
              {["All", ...groups].map((g) => (
                <button
                  key={g}
                  type="button"
                  data-category={g}
                  aria-pressed={group === g}
                  onClick={() => {
                    setGroup(g);
                    setOffset(0);
                  }}
                >
                  {g}
                </button>
              ))}
            </div>
          </div>
          <p className="clothing-picker-note">
            This edits your outfit draft only. Save it when ready.
          </p>
          <div className="clothing-picker-grid">
            {items.map((g) => (
              <button
                key={g.asset}
                type="button"
                className="clothing-tile"
                data-item-id={g.asset}
                aria-pressed={current(picker) === g.asset}
                title={label(g.asset)}
                onClick={() => choose(g.asset)}
              >
                <span className="clothing-picture">
                  <GarmentImg
                    asset={g.asset}
                    garment={garmentImage(g) ? g : undefined}
                    alt={label(g.asset)}
                    fallback="No preview"
                  />
                </span>
                <span className="clothing-tile-name">{label(g.asset)}</span>
                {current(picker) === g.asset && (
                  <small className="clothing-selected">Selected</small>
                )}
              </button>
            ))}
            {loading && (
              <div className="clothing-empty">
                <p>Loading catalog…</p>
              </div>
            )}
            {!loading && !items.length && (
              <div className="clothing-empty">
                <p>Nothing matches these filters.</p>
              </div>
            )}
          </div>
          <div className="clothing-picker-pager">
            <button
              type="button"
              disabled={offset === 0 || loading}
              onClick={() => setOffset(Math.max(0, offset - PICKER_PAGE))}
            >
              Previous
            </button>
            <span role="status">
              {total
                ? `${offset + 1}–${Math.min(offset + PICKER_PAGE, total)} of ${total.toLocaleString()} · Page ${Math.floor(offset / PICKER_PAGE) + 1} / ${Math.max(1, Math.ceil(total / PICKER_PAGE))}`
                : "No results"}
            </span>
            <button
              type="button"
              disabled={offset + PICKER_PAGE >= total || loading}
              onClick={() => setOffset(offset + PICKER_PAGE)}
            >
              Next
            </button>
          </div>
        </ModalDialog>
      )}
    </>
  );
}
