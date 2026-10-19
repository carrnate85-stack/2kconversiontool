(() => {
  // These are static, illustrative renders of stock scene archives. They are
  // independent of whether a scoreboard has been verified to work in Blacktop.
  const scenes = {
    default: '2kw_scorebug_generic_reduced',
    simple: '2kw_scorebug_generic_simple',
    'team-colors': '2kw_scorebug_teamcolors_reduced',
    'proving-grounds': '2kw_scorebug_provinggrounds_reduced',
    mypark: 'mypark_scorebug',
    'world-full': '2kw_scorebug_full',
    'proving-grounds-full': '2kw_scorebug_provinggrounds_full',
    broadcast: 'scorebug_broadcast',
    'era-1980': '1980s_scorebug',
    'era-2000': '2000s_scorebug',
    'era-2010': '2010_scorebug',
    euro: 'euro_scorebug',
    'mypark-simple': 'mypark_scorebug_simple',
    'proam-spectator': 'scorebug_opt',
    'flashback-openrun': 'scorebug_familyflashback_nba_open_run',
    highschool: 'scorebug_highschool',
    'shut-it-down': 'shut_it_down_scorebug',
  };
  const needsReview = new Set(['era-1980', 'era-2010', 'euro', 'proam-spectator',
    'highschool', 'shut-it-down']);
  const catalog = Object.fromEntries(Object.entries(scenes).map(([id, scene]) => [id, {
    image: `assets/scoreboard-previews/${scene}-asset.png`,
    scene,
    frame: 'contain',
    caption: needsReview.has(id)
      ? 'Ringers vs Ballers sample · needs visual review'
      : 'Ringers vs Ballers · illustrative offline render',
  }]));
  for (const id of ['default', 'proving-grounds', 'proving-grounds-full']) {
    catalog[id].caption = 'Built-in team icons · illustrative offline render';
  }
  catalog.simple.caption = '2K27 branding · illustrative offline render';
  catalog['era-1980'].caption = 'Sparse source layout · needs in-game comparison';
  catalog['proam-spectator'].caption = 'Blacktop scrimmage sample · logo fit needs review';
  catalog['era-1990'] = { image: 'assets/scoreboard-previews/1990s_scorebug_alternate-asset.png',
    scene: '1990s_scorebug_1',
    caption: 'Unsuffixed scene + controller clock frame · unverified' };
  for (const id of ['mycourt-scrimmage', 'w-scrimmage']) {
    catalog[id] = { image: 'assets/scoreboard-previews/mycourt_scorebug_scrimmage_composite-asset.png',
      scene: 'mycourt_scorebug_scrimmage',
      caption: 'Controller-derived composite · needs in-game review' };
  }
  if (typeof module !== 'undefined') module.exports = catalog;
  if (typeof window !== 'undefined') window.ScoreboardPreviewCatalog = catalog;
})();
