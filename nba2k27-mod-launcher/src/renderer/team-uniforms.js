(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  if (root) root.TeamUniforms = api;
})(typeof window !== 'undefined' ? window : globalThis, function () {
  'use strict';

  const TYPES = Object.freeze([
    'Normal', 'Practice', 'Historic', 'Unlockable All-Stars', 'Unlockable Secondary',
    'Unlockable Christmas', 'Unlockable Heat', 'Unlockable Warriors', 'Unlockable Hornets',
    "Unlockable St. Patrick's", 'Unlockable Latin Night', 'Unlockable NBA Green',
    'Unlockable HWC', 'Unlockable Black', 'Unlockable Military', 'Hidden', 'Template',
    'Created', 'Alternate Pool', 'Pro-Am', 'Randomized',
  ]);
  const SIDES = Object.freeze(['Home', 'Away']);
  const DESIGNATIONS = Object.freeze([
    'None', 'Icon', 'Statement', 'City', 'Association', 'Classic', 'Specter',
    'Rebel', 'Explorer', 'Heroine', 'Court Origins',
  ]);
  const HEADBAND_COLORS = Object.freeze([
    'White', 'Black', 'Uniform 1', 'Uniform 2', 'Uniform 3',
    'Uniform Armsleeve', 'Uniform Legsleeve', 'Uniform Primary Color',
  ]);
  const HEADBAND_LOGOS = Object.freeze(['None', 'NBA only', 'NBA + Nike', 'WNBA + Nike']);
  const LOGO_TYPES = Object.freeze(['None', 'NBA Authentics', 'Christmas', 'Latin Nights', 'Hardwood Classics', 'WNBA']);
  const LOGO_BRANDS = Object.freeze(['None', 'Adidas', 'Nike', 'Jordan']);
  const DOMINANT_COLORS = Object.freeze([
    'None', 'White', 'Opal', 'Silver', 'Yellow', 'Gold', 'Orange', 'Pink', 'Red',
    'Dark Red', 'Dark Teal', 'Light Green', 'Green', 'Dark Green', 'Purple', 'Black',
    'Gray', 'Light Blue', 'Blue', 'Royal Blue', 'Dark Blue',
  ]);
  const NAMEPLATE_STYLES = Object.freeze(['None', 'Last name', 'First name', 'Nickname']);
  const YES_NO = Object.freeze(['No', 'Yes']);
  const PRIORITIES = Object.freeze(Array.from({ length: 16 }, (_, index) => String(index)));
  const PACKED_FIELDS = Object.freeze([
    { key: 'typeId', label: 'Uniform type', group: 'Identity and logos', values: TYPES },
    { key: 'logoTypeId', label: 'Logo type', group: 'Identity and logos', values: LOGO_TYPES },
    { key: 'logoBrandId', label: 'Logo brand', group: 'Identity and logos', values: LOGO_BRANDS },
    { key: 'dominantColorId', label: 'Dominant color', group: 'Identity and logos', values: DOMINANT_COLORS },
    { key: 'scorebugUsesUniformColor', label: 'Scorebug uses uniform color', group: 'Identity and logos', values: YES_NO },
    { key: 'usesTeamLogo', label: 'Use team logo', group: 'Identity and logos', values: YES_NO },
    { key: 'disablePhoto', label: 'Disable uniform photo', group: 'Identity and logos', values: YES_NO },
    { key: 'homePriority', label: 'Home priority', group: 'Picker behavior', values: PRIORITIES },
    { key: 'awayPriority', label: 'Away priority', group: 'Picker behavior', values: PRIORITIES },
    { key: 'sideId', label: 'Side', group: 'Picker behavior', values: SIDES },
    { key: 'designationId', label: 'Designation', group: 'Picker behavior', values: DESIGNATIONS },
    { key: 'isAlternate', label: 'Alternate', group: 'Picker behavior', values: YES_NO },
    { key: 'topNamePlateStyleId', label: 'Top nameplate', group: 'Jersey layout', values: NAMEPLATE_STYLES },
    { key: 'bottomNamePlateStyleId', label: 'Bottom nameplate', group: 'Jersey layout', values: NAMEPLATE_STYLES },
    { key: 'overrideNbaPatch', label: 'Override NBA patch', group: 'Jersey layout', values: YES_NO },
    { key: 'hasNumberOnShorts', label: 'Number on shorts', group: 'Jersey layout', values: YES_NO },
    { key: 'rightAlignFrontNumber', label: 'Right-align front number', group: 'Jersey layout', values: YES_NO },
    { key: 'legAccessoryColorHome', label: 'Leg route home', group: 'Accessory routing', values: HEADBAND_COLORS },
    { key: 'legAccessoryColorAway', label: 'Leg route away', group: 'Accessory routing', values: HEADBAND_COLORS },
    { key: 'armItemColorHome', label: 'Arm route home', group: 'Accessory routing', values: HEADBAND_COLORS },
    { key: 'armItemColorAway', label: 'Arm route away', group: 'Accessory routing', values: HEADBAND_COLORS },
    { key: 'sockColorHome', label: 'Sock route home', group: 'Accessory routing', values: HEADBAND_COLORS },
    { key: 'sockColorAway', label: 'Sock route away', group: 'Accessory routing', values: HEADBAND_COLORS },
    { key: 'primaryLogoSockColor', label: 'Primary logo sock color', group: 'Logo sock routing', values: YES_NO },
    { key: 'secondaryLogoSockColor', label: 'Secondary logo sock color', group: 'Logo sock routing', values: YES_NO },
    { key: 'tertiaryLogoSockColor', label: 'Tertiary logo sock color', group: 'Logo sock routing', values: YES_NO },
    { key: 'quaternaryLogoSockColor', label: 'Quaternary logo sock color', group: 'Logo sock routing', values: YES_NO },
    { key: 'quintaryLogoSockColor', label: 'Quintary logo sock color', group: 'Logo sock routing', values: YES_NO },
  ]);
  const COLOR_FIELDS = Object.freeze([
    { key: 'uniformPrimaryColor', label: 'Uniform primary', group: 'Uniform routing' },
    { key: 'uniformSecondaryColor', label: 'Uniform secondary', group: 'Uniform routing' },
    { key: 'primaryColor', label: 'Primary', group: 'Jersey palette' },
    { key: 'secondaryColor', label: 'Secondary', group: 'Jersey palette' },
    { key: 'tertiaryColor', label: 'Tertiary', group: 'Jersey palette' },
    { key: 'quaternaryColor', label: 'Quaternary', group: 'Jersey palette' },
    { key: 'quintaryColor', label: 'Quintary', group: 'Jersey palette' },
    { key: 'sextaryColor', label: 'Sextary', group: 'Jersey palette' },
    { key: 'primaryAccessoryColor', label: 'Primary accessory', group: 'Accessory palette' },
    { key: 'secondaryAccessoryColor', label: 'Secondary accessory', group: 'Accessory palette' },
    { key: 'tertiaryAccessoryColor', label: 'Tertiary accessory', group: 'Accessory palette' },
    { key: 'quaternaryAccessoryColor', label: 'Quaternary accessory', group: 'Accessory palette' },
    { key: 'quintaryAccessoryColor', label: 'Quintary accessory', group: 'Accessory palette' },
    { key: 'underShirtColorHome', label: 'Home undershirt', group: 'Undershirt and shoes' },
    { key: 'underShirtColorAway', label: 'Away undershirt', group: 'Undershirt and shoes' },
    { key: 'shoePrimaryColorAway', label: 'Away shoe primary', group: 'Undershirt and shoes' },
    { key: 'primarySockColor', label: 'Primary sock', group: 'Sock palette' },
    { key: 'secondarySockColor', label: 'Secondary sock', group: 'Sock palette' },
    { key: 'tertiarySockColor', label: 'Tertiary sock', group: 'Sock palette' },
    { key: 'quaternarySockColor', label: 'Quaternary sock', group: 'Sock palette' },
    { key: 'quintarySockColor', label: 'Quintary sock', group: 'Sock palette' },
  ]);

  function enumLabel(values, value, prefix) {
    return Number.isInteger(value) && values[value] ? values[value] : `${prefix} ${value}`;
  }

  function colorHex(value) {
    if (!Number.isInteger(value) || value < 0 || value > 0xFFFFFFFF) return 'Unavailable';
    return `0x${(value >>> 0).toString(16).padStart(8, '0').toUpperCase()}`;
  }

  function normalizeUniform(uniform) {
    const value = uniform || {};
    const normalized = {
      ...value,
      slotLabel: `Slot ${Number.isInteger(value.slot) ? value.slot : '?'}`,
      sideLabel: enumLabel(SIDES, value.sideId, 'Side'),
      designationLabel: enumLabel(DESIGNATIONS, value.designationId, 'Designation'),
      typeLabel: enumLabel(TYPES, value.typeId, 'Type'),
      headbandHomeLabel: enumLabel(HEADBAND_COLORS, value.headbandColorHome, 'Color'),
      headbandAwayLabel: enumLabel(HEADBAND_COLORS, value.headbandColorAway, 'Color'),
      headbandLogoLabel: enumLabel(HEADBAND_LOGOS, value.headbandLogoTypeId, 'Logo'),
      logoTypeLabel: enumLabel(LOGO_TYPES, value.logoTypeId, 'Logo type'),
      logoBrandLabel: enumLabel(LOGO_BRANDS, value.logoBrandId, 'Logo brand'),
      dominantColorLabel: enumLabel(DOMINANT_COLORS, value.dominantColorId, 'Dominant color'),
      topNamePlateLabel: enumLabel(NAMEPLATE_STYLES, value.topNamePlateStyleId, 'Nameplate'),
      bottomNamePlateLabel: enumLabel(NAMEPLATE_STYLES, value.bottomNamePlateStyleId, 'Nameplate'),
      legHomeLabel: enumLabel(HEADBAND_COLORS, value.legAccessoryColorHome, 'Color'),
      legAwayLabel: enumLabel(HEADBAND_COLORS, value.legAccessoryColorAway, 'Color'),
      armHomeLabel: enumLabel(HEADBAND_COLORS, value.armItemColorHome, 'Color'),
      armAwayLabel: enumLabel(HEADBAND_COLORS, value.armItemColorAway, 'Color'),
      sockHomeLabel: enumLabel(HEADBAND_COLORS, value.sockColorHome, 'Color'),
      sockAwayLabel: enumLabel(HEADBAND_COLORS, value.sockColorAway, 'Color'),
      customCasualShirtLabel: enumLabel(HEADBAND_COLORS, value.customCasualShirtColor, 'Route'),
      customJerseyRedLabel: enumLabel(HEADBAND_COLORS, value.customJerseyColorRed, 'Route'),
      customJerseyGreenLabel: enumLabel(HEADBAND_COLORS, value.customJerseyColorGreen, 'Route'),
      customJerseyBlueLabel: enumLabel(HEADBAND_COLORS, value.customJerseyColorBlue, 'Route'),
      customNameRedLabel: enumLabel(HEADBAND_COLORS, value.customNameColorRed, 'Route'),
      customNameGreenLabel: enumLabel(HEADBAND_COLORS, value.customNameColorGreen, 'Route'),
      customNameBlueLabel: enumLabel(HEADBAND_COLORS, value.customNameColorBlue, 'Route'),
      customShoeTrim1Label: enumLabel(HEADBAND_COLORS, value.customShoeTrimColor1, 'Route'),
      customShoeTrim2Label: enumLabel(HEADBAND_COLORS, value.customShoeTrimColor2, 'Route'),
      customShoeDominantLabel: enumLabel(HEADBAND_COLORS, value.customShoeDominantColor, 'Route'),
      primaryColorHex: colorHex(value.primaryColor),
      secondaryColorHex: colorHex(value.secondaryColor),
      tertiaryColorHex: colorHex(value.tertiaryColor),
      primaryAccessoryColorHex: colorHex(value.primaryAccessoryColor),
    };
    for (const field of COLOR_FIELDS) normalized[`${field.key}Hex`] = colorHex(value[field.key]);
    return normalized;
  }

  function argbToInput(value) {
    return `#${((Number(value) || 0) >>> 0).toString(16).padStart(8, '0').slice(-6)}`;
  }

  function inputToArgb(value, original) {
    const rgb = Number.parseInt(String(value || '').replace(/^#/, ''), 16);
    if (!Number.isInteger(rgb) || rgb < 0 || rgb > 0xFFFFFF) return Number(original) >>> 0;
    return (((Number(original) >>> 0) & 0xFF000000) | rgb) >>> 0;
  }

  function element(document, tag, className, text) {
    const node = document.createElement(tag);
    if (className) node.className = className;
    if (text !== undefined) node.textContent = text;
    return node;
  }

  function addFact(document, parent, label, value, className = '') {
    const item = element(document, 'div', `team-uniform-fact ${className}`.trim());
    item.append(element(document, 'small', '', label), element(document, 'strong', '', value || '—'));
    parent.append(item);
  }

  function selectField(document, label, values, selected) {
    const field = element(document, 'label', 'team-uniform-select');
    field.append(element(document, 'small', '', label));
    const select = document.createElement('select');
    values.forEach((text, value) => {
      const option = document.createElement('option');
      option.value = String(value);
      option.textContent = text;
      select.append(option);
    });
    if(Number.isInteger(selected)&&(selected<0||selected>=values.length)){
      const unknown=document.createElement('option');unknown.value=String(selected);unknown.textContent=`Unknown (${selected}) · unchanged`;unknown.disabled=true;select.append(unknown);
    }
    select.value = String(selected);
    field.append(select);
    return { field, select };
  }

  function buildEditPayload(team, uniform, values) {
    const payload = {
      recordIndex: team.recordIndex,
      teamId: team.teamId,
      expectedTeamName: team.name,
      slot: uniform.slot,
      uniformId: uniform.uniformId,
      expectedBaseFilename: uniform.baseFilename,
      expectedHome: uniform.headbandColorHome,
      expectedAway: uniform.headbandColorAway,
      expectedLogo: uniform.headbandLogoTypeId,
      home: Number(values.home),
      away: Number(values.away),
      logo: Number(values.logo),
    };
    for (const field of COLOR_FIELDS) {
      const expected = `expected${field.key[0].toUpperCase()}${field.key.slice(1)}`;
      payload[expected] = Number(uniform[field.key]) >>> 0;
      payload[field.key] = values[field.key] === undefined ? payload[expected] : Number(values[field.key]) >>> 0;
    }
    for (const suffix of ['160', '164', '168', '16C']) payload[`expectedFlags${suffix}`] = Number(uniform[`flags${suffix}`]) >>> 0;
    for (const definition of PACKED_FIELDS) {
      const value = values[definition.key] === undefined ? uniform[definition.key] : values[definition.key];
      payload[definition.key] = Number(value);
    }
    return payload;
  }

  function colorField(document, definition, uniform) {
    let alphaSource=uniform[definition.key];
    const field = element(document, 'label', 'team-uniform-color');
    field.append(element(document, 'small', '', definition.label));
    const row = element(document, 'span', 'team-uniform-color-row');
    const input = document.createElement('input');
    input.type = 'color';
    input.value = argbToInput(uniform[definition.key]);
    input.setAttribute('aria-label', `${definition.label} color`);
    const code = element(document, 'code', '', colorHex(uniform[definition.key]));
    input.addEventListener('input', () => { code.textContent = colorHex(inputToArgb(input.value, uniform[definition.key])); });
    row.append(input, code);
    field.append(row);
    return { field, value: () => inputToArgb(input.value, alphaSource), stage(value){alphaSource=value;input.value=argbToInput(value);code.textContent=colorHex(value);} };
  }

  function render(container, team, summaryElement, options = {}) {
    if (!container) return;
    const transfer=typeof window==='undefined'?require('./team-uniform-transfer'):window.TeamUniformTransfer;
    const uniforms = Array.isArray(team?.uniforms) ? team.uniforms.map(normalizeUniform) : [];
    const editable = team?.source === 'live-team' && typeof options.onSave === 'function';
    container.replaceChildren();
    if (summaryElement) summaryElement.textContent = `${uniforms.length} verified owned slot${uniforms.length === 1 ? '' : 's'} · ${editable ? 'jersey settings editable' : 'read-only'}`;
    if (!uniforms.length) {
      container.append(element(container.ownerDocument, 'div', 'empty', 'No verified uniform slots were resolved for this team.'));
      return;
    }
    const document = container.ownerDocument;
    const models=[];
    for (const uniform of uniforms) {
      const card = element(document, 'article', 'team-uniform-card');
      const header = element(document, 'div', 'team-uniform-card-head');
      const title = element(document, 'div');
      title.append(element(document, 'strong', '', `${uniform.sideLabel} · ${uniform.designationLabel}`));
      title.append(element(document, 'small', 'mono', uniform.baseFilename || uniform.name || 'Filename unavailable'));
      const tags = element(document, 'div', 'team-uniform-tags');
      tags.append(element(document, 'span', 'pill', uniform.slotLabel));
      if (uniform.isAlternate) tags.append(element(document, 'span', 'pill amber', 'Alternate'));
      header.append(title, tags);
      const facts = element(document, 'div', 'team-uniform-facts');
      addFact(document, facts, 'Uniform ID', String(uniform.uniformId ?? '—'), 'mono');
      addFact(document, facts, 'Type', uniform.typeLabel);
      addFact(document, facts, 'Home priority', String(uniform.homePriority ?? '—'));
      addFact(document, facts, 'Away priority', String(uniform.awayPriority ?? '—'));
      addFact(document, facts, 'Primary color', uniform.primaryColorHex, 'mono');
      addFact(document, facts, 'Secondary color', uniform.secondaryColorHex, 'mono');
      addFact(document, facts, 'Tertiary color', uniform.tertiaryColorHex, 'mono');
      addFact(document, facts, 'Primary accessory', uniform.primaryAccessoryColorHex, 'mono');
      card.append(header, facts);
      const mapped = element(document, 'details', 'team-uniform-mapped');
      mapped.append(element(document, 'summary', '', 'Mapped uniform settings'));
      const mappedFacts = element(document, 'div', 'team-uniform-facts');
      addFact(document, mappedFacts, 'Logo type', uniform.logoTypeLabel);
      addFact(document, mappedFacts, 'Logo brand', uniform.logoBrandLabel);
      addFact(document, mappedFacts, 'Dominant color', uniform.dominantColorLabel);
      addFact(document, mappedFacts, 'Scorebug color', uniform.scorebugUsesUniformColor ? 'Uniform color' : 'Team/default');
      addFact(document, mappedFacts, 'Team logo', uniform.usesTeamLogo ? 'Shown' : 'Not used');
      addFact(document, mappedFacts, 'Top nameplate', uniform.topNamePlateLabel);
      addFact(document, mappedFacts, 'Bottom nameplate', uniform.bottomNamePlateLabel);
      addFact(document, mappedFacts, 'Number on shorts', uniform.hasNumberOnShorts ? 'Yes' : 'No');
      addFact(document, mappedFacts, 'Front number', uniform.rightAlignFrontNumber ? 'Right aligned' : 'Standard');
      addFact(document, mappedFacts, 'Arm routing', `${uniform.armHomeLabel} / ${uniform.armAwayLabel}`);
      addFact(document, mappedFacts, 'Leg routing', `${uniform.legHomeLabel} / ${uniform.legAwayLabel}`);
      addFact(document, mappedFacts, 'Sock routing', `${uniform.sockHomeLabel} / ${uniform.sockAwayLabel}`);
      mapped.append(mappedFacts);
      card.append(mapped);
      const resources = element(document, 'details', 'team-uniform-mapped');
      resources.append(element(document, 'summary', '', 'Resource bindings'));
      const resourceFacts = element(document, 'div', 'team-uniform-facts');
      addFact(document, resourceFacts, 'Sponsor logo', uniform.sponsorLogoName || 'Not set', 'mono');
      addFact(document, resourceFacts, 'Localized name tokens', `${uniform.localizedNameToken ?? '—'} / ${uniform.localizedTeamNameToken ?? '—'}`, 'mono');
      addFact(document, resourceFacts, 'Unique signature', String(uniform.uniqueSignatureId ?? '—'), 'mono');
      addFact(document, resourceFacts, 'Sock clothing IDs', `${uniform.sockClothingItemNameId ?? '—'} / ${uniform.sockClothingItemNameAwayId ?? '—'}`, 'mono');
      addFact(document, resourceFacts, 'Bench shirt ID', String(uniform.benchPlayerShirtId ?? '—'), 'mono');
      addFact(document, resourceFacts, 'Sock option IDs', Array.isArray(uniform.sockOptionIds) ? uniform.sockOptionIds.join(', ') : '—', 'mono');
      if (uniform.hasCustomUniformData) {
        addFact(document, resourceFacts, 'Custom uniform', uniform.customUniformName || `ID ${uniform.customUniformUniqueId ?? '—'}`);
        addFact(document, resourceFacts, 'Custom layers / options', `${uniform.customUniformNumberOfLayers ?? '—'} / ${uniform.customUniformNumberOfOptions ?? '—'}`);
        addFact(document, resourceFacts, 'Baked jersey', uniform.bakedJerseyFilename || 'Not set', 'mono');
        addFact(document, resourceFacts, 'Baked shorts', uniform.bakedShortsFilename || 'Not set', 'mono');
      }
      resources.append(resourceFacts);
      card.append(resources);
      const creator = element(document, 'details', 'team-uniform-mapped');
      creator.append(element(document, 'summary', '', 'Custom creator routing'));
      const creatorFacts = element(document, 'div', 'team-uniform-facts');
      addFact(document, creatorFacts, 'Casual shirt', uniform.customCasualShirtLabel);
      addFact(document, creatorFacts, 'Jersey RGB routes', `${uniform.customJerseyRedLabel} / ${uniform.customJerseyGreenLabel} / ${uniform.customJerseyBlueLabel}`);
      addFact(document, creatorFacts, 'Name RGB routes', `${uniform.customNameRedLabel} / ${uniform.customNameGreenLabel} / ${uniform.customNameBlueLabel}`);
      addFact(document, creatorFacts, 'Shoe routes', `${uniform.customShoeTrim1Label} / ${uniform.customShoeTrim2Label} / ${uniform.customShoeDominantLabel}`);
      creator.append(creatorFacts);
      card.append(creator);
      if (editable) {
        const editor = element(document, 'div', 'team-uniform-headband-editor');
        const home = selectField(document, 'Headband home', HEADBAND_COLORS, uniform.headbandColorHome);
        const away = selectField(document, 'Headband away', HEADBAND_COLORS, uniform.headbandColorAway);
        const logo = selectField(document, 'Headband logo', HEADBAND_LOGOS, uniform.headbandLogoTypeId);
        const button = element(document, 'button', 'secondary team-uniform-save', 'Save & verify');
        button.type = 'button';
        const settings = element(document, 'details', 'team-uniform-palette');
        settings.append(element(document, 'summary', '', 'Edit mapped uniform settings'));
        const settingsGrid = element(document, 'div', 'team-uniform-color-grid team-uniform-setting-grid');
        const settingControls = {};
        let settingGroup = '';
        for (const definition of PACKED_FIELDS) {
          if (definition.group !== settingGroup) {
            settingGroup = definition.group;
            settingsGrid.append(element(document, 'h4', '', settingGroup));
          }
          const control = selectField(document, definition.label, definition.values, Number(uniform[definition.key]));
          settingControls[definition.key] = control;
          settingsGrid.append(control.field);
        }
        settings.append(settingsGrid);
        const palette = element(document, 'details', 'team-uniform-palette');
        palette.append(element(document, 'summary', '', 'Edit uniform and accessory colors'));
        const paletteGrid = element(document, 'div', 'team-uniform-color-grid');
        const colorControls = {};
        let group = '';
        for (const definition of COLOR_FIELDS) {
          if (definition.group !== group) {
            group = definition.group;
            paletteGrid.append(element(document, 'h4', '', group));
          }
          const control = colorField(document, definition, uniform);
          colorControls[definition.key] = control;
          paletteGrid.append(control.field);
        }
        palette.append(paletteGrid);
        const values=()=>{
          const v={home:Number(home.select.value),away:Number(away.select.value),logo:Number(logo.select.value)};
          for(const f of COLOR_FIELDS)v[f.key]=colorControls[f.key].value();
          for(const f of PACKED_FIELDS)v[f.key]=Number(settingControls[f.key].select.value);
          return v;
        };
        models.push({uniform,values,stage(v){
          for(const [key,value] of Object.entries(v)){
            if(colorControls[key])colorControls[key].stage(value);
            else if(settingControls[key])settingControls[key].select.value=String(value);
            else ({home,away,logo})[key].select.value=String(value);
          }
          settings.open=palette.open=true;
        }});
        const review=element(document,'button','secondary small','Review changes');review.type='button';
        const beforeValues={home:uniform.headbandColorHome,away:uniform.headbandColorAway,logo:uniform.headbandLogoTypeId,...Object.fromEntries([...COLOR_FIELDS,...PACKED_FIELDS].map(f=>[f.key,uniform[f.key]]))};
        review.addEventListener('click',()=>{const model=models.find(m=>m.uniform===uniform);globalThis.EditorReview?.open({title:'Review uniform changes',rows:Object.entries(values()).filter(([key,v])=>v!==beforeValues[key]).map(([key,v])=>({label:[...COLOR_FIELDS,...PACKED_FIELDS].find(f=>f.key===key)?.label||key,before:colorControls[key]?colorHex(beforeValues[key]):beforeValues[key],after:colorControls[key]?colorHex(v):v,remove:()=>{if(button.disabled)return false;model.stage({[key]:beforeValues[key]});return true;}}))});});
        button.addEventListener('click', () => {
          const values = { home: home.select.value, away: away.select.value, logo: logo.select.value };
          for (const definition of COLOR_FIELDS) values[definition.key] = colorControls[definition.key].value();
          for (const definition of PACKED_FIELDS) values[definition.key] = settingControls[definition.key].select.value;
          options.onSave(buildEditPayload(team, uniform, values), button);
        });
        const uniformActions=element(document,'div','team-uniform-actions');uniformActions.append(button,review);
        editor.append(home.field, away.field, logo.field, uniformActions, settings, palette);
        card.append(editor);
      }
      container.append(card);
    }
    return {
      drafts(exceptSlot){return models.filter(m=>m.uniform.slot!==exceptSlot).map(m=>{
        const baseline={home:m.uniform.headbandColorHome,away:m.uniform.headbandColorAway,logo:m.uniform.headbandLogoTypeId,...Object.fromEntries([...COLOR_FIELDS,...PACKED_FIELDS].map(f=>[f.key,m.uniform[f.key]]))};
        return {slot:m.uniform.slot,uniformId:m.uniform.uniformId,baseFilename:m.uniform.baseFilename,values:Object.fromEntries(Object.entries(m.values()).filter(([key,v])=>v!==baseline[key]))};
      }).filter(row=>Object.keys(row.values).length);},
      restoreDrafts(rows){for(const row of rows){const m=models.find(m=>m.uniform.slot===row.slot&&m.uniform.uniformId===row.uniformId&&m.uniform.baseFilename===row.baseFilename);if(m)m.stage(transfer.values(row.values));}},
      snapshot(){return models.map(m=>({match:{typeId:m.uniform.typeId,sideId:m.uniform.sideId,designationId:m.uniform.designationId},values:Object.fromEntries(Object.entries(m.values()).filter(([key,v])=>{try{transfer.values({[key]:v});return true;}catch{return false;}}))}));},
      stage(rows){const validated=transfer.validate(rows),assignments=[],used=new Set(),warnings=[];
        for(const row of validated){const found=models.filter(m=>transfer.matches(row.match,m.uniform));if(found.length!==1||used.has(found[0])){warnings.push('A missing, ambiguous or duplicate uniform slot was left unchanged.');continue;}used.add(found[0]);assignments.push([found[0],row.values]);}
        for(const [model,v] of assignments)model.stage(v);return warnings;
      }
    };
  }

  function buildLinkPayload(team, uniform, slot) {
    return {
      recordIndex: team.recordIndex,
      teamId: team.teamId,
      expectedTeamName: team.name,
      slot: Number(slot),
      uniformRecordIndex: uniform.recordIndex,
      uniformId: uniform.uniformId,
      expectedBaseFilename: uniform.baseFilename,
    };
  }

  function renderAvailable(container, team, candidates, summaryElement, options = {}) {
    if (!container) return;
    const document = container.ownerDocument;
    const available = (Array.isArray(candidates) ? candidates : [])
      .filter((uniform) => uniform?.stockResourceAvailable !== false)
      .sort((left, right) => String(left.baseFilename || '').localeCompare(String(right.baseFilename || '')));
    const emptySlots = Array.isArray(team?.emptyUniformSlots) ? [...team.emptyUniformSlots].sort((a, b) => a - b) : [];
    const editable = team?.source === 'live-team' && emptySlots.length > 0 && typeof options.onLink === 'function';
    container.replaceChildren();
    if (summaryElement) {
      summaryElement.textContent = emptySlots.length
        ? `${available.length} stock record${available.length === 1 ? '' : 's'} · ${emptySlots.length} empty slot${emptySlots.length === 1 ? '' : 's'}`
        : 'No empty team slots';
    }
    if (!available.length || !emptySlots.length) {
      container.append(element(document, 'div', 'empty', emptySlots.length
        ? 'No unreferenced same-team jerseys with verified stock art were found.'
        : 'This team has no empty uniform slots.'));
      return;
    }
    for (const uniform of available) {
      const card = element(document, 'article', 'team-uniform-card team-uniform-available-card');
      const header = element(document, 'div', 'team-uniform-card-head');
      const title = element(document, 'div');
      title.append(element(document, 'strong', '', uniform.baseFilename || 'Unnamed uniform'));
      title.append(element(document, 'small', 'mono', `Roster record #${uniform.recordIndex}`));
      const tags = element(document, 'div', 'team-uniform-tags');
      tags.append(element(document, 'span', 'pill', enumLabel(TYPES, uniform.typeId, 'Type')));
      header.append(title, tags);
      const facts = element(document, 'div', 'team-uniform-facts');
      addFact(document, facts, 'Uniform ID', String(uniform.uniformId ?? '—'), 'mono');
      addFact(document, facts, 'Designation', enumLabel(DESIGNATIONS, uniform.designationId, 'Designation'));
      addFact(document, facts, 'Team ID', String(uniform.teamId ?? '—'), 'mono');
      addFact(document, facts, 'Stock art', uniform.stockResourceAvailable ? 'Verified' : 'Unavailable');
      const controls = element(document, 'div', 'team-uniform-link-controls');
      const slot = selectField(document, 'Add to empty slot', emptySlots.map((value) => `Slot ${value}`), 0);
      slot.select.replaceChildren(...emptySlots.map((value) => {
        const option = document.createElement('option');
        option.value = String(value);
        option.textContent = `Slot ${value}`;
        return option;
      }));
      slot.select.value = String(emptySlots[0]);
      const button = element(document, 'button', 'secondary team-uniform-save', 'Add & verify');
      button.type = 'button';
      button.disabled = !editable;
      button.addEventListener('click', () => options.onLink(buildLinkPayload(team, uniform, slot.select.value), button));
      controls.append(slot.field, button);
      card.append(header, facts, controls);
      container.append(card);
    }
  }

  return { TYPES, SIDES, DESIGNATIONS, HEADBAND_COLORS, HEADBAND_LOGOS, LOGO_TYPES, LOGO_BRANDS, DOMINANT_COLORS, NAMEPLATE_STYLES, PACKED_FIELDS, COLOR_FIELDS, colorHex, argbToInput, inputToArgb, normalizeUniform, buildEditPayload, buildLinkPayload, render, renderAvailable };
});
