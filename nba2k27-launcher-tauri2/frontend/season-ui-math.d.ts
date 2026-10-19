export const seasonMath: {
  averages: Record<string, string>;
  percentages: Record<string, [string, string]>;
  limits: Record<string, number>;
  display: (totals: Record<string, number>) => Record<string, number | null>;
  change: (
    totals: Record<string, number>,
    key: string,
    value: number,
  ) => Record<string, number>;
};
