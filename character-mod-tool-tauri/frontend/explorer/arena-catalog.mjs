// Capture-only scene interpretation. Source archives remain unchanged.
export function catalogOverheadTarget(name) {
  if (/@baskets?@|basket$|scorecube|scoreboard/i.test(name)) return false;
  return /@(?:[^@]*ceiling[^@]*|curvy_skylight_grp[^@]*|newlights|lights_group)@/i.test(name);
}

export function captureCreatedWoodTint(name, shader, parameter) {
  if(!/^floor\.fx(?:#|$)/i.test(shader || '') || !parameter?.CreatedFloor)return null;
  const key=/:floorwood_insidethreepointline_mat$/i.test(name) ? 'InsideThreePointLineWoodColor'
    : /:floorwood_outsidethreepointline_mat$/i.test(name) ? 'OutsideThreePointLineWoodColor' : null;
  const tint=key && parameter[key];
  return Array.isArray(tint) && tint.length>=3 && tint.slice(0,3).every(Number.isFinite) ? tint.slice(0,3) : null;
}

// Principal court direction avoids pairing diagonally opposite baskets in
// facilities with parallel courts. The envelope includes every basket lane.
export function captureBasketFrame(anchors) {
  if (anchors.length < 2) return null;
  const center = [0, 1, 2].map(axis => anchors.reduce((sum, p) => sum + p[axis], 0) / anchors.length);
  let xx = 0, zz = 0, xz = 0;
  for (const p of anchors) { const x = p[0]-center[0], z = p[2]-center[2]; xx += x*x; zz += z*z; xz += x*z; }
  const angle = .5 * Math.atan2(2*xz, xx-zz);
  let axis = [Math.cos(angle), 0, Math.sin(angle)];
  if (axis[2] < -1e-8 || Math.abs(axis[2]) < 1e-8 && axis[0] < 0) axis = axis.map(v => -v);
  const side = [axis[2], 0, -axis[0]];
  const project = (p, a) => (p[0]-center[0])*a[0] + (p[2]-center[2])*a[2];
  const longitudinal = anchors.map(p => project(p,axis)), lateral = anchors.map(p => project(p,side));
  const span = Math.max(...longitudinal)-Math.min(...longitudinal);
  if (span <= 500) return null;
  const length = span+244;
  return {center, axis, length, width:Math.max(...lateral)-Math.min(...lateral)+length*.534};
}
