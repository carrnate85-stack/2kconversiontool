// Preview-only response from paired, decoded boundary samples.
export function skinBoundaryResponse(reference, source, referenceResponse = [1, 1, 1]) {
  if (![reference, source, referenceResponse].every(values => Array.isArray(values)
    && values.length === 3 && values.every(value => Number.isFinite(value) && value >= 0))) return null;
  const response = source.map((value, channel) => reference[channel] * referenceResponse[channel] / Math.max(value, 0.005));
  // Per-channel clipping changes hue. Reject the entire unreliable response.
  return response.every(value => value >= 0.3 && value <= 8) ? response : null;
}

// Fallback when the per-channel response is rejected: one luminance gain
// still closes the brightness step at the seam without shifting hue.
export function skinLuminanceResponse(reference, source, referenceResponse = [1, 1, 1]) {
  if (![reference, source, referenceResponse].every(values => Array.isArray(values)
    && values.length === 3 && values.every(value => Number.isFinite(value) && value >= 0))) return null;
  const weights = [0.2126, 0.7152, 0.0722];
  const luminance = values => values.reduce((total, value, channel) => total + value * weights[channel], 0);
  const gain = luminance(reference.map((value, channel) => value * referenceResponse[channel])) / Math.max(luminance(source), 0.005);
  return gain >= 0.3 && gain <= 8 ? [gain, gain, gain] : null;
}
