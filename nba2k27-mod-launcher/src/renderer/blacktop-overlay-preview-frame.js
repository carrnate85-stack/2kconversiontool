(() => {
  window.addEventListener('error',event=>{document.body.dataset.previewError=event.message;});
  // Display-only copy of the helper's labels; test/blacktop-overlay-preview.test.js
  // checks every label and field width against unlocks-helper/GameRuleData.cs.
  const rules=[
    ['Shot Clock',1],['Out Of Bounds',1],['Backcourt Violation',1],['8 Sec Half Court',1],
    ['5 Sec Inbound',1],['3 in the Key',4],['Def 3 in the Key',4],['5 Sec Back To Basket',1],
    ['5 Sec Closely Guarded',1],['Over the Back Foul',1],['Generic Personal Foul',1],
    ['Charging Foul',1],['Offensive Offball Foul',1],['Defensive Offball Foul',1],
    ['Blocking Foul',1],['Reaching Foul',1],['Loose Ball Foul',1],['Shooting Foul',1],
    ['Intentional Foul',1],['Clear Path Foul',1],['Transition Take Foul',1],['Kick Ball',1],
    ['Goaltending',1],['Illegal Pick',1],['Traveling',1],['Double Dribble',1],
    ['Foul Out',4],['3Pt Shots',4],['Zone Defense Allowed',1],['No Charge Zone',1],
    ['Buzzer Beater Review',1],['Clear Ball Required',1],['Timeouts',1],
    ['Screen Set Out of Bounds',1],['Offensive Player Out of Bounds',1],
    ['Ball Entered Basket From Below',1],['Technical Foul',1],['Flagrant Foul',1],
    ['Technical Foul Ejection',1]
  ];
  const message=(type,detail={})=>parent.postMessage({type,...detail},'*');
  const byId=id=>document.getElementById(id);
  let library=null,previewSnapshot=null,thumbnailId=0;
  let scoreboardState={selected:'default',phase:'ready',message:'Preview only',choices:[]};
  let ballState={selected:'current',phase:'ready',message:'Preview only',choices:[]};
  const panelShown=new Set();
  const thumbnailRequests=new Map();
  const thumbnail=asset=>new Promise(resolve=>{
    const id=++thumbnailId,timeout=setTimeout(()=>{thumbnailRequests.delete(id);resolve(null);},5000);
    thumbnailRequests.set(id,{resolve,timeout,asset});message('blacktop-preview-thumbnail',{id,asset});
  });
  const initializePlayer=(data,draft={})=>{
    if(previewSnapshot)return;
    document.body.dataset.previewLibrary=String(data.catalog?.length||0);
    document.body.dataset.previewCallbacks=String(panelShown.size);
    library=data;
    const top=data.catalog.find(p=>p.id===draft.streetId&&p.pool==='male')||data.catalog.find(p=>p.pool==='male'&&p.separable&&p.shoes?.length===1&&!p.experimental);
    if(!top)return;
    document.body.dataset.previewPlayer=top.id;
    previewSnapshot={token:'preview-only',pid:0,streetwear:data.catalog,accessories:data.accessories,shoes:data.shoes,shoeLabels:data.shoeLabels,
      players:[{slot:0,name:'Preview player',clothingPool:'male',cyberface:0,streetId:top.id,streetBottom:draft.bottom||top.bottom,
        streetShoe:draft.shoe||'',streetSock:draft.sock||'',streetAccessories:[],outfits:[{id:'preview-original',items:[top.top,top.bottom]}],selected:'preview-original'}]};
    for(const callback of panelShown)callback();
  };
  const snapshotView=options=>structuredClone(options?.omitCatalog===true
    ?{token:previewSnapshot?.token,players:previewSnapshot?.players||[]}:previewSnapshot);
  const applyPreview=async args=>{
    if(!previewSnapshot)throw Error('Preview player is still loading.');
    const player=previewSnapshot.players.find(p=>p.slot===args.slot);
    let choice={streetId:player.streetId,bottom:player.streetBottom,shoe:player.streetShoe,sock:player.streetSock,layer:player.streetLayer||'',accessories:player.streetAccessories};
    if(args.outfitPresetId){const preset=library.outfits.find(p=>p.id===args.outfitPresetId&&p.available&&p.pool==='male');if(preset)choice={...preset};}
    else if(args.randomFit){
      if(args.randomMode==='accessories')choice=window.MannequinRandom.accessories(choice,library);
      else if(args.randomMode!=='team'){choice=window.MannequinRandom.clothes(choice,library);if(args.randomMode==='all')choice=window.MannequinRandom.accessories(choice,library);}
    }else if(args.streetId)choice={...choice,streetId:args.streetId,bottom:args.bottom,shoe:args.shoe,sock:args.sock,layer:args.layer,accessories:args.accessories||[]};
    Object.assign(player,{streetId:choice.streetId,streetBottom:choice.bottom,streetShoe:choice.shoe||'',streetSock:choice.sock||'',streetLayer:choice.layer||'',streetAccessories:[...(choice.accessories||[])]});
    return {...snapshotView(args),report:{unchanged:true,matchCarryover:'queued'}};
  };
  const showView=view=>{
    const expanded=view!=='compact';
    document.body.classList.toggle('compact-bar',!expanded);
    document.body.classList.toggle('rules-open',view==='rules');
    byId('blacktopRulesPane').hidden=view!=='rules';
    byId('collapseBlacktopPanel').hidden=!expanded;
    byId('closeBlacktopEditor').hidden=!expanded;
    message('blacktop-preview-view',{view});
  };
  const load=async()=>{
    const response=await fetch('blacktop-panel.html',{cache:'no-store'});
    if(!response.ok)throw Error(`F8 overlay source unavailable (${response.status}).`);
    const source=new DOMParser().parseFromString(await response.text(),'text/html');
    const styles=[...source.head.querySelectorAll('link[rel="stylesheet"]')].map(link=>new Promise((resolve,reject)=>{
      const copy=document.importNode(link,true);
      copy.addEventListener('load',resolve,{once:true});
      copy.addEventListener('error',()=>reject(Error(`F8 overlay style unavailable: ${link.getAttribute('href')}`)),{once:true});
      document.head.append(copy);
    }));
    const previewStyle=document.createElement('link');previewStyle.rel='stylesheet';previewStyle.href='blacktop-overlay-preview-frame.css';
    styles.push(new Promise((resolve,reject)=>{previewStyle.addEventListener('load',resolve,{once:true});previewStyle.addEventListener('error',()=>reject(Error('F8 preview style unavailable.')),{once:true});document.head.append(previewStyle);}));
    await Promise.all(styles);
    document.body.className=`${source.body.className} blacktop-preview-source`;
    document.body.replaceChildren(...[...source.body.children].filter(child=>child.tagName!=='SCRIPT').map(child=>document.importNode(child,true)));
    byId('blacktopStatus').textContent='';
    byId('blacktopBarStatus').lastChild.textContent='Ready';
    byId('blacktopPlayerSelect').innerHTML='<option>Select player</option>';
    // Run the actual F8 outfit renderer against a local dummy snapshot. No
    // live game API, settings writes or preset saves are exposed to this frame.
    if(parent.__wpfRenderer)window.__wpfRenderer={};
    window.modLauncher={__previewOnly:true,
      scoreboardStatus:async()=>scoreboardState,
      onScoreboardStatus:callback=>{window.previewScoreboardStatus=callback;},
      ballStatus:async()=>ballState,
      onBallStatus:callback=>{window.previewBallStatus=callback;},
      ballThumbnail:async()=>({state:'missing'}),
      listOutfitPresets:async options=>options?.omitCatalog===true?{outfits:library?.outfits||[]}:library||{outfits:[]},
      scanBlacktop:async options=>snapshotView(options),
      applyBlacktop:applyPreview,
      expandBlacktopPanel:async expanded=>{showView(expanded?(document.body.classList.contains('rules-open')?'rules':'outfit'):'compact');return {expanded};},
      hideBlacktopPanel:async()=>message('blacktop-preview-close'),
      onBlacktopPanelShown:callback=>{panelShown.add(callback);},
      onBlacktopPanelCollapsed:()=>{},
      sizeBlacktopPlayerMenu:async rows=>{message('blacktop-preview-view',{view:rows?'menu':'compact'});},
      clothingThumbnail:thumbnail,cachedClothingThumbnail:thumbnail};
    for(const name of ['../shared/game-rule-values.js','preview-image-url.js','shoe-preview-catalog.js','../shared/accessory-slots.js','../shared/mannequin-random.js',
      'accessory-controls.js','clothing-catalog-ui.js','sock-preview-catalog.js','accessory-preview-catalog.js','top-preview-catalog.js','bottom-preview-catalog.js','clothing-picker.js',
      'blacktop-batch.js','blacktop.js','scoreboard-preview-catalog.js','scoreboard-picker.js','ball-picker.js','sidebar-icons.js','blacktop-player-menu.js','blacktop-live-bar.js']){
      await new Promise((resolve,reject)=>{
        const script=document.createElement('script');script.src=name;script.onload=resolve;script.onerror=()=>reject(Error(`${name} unavailable`));document.body.append(script);
      });
    }
    byId('refreshBlacktopRules').disabled=true;
    byId('blacktopRulesStatus').textContent='Preview only · current values appear when the F8 overlay is opened in game.';
    const list=byId('blacktopRulesList');
    for(const [label,width] of rules){
      const row=document.createElement('div');row.className='blacktop-rule';
      const name=document.createElement('span');name.className='blacktop-rule-name';name.textContent=label;
      const controls=document.createElement('div');controls.className='blacktop-rule-controls';
      if(width===1){
        for(const value of ['Off','On']){const button=document.createElement('button');button.type='button';button.textContent=value;button.disabled=true;controls.append(button);}
      }else{
        const select=document.createElement('select');select.setAttribute('aria-label',label);select.disabled=true;
        const current=document.createElement('option');current.textContent='Live value';select.append(current);
        for(const option of window.GameRuleValues.optionsForLabel(label,width)){
          const choice=document.createElement('option');choice.value=String(option.value);choice.textContent=option.label;select.append(choice);
        }
        controls.append(select);
      }
      row.append(name,controls);list.append(row);
    }
    byId('openBlacktopRules').addEventListener('click',()=>window.BlacktopOverlay.expand(document.body.dataset.overlayView!=='openBlacktopRules','openBlacktopRules'));
    document.addEventListener('keydown',event=>{
      if(event.key==='Escape'&&!event.defaultPrevented&&!document.querySelector('.clothing-picker[open]')&&!document.body.classList.contains('compact-bar')){
        event.preventDefault();window.BlacktopOverlay.expand(false);
      }
    });
    window.addEventListener('message',event=>{
      if(event.source!==parent||event.origin!==location.origin)return;
      if(event.data?.type==='blacktop-preview-thumbnail-result'){
        const pending=thumbnailRequests.get(event.data.id);if(!pending||pending.asset!==event.data.asset)return;
        clearTimeout(pending.timeout);thumbnailRequests.delete(event.data.id);pending.resolve(event.data.result);return;
      }
      if(event.data?.type!=='blacktop-preview-choices')return;
      for(const [id,value] of [['arenaMenuButton',event.data.arena],['scoreboardMenuButton',event.data.scoreboard],['ballMenuButton',event.data.ball]])
        byId(id).textContent=typeof value==='string'?value:'Not selected';
      if(event.data.scoreboardState){scoreboardState=event.data.scoreboardState;window.previewScoreboardStatus?.(scoreboardState);}
      if(event.data.ballState){ballState=event.data.ballState;window.previewBallStatus?.(ballState);}
      if(event.data.library)initializePlayer(event.data.library,event.data.draft);
    });
    document.body.dataset.previewReady='true';
    message('blacktop-preview-ready');
  };
  load().catch(error=>{document.body.textContent=`F8 preview could not load: ${error.message}`;message('blacktop-preview-error',{message:error.message});});
})();
