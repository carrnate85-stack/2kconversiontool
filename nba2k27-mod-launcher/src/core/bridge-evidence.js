const LIVE_BRIDGE_EVIDENCE = Object.freeze({
  gameSha256: 'a870c4920985ee9bf9f3dba996d1bf41e5d65748274aadc00861bcf11b3b9ae5',
  modiumSha256: '1d3c23eee8276eebbb46a86fc67aa5bbcbdc3ddb8997bf7c94227c36ab77c6c3',
  backendSha256: 'b690749c5207707ee1ad3c26826dd833db76a23ecc32d83dbdf693331d77f231',
  observedAt: '2026-09-02',
  endpointObserved: true,
  receiverWorkerCount: 3,
  serverHandleCount: 3,
  modiumHoldsWritableGameHandle: true,
  workersIdleCycleDelta: 0,
  workersWokeDuringAssetReload: true,
  cleanRoomGameBridgeReady: false,
});

function evidenceForGame(gameSha256) {
  return String(gameSha256 || '').toLowerCase() === LIVE_BRIDGE_EVIDENCE.gameSha256
    ? { ...LIVE_BRIDGE_EVIDENCE }
    : null;
}

module.exports = { LIVE_BRIDGE_EVIDENCE, evidenceForGame };
