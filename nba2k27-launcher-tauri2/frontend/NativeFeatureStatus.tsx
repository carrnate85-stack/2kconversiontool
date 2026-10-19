export type FeatureStatus = { phase: string; message: string };
export type V2Features = { features?: Record<string, FeatureStatus> };
const labels = {
  cameras: "Cameras",
  scaler: "Scaler",
  "wnba-freestyle": "WNBA Freestyle",
  "wnba-blacktop": "WNBA Blacktop",
  presentation: "Court presentation",
};
const problemPhases = new Set([
  "held",
  "unavailable",
  "unverified",
  "error",
  "reset",
  "reload",
]);
export function NativeFeatureStatus({
  status,
  issuesOnly = false,
}: {
  status?: V2Features | null;
  issuesOnly?: boolean;
}) {
  if (!status?.features) return null;
  const entries = Object.entries(labels).filter(([name]) => {
    const feature = status.features?.[name];
    return feature && (!issuesOnly || problemPhases.has(feature.phase));
  });
  if (!entries.length) return null;
  return (
    <div className="native-v2-features">
      {entries.map(([name, label]) => {
        const feature = status.features?.[name];
        if (!feature) return null;
        return (
          <p
            className={problemPhases.has(feature.phase) ? "notice" : "muted"}
            role={problemPhases.has(feature.phase) ? "alert" : undefined}
            key={name}
          >
            <strong>{label}</strong>
            {" · "}
            {feature.phase} · {feature.message}
          </p>
        );
      })}
    </div>
  );
}
