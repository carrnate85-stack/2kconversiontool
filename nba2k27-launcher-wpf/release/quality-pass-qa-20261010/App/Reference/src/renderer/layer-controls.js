// Catalog observations are not a proof of body/mesh compatibility.
(() => {
  const label=value=>value==='none'?'No layer':value.replace(/^female_/,'').replaceAll('_',' ');
  window.LayerControls={fill(select,browse,preset,selected='') {
    const current=preset?.layers?.[0]||'none';
    const values=[...new Set(preset?.layerChoices||[])].filter(v=>v!==current);
    const options=[{value:'',text:preset?`${label(current)} (stock)`:'Choose streetwear first'},...values.map(value=>({value,text:label(value)}))];
    select.replaceChildren(...options.map(o=>{const n=document.createElement('option');n.value=o.value;n.textContent=o.text;return n;}));
    select.value=options.some(o=>o.value===selected)?selected:'';
    const disabled=!preset||!values.length||preset.layers?.length>1;
    select.disabled=browse.disabled=disabled;
    const message=!preset?'Select a streetwear top to inspect its layers.':values.length
      ?(preset.layerChoices.includes('none')?'With/without layer observed in catalog.':'Alternative layers observed; removal unverified.')+' Check the fit in game.'
      :preset.layers?.length?'Layer kept: requirement unverified. No alternative observed for this exact top.'
        :'Standalone in the catalog; no layer control needed.';
    select.title=browse.title=message;return message;
  },label};
})();
