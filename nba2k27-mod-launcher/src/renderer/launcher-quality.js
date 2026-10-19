(()=>{
 const api=window.modLauncher,$=id=>document.getElementById(id);if(!api)return;
 const host=document.querySelector('#page-settings .settings-page-layout');
 let last=null,loading=false;
 function cell(row,text,tag='td'){const node=document.createElement(tag);node.textContent=String(text??'—');row.append(node);return node;}
 function table(headers,rows){const t=document.createElement('table');t.className='quality-table';const head=document.createElement('tr');for(const h of headers)cell(head,h,'th');t.append(head);for(const values of rows){const row=document.createElement('tr');for(const value of values)cell(row,value);t.append(row);}return t;}
 function replace(id,content){const node=$(id),top=node.scrollTop,left=node.scrollLeft;node.replaceChildren(content);node.scrollTop=top;node.scrollLeft=left;}
 if(host&&api.launcherHealth){
  const card=document.createElement('article');card.className='card settings-path-card';card.id='launcherHealthCard';
  card.innerHTML='<div class="settings-card-heading"><strong>Diagnostics & compatibility</strong><button type="button" class="secondary" id="refreshLauncherHealth">Refresh</button></div><p id="launcherCompatibilitySummary"></p><details><summary>Feature compatibility</summary><div class="quality-scroll" id="launcherCompatibilityRows"></div></details><div class="quality-scroll" id="launcherHelperRows"></div><p class="settings-description">Latest reported helper states, with timestamps—not a guarantee that a pending effect is already visible in-game.</p><details><summary>Performance measurements</summary><p>Host startup, shutdown cleanup, and catalog click-to-visible time. A regression needs at least three samples per build and a median increase above 30% and 100 ms. QA tests are kept separate.</p><div class="quality-scroll" id="launcherPerformanceRows"></div></details><div class="settings-support-actions"><button type="button" class="secondary" id="copyLauncherHealth">Copy diagnostic report</button><small id="launcherHealthMessage" role="status" aria-live="polite"></small></div>';
  host.insertBefore(card,host.children[1]||null);
  $('refreshLauncherHealth').onclick=()=>refresh();$('copyLauncherHealth').onclick=async()=>{try{await api.copyLauncherHealth();$('launcherHealthMessage').textContent='Diagnostic report copied. Local paths are redacted; nothing was uploaded.';}catch(e){$('launcherHealthMessage').textContent=e.message;}};
 }
 async function refresh(){if(loading||!api.launcherHealth||!$('launcherHealthCard'))return;loading=true;
  try{last=await api.launcherHealth();$('launcherCompatibilitySummary').textContent=(last.compatibility.build?last.compatibility.build+' · ':'')+last.compatibility.message;
   replace('launcherCompatibilityRows',table(['Feature','Support','Details'],last.compatibility.features.map(r=>[r.label,r.supported?'Recognized':'Unavailable',r.reason])));
   replace('launcherHelperRows',table(['Helper','State','Latest report','Updated'],last.helpers.map(r=>[r.name,r.status,r.message,r.updatedAt?new Date(r.updatedAt).toLocaleTimeString():'Not observed'])));
   const ms=n=>n===null?'—':n+' ms';replace('launcherPerformanceRows',table(['Operation','Samples','Latest','Median','Previous build median','Trend'],last.performance.map(r=>[r.name,r.count,ms(r.lastMs),ms(r.medianMs),ms(r.baselineMs),r.regression?'Slower—investigate':r.count<3?'Collecting samples':'Recorded'])));
   for(const [id,feature] of Object.entries(controls)){const button=$(id),row=last.compatibility.features.find(r=>r.key===feature);if(!button||!row)continue;button.classList.toggle('quality-disabled',!row.supported);if(!row.supported){if(!button.hasAttribute('data-quality-title'))button.setAttribute('data-quality-title',button.title||'');button.setAttribute('aria-disabled','true');button.title=row.reason;}else if(button.hasAttribute('data-quality-title')){button.title=button.getAttribute('data-quality-title');button.removeAttribute('data-quality-title');button.removeAttribute('aria-disabled');}}
  }catch(e){$('launcherHealthMessage').textContent=e.message;}finally{loading=false;}
 }
 const controls={launchGame:'session',launchWithoutEac:'session',startSession:'session',applyScaler:'scaler',ballMenuButton:'ball',arenaMenuButton:'arena',scoreboardMenuButton:'scoreboard',openBlacktopRules:'rules',editBlacktopPlayer:'outfits'};
 window.addEventListener('launcher:settings-refreshed',()=>{last=null;refresh();});
 // Renderer feedback complements (never replaces) the backend and native guards.
 document.addEventListener('click',e=>{const button=e.target.closest('button');const feature=button&&controls[button.id],row=last?.compatibility.features.find(r=>r.key===feature);if(row&&!row.supported){e.preventDefault();e.stopImmediatePropagation();$('launcherHealthMessage')&&($('launcherHealthMessage').textContent=row.label+': '+row.reason);refresh();}},true);
 if(api.launcherHealth&&host){refresh();setInterval(()=>{if($('page-settings')?.classList.contains('active'))refresh();},5000);}
})();
