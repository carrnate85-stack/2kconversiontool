export const wnbaFreestyleAvailability = Object.freeze({
  enabled: true,
  message:
    "WNBA Freestyle team selection and on-court play passed dev testing. Changes apply on your next Play session.",
});
// Next-Play dev integration; the backend also requires its frozen debug plan.
export const wnbaBlacktopAvailability = Object.freeze({
  enabled: true,
});
// Frontend dev assets can hot-reload while an older launcher is still open.
// Its missing capability must not advertise a route that binary cannot run.
export function blacktopUnlockAvailable(runtime: { capabilities?: { wnbaBlacktop?: boolean } } | null | undefined) {
  return wnbaBlacktopAvailability.enabled && runtime?.capabilities?.wnbaBlacktop === true;
}
