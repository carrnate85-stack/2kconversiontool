import type { ReactNode } from "react";
import { wnbaFreestyleAvailability } from "./feature-availability";
import wnbaLogo from "./wpf/assets/wnba-logo-horizontal.png";
import { UiIcon } from "./wpf/WpfShell";

// WPF #page-unlocks markup. Routes without a proven Version 2 write path stay
// disabled; the status line says why.
export type WnbaRosterVariant = "wpf-era" | "national" | "clubs";
export const wnbaRosterVariants: ReadonlyArray<{
  id: WnbaRosterVariant;
  label: string;
}> = [
  { id: "wpf-era", label: "WPF-era mixed roster (403 teams, recommended)" },
  { id: "national", label: "Clubs + WNBA USA (350 teams)" },
  { id: "clubs", label: "Clubs only (349 teams, roster test only)" },
];
function Feature({
  id,
  title,
  help,
  status,
  checked,
  enabled = false,
  onChange,
  children,
}: {
  id: string;
  title: string;
  help: string;
  status: string;
  checked: boolean;
  enabled?: boolean;
  onChange: (checked: boolean) => void;
  children?: ReactNode;
}) {
  const state = enabled ? "" : " unavailable";
  return (
    <article className={`card unlock-feature${state}`}>
      <span className="unlock-feature-icon wnba-logo-mark" aria-hidden="true">
        <img src={wnbaLogo} alt="" />
      </span>
      <label className={`settings-switch-row unlock-choice${state}`}>
        <input
          className="settings-switch"
          type="checkbox"
          id={id}
          aria-describedby={`${id}Help`}
          checked={checked}
          disabled={!enabled}
          onChange={(event) => onChange(event.target.checked)}
        />
        <span>
          <strong>{title}</strong>
        </span>
      </label>
      <p className="settings-description" id={`${id}Help`}>
        {help}
      </p>
      {children}
      <small className="unlock-status" role="status" aria-live="polite">
        {status}
      </small>
    </article>
  );
}

export function freestyleUnlockMessage(enabled: boolean) {
  return enabled
    ? "Saved: On. WNBA teams will be available in Freestyle on your next Play session."
    : "Saved: Off. WNBA access in Freestyle is off for your next Play session.";
}

export function UnlocksPanel({
  blacktop,
  onBlacktopChange,
  freestyle,
  onFreestyleChange,
  freestyleMessage,
  blacktopAvailable = false,
}: {
  blacktop: boolean;
  onBlacktopChange: (value: boolean) => void;
  freestyle: boolean;
  onFreestyleChange: (value: boolean) => void;
  freestyleMessage: string;
  blacktopAvailable?: boolean;
}) {
  return (
    <div className="unlocks-content unlocks-workspace">
      <div
        className="unlock-category-strip"
        aria-label="Available unlock categories"
      >
        <span className="unlock-category-active" data-ui-icon="unlocks">
          <UiIcon name="unlocks" />
          Team access
        </span>
        <span data-ui-icon="settings">
          <UiIcon name="settings" />
          Experimental
        </span>
        <small>Changes apply when you launch with Play</small>
      </div>
      <div className="unlocks-layout">
        <div className="unlocks-main">
          <section className="unlock-group" aria-labelledby="teamUnlocksTitle">
            <div className="settings-section-heading">
              <div>
                <h2 id="teamUnlocksTitle">Team access</h2>
                <p>Choose where WNBA teams are available.</p>
              </div>
              <span className="settings-badge">Applies next launch</span>
            </div>
            <div className="team-unlocks-grid">
              <Feature
                id="unlockWnbaFreestyle"
                title="WNBA Teams in Freestyle"
                help="Adds WNBA teams to Freestyle. Team selection and on-court play have passed dev testing. Changes apply on your next Play session."
                status={freestyleMessage}
                checked={freestyle}
                enabled={wnbaFreestyleAvailability.enabled}
                onChange={onFreestyleChange}
              />
              <Feature
                id="unlockWnbaBlacktop"
                title="Unlock WNBA Teams in Blacktop"
                help={
                  blacktopAvailable
                    ? "Use Y to cycle to WNBA teams in Blacktop. Changes apply on your next Play session."
                    : "WNBA access in Blacktop is being integrated and is not available in this build."
                }
                status={
                  blacktopAvailable
                    ? `Saved: ${blacktop ? "On" : "Off"}. Applies on your next Play session.`
                    : "Not available yet. Your saved choice is kept, but it is not applied to the game."
                }
                checked={blacktop}
                enabled={blacktopAvailable}
                onChange={onBlacktopChange}
              />
            </div>
          </section>
          <section
            className="unlock-group unlock-experimental"
            aria-labelledby="experimentalUnlocksTitle"
          >
            <div className="settings-section-heading experimental-heading">
              <div>
                <h2 id="experimentalUnlocksTitle">Experimental</h2>
                <p>Advanced controls. Use with care.</p>
              </div>
              <span className="settings-badge">Resets on app launch</span>
            </div>
            <article className="card debug-unlock-card unavailable">
              <span
                className="unlock-feature-icon"
                data-ui-icon="settings"
                aria-hidden="true"
              >
                <UiIcon name="settings" />
              </span>
              <label className="settings-switch-row unlock-choice unavailable">
                <input
                  className="settings-switch"
                  type="checkbox"
                  id="enableDebugMenus"
                  aria-describedby="debugMenuHelp"
                  checked={false}
                  disabled
                  onChange={() => {}}
                />
                <span>
                  <strong>Enable Debug Menus</strong>
                </span>
              </label>
              <p className="settings-description">
                Opens built-in debug and restricted menus when starting the game
                with Play.
              </p>
              <div className="debug-offline-note" id="debugMenuHelp">
                <strong>Steam Offline Mode required</strong>
                <span>
                  May cause instability or affect saves. Use a backup or test
                  roster. Always off when the launcher opens; this choice is
                  never saved.
                </span>
              </div>
              <small
                className="unlock-status"
                id="debugMenuStatus"
                role="status"
                aria-live="polite"
              >
                No supported Version 2 route is exposed; no game write will be
                attempted.
              </small>
            </article>
          </section>
        </div>
        <aside className="unlocks-aside" aria-label="Unlock guidance">
          <section className="card unlock-guide-card">
            <h2 data-ui-icon="unlocks">
              <UiIcon name="unlocks" />
              How unlocks apply
            </h2>
            <p>
              WNBA choices save automatically and take effect on your next Play
              session.
            </p>
            <p>
              {blacktopAvailable
                ? "Freestyle and Blacktop use the verified WNBA roster. Debug menus remain unavailable."
                : "Freestyle uses the verified WNBA roster. Blacktop and debug menus remain unavailable."}
            </p>
          </section>
          <section className="card unlock-guide-card">
            <h2 data-ui-icon="book">
              <UiIcon name="book" />
              Good to know
            </h2>
            <ul>
              <li>
                Custom rosters must contain WNBA teams for them to appear.
              </li>
              <li>
                If debug menus cause instability, turn them off and test with a
                backup roster.
              </li>
            </ul>
          </section>
        </aside>
      </div>
    </div>
  );
}
