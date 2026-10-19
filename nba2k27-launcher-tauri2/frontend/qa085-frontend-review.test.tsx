import { afterEach, describe, expect, it, vi } from "vitest";
const hooks = vi.hoisted(() => ({ active: null as any }));
vi.mock("react", async () => ({ ...await vi.importActual<any>("react"),
  useState: (value: any) => hooks.active.state(value),
  useRef: (value: any) => hooks.active.ref(value),
  useEffect: (effect: any, deps: any) => hooks.active.effect(effect, deps),
}));
vi.mock("./api", () => ({call: vi.fn(), garmentImage: () => undefined}));
vi.mock("@tauri-apps/api/event", () => ({listen:vi.fn(async()=>()=>{})}));
vi.mock("./NativeOutfits", () => ({canApplyOutfit:()=>false,useNativeOutfits:vi.fn()}));
import { call } from "./api";
import { SettingsSaver } from "./settings-save";
import { finishModInstall } from "./mod-install-feedback";
import { SeasonNumber, seasonName } from "./SeasonHistory";
import { OverlayOutfitEditor, ClothingPicker } from "./OverlayOutfitEditor";
import { NativeOverlayControls } from "./NativeOverlay";
import { LauncherUpdatesCard } from "./SettingsTools";
class View {
  slots:any[]=[]; cursor=0; pending:(()=>void)[]=[];
  state(value:any){const i=this.cursor++;if(!(i in this.slots))this.slots[i]=typeof value==='function'?value():value;return [this.slots[i],(next:any)=>{this.slots[i]=typeof next==='function'?next(this.slots[i]):next;}];}
  ref(value:any){const i=this.cursor++;if(!(i in this.slots))this.slots[i]={current:value};return this.slots[i];}
  effect(fn:any,deps:any[]){const i=this.cursor++,old=this.slots[i];if(!old||!deps||deps.some((v,j)=>!Object.is(v,old.deps[j]))){this.pending.push(()=>{old?.cleanup?.();this.slots[i]={deps,cleanup:fn()};});}}
  render(fn:()=>any){this.cursor=0;this.pending=[];hooks.active=this;const tree=fn();hooks.active=null;for(const effect of this.pending)effect();return tree;}
  dispose(){for(const s of this.slots)s?.cleanup?.();}
}
function nodes(value:any):any[]{if(!value||typeof value!=='object')return [];return [value,...[value.props?.children].flat(Infinity).flatMap(nodes)];}
const flush=async()=>{await Promise.resolve();await Promise.resolve();await Promise.resolve();};
afterEach(()=>{vi.resetAllMocks();vi.useRealTimers();vi.unstubAllGlobals();});
it("settings failure reloads persisted root without losing a newer queued patch or backend event",async()=>{
 const seen:any[]=[],failed:any[]=[];let first!:(v:any)=>void,second!:(v:any)=>void;
 const save=vi.fn().mockImplementationOnce(()=>new Promise((_,reject)=>first=reject)).mockImplementationOnce(()=>new Promise(resolve=>second=resolve));
 const q=new SettingsSaver({publish:v=>seen.push(v),save,load:async()=>({root:'old',theme:'dark'}),saved:vi.fn(),failed:e=>failed.push(e)});
 q.accept({root:'old',theme:'dark'});q.save({root:'new'});const done=q.save({theme:'light'});await flush();q.accept({root:'old',theme:'dark'});expect(seen.at(-1)).toEqual({root:'new',theme:'light'});
 first(Error('disk full'));await flush();expect(seen.at(-1)).toEqual({root:'old',theme:'light'});expect(save).toHaveBeenCalledTimes(2);second({});await done;expect(seen.at(-1)).toEqual({root:'old',theme:'light'});expect(failed).toHaveLength(1);
});
it("mod commit completion survives refresh failure and distinguishes the two results",async()=>{
 const messages:string[]=[];await finishModInstall('2 mods installed.',async()=>{throw Error('scan unavailable');},s=>messages.push(s));expect(messages[0]).toBe('2 mods installed.');expect(messages.at(-1)).toContain('2 mods installed. Library refresh failed:');
});
it("number clear never commits zero; explicit zero still commits; invalid year has a truthful label",()=>{
 const view=new View(),commit=vi.fn();const render=()=>view.render(()=>SeasonNumber({label:'Total',value:3,disabled:false,commit}));let input=render();const target={setCustomValidity:vi.fn()};input.props.onChange({target:{value:''},currentTarget:target});input=render();input.props.onBlur({currentTarget:target});expect(commit).not.toHaveBeenCalled();expect(target.setCustomValidity).toHaveBeenCalledWith(expect.stringContaining('valid number'));
 input=render();input.props.onChange({target:{value:'0'},currentTarget:target});input=render();input.props.onBlur({currentTarget:target});expect(commit).toHaveBeenCalledWith(0);expect(seasonName(0)).toBe('Invalid season year');expect(seasonName(2026)).toBe('2025–26');
});
it("late outfit pick cannot overwrite a removed draft or closed picker and duplicate clicks invoke once",async()=>{
 for(const change of ['remove','close']){
  const view=new View(),setOutfit=vi.fn();let reply!:(v:any)=>void;vi.mocked(call).mockImplementation((channel)=>channel==='select-outfit-piece'?new Promise(resolve=>reply=resolve):Promise.resolve(channel==='outfit-pieces'?[]:null));
  let outfit:any={pool:'male',top:'old',bottom:'',socks:'',shoes:'',accessories:[]};const control:any={binding:'player1',status:null,eligible:false,busy:false};
  const render=()=>view.render(()=>OverlayOutfitEditor({live:false,control,outfit,setOutfit,message:'',setMessage:vi.fn(),restore:vi.fn()}));render();view.slots[2]={slot:'top',name:'Top'};let tree=render();let picker=nodes(tree).find(n=>n.type===ClothingPicker);const pending=picker.props.onChoose('new');picker.props.onChoose('another');expect(vi.mocked(call).mock.calls.filter(c=>c[0]==='select-outfit-piece')).toHaveLength(1);
  if(change==='close')picker.props.onClose();else outfit={...outfit,top:''};render();reply({...outfit,top:'new'});await pending;expect(setOutfit).not.toHaveBeenCalled();view.dispose();vi.clearAllMocks();
 }
});
it("wardrobe errors are visible and successful later query clears them",async()=>{
 vi.stubGlobal('window',{addEventListener:vi.fn(),removeEventListener:vi.fn()});vi.useFakeTimers();vi.mocked(call).mockRejectedValueOnce(Error('wardrobe unavailable')).mockResolvedValue({items:[],total:0,groups:[]});const view=new View();const props={picker:{slot:'top',name:'Top'},playerName:'A',pool:'male',selected:'',pending:false,onChoose:vi.fn(),onClose:vi.fn()};const render=()=>view.render(()=>ClothingPicker(props));render();await vi.runAllTimersAsync();let tree=render();expect(nodes(tree).find(n=>n.props?.role==='alert')?.props.children.join('')).toContain('wardrobe unavailable');const search=nodes(tree).find(n=>n.props?.type==='search');search.props.onChange({target:{value:'shirt'}});render();await vi.runAllTimersAsync();tree=render();expect(nodes(tree).some(n=>n.props?.role==='alert')).toBe(false);view.dispose();
});
it("overlay transient polling error clears on recovered status without clearing action errors",async()=>{
 vi.useFakeTimers();vi.mocked(call).mockRejectedValueOnce(Error('poll failed')).mockResolvedValue({message:'Recovered',phase:'ready'});const view=new View();const render=()=>view.render(()=>NativeOverlayControls({gameRoot:'C:/Game',running:true}));render();await flush();let tree=render();expect(JSON.stringify(tree)).toContain('poll failed');await vi.advanceTimersByTimeAsync(1000);tree=render();expect(JSON.stringify(tree)).not.toContain('poll failed');expect(JSON.stringify(tree)).toContain('Recovered');
 vi.mocked(call).mockImplementation(channel=>channel==='native-overlay-enable'?Promise.reject(Error('action refused')):Promise.resolve({message:'Recovered',phase:'ready'}));
 nodes(tree).find(n=>n.type==='button'&&n.props.children==='Enable native F8').props.onClick();await flush();await vi.advanceTimersByTimeAsync(1000);tree=render();expect(JSON.stringify(tree)).toContain('action refused');view.dispose();
});
it("update card keeps prior build after failed follow-up refresh and offers retry on initial failure",async()=>{
 const info={version:'1',channel:'dev',notes:[],pending:null,lastResult:null,builtAt:'',busy:false,recoveryNeeded:false,previous:null,portable:true};vi.mocked(call).mockResolvedValueOnce(info).mockRejectedValue(Error('refresh offline'));const view=new View();const render=()=>view.render(()=>LauncherUpdatesCard({setStatus:vi.fn()}));render();await flush();let tree=render();expect(JSON.stringify(tree)).toContain('1');const buttons=nodes(tree).filter(n=>n.type==='button');expect(buttons.length).toBeGreaterThan(0);
 vi.mocked(call).mockImplementation(channel=>channel==='launcher-tools-update-choose'?Promise.resolve({message:'Canceled'}):Promise.reject(Error('follow-up unavailable')));
 buttons.find(n=>n.props.children==='Install update folder').props.onClick();await flush();await flush();tree=render();expect(JSON.stringify(tree)).toContain('follow-up unavailable');expect(JSON.stringify(tree)).toContain('settings-badge');

 // Initial load failure retains a visible retry path rather than a vanished card.
 const empty=new View();empty.render(()=>LauncherUpdatesCard({setStatus:vi.fn()}));await flush();tree=empty.render(()=>LauncherUpdatesCard({setStatus:vi.fn()}));expect(JSON.stringify(tree)).toContain('follow-up unavailable');expect(nodes(tree).some(n=>n.type==='button'&&n.props.children==='Retry')).toBe(true);view.dispose();empty.dispose();
});
