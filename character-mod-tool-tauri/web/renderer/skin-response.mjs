// Preview-only response from paired, decoded boundary samples.
export function skinBoundaryResponse(reference, source, referenceResponse = [1, 1, 1]) {
  if (![reference, source, referenceResponse].every(values => Array.isArray(values)
    && values.length === 3 && values.every(value => Number.isFinite(value) && value >= 0))) return null;
  const response = source.map((value, channel) => reference[channel] * referenceResponse[channel] / Math.max(value, 0.005));
  // Per-channel clipping changes hue. Reject the entire unreliable response.
  return response.every(value => value >= 0.3 && value <= 8) ? response : null;
}
