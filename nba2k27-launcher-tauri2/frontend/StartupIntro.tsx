// Staged outside frontend until QA0.35 checkpoint. Integrate by rendering this
// component ONLY for ?startupIntro=1 before App/broad launcher clients mount.
import { useEffect, useRef, useState } from "react";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./StartupIntro.css";
import { launchProgress, forwardProgress, type LaunchProgress } from "./launch-progress";

export interface IntroBootstrap { generation:number; source:string; title:string; volume:number; muted:boolean; audioStatus:string }
export interface IntroControl { generation:number; sequence:number; op:"volume"|"play"|"silence"; volume:{volume:number;muted:boolean}|null }
export type IntroRequest = {op:"bootstrap"}|{op:"launchStatus";generation:number}|{op:"prepared"|"pointerReleased";generation:number}|{op:"keyReleased";generation:number;key:string}|{op:"progress";generation:number;stage:"bootstrap"|"metadata"|"canplay"|"playing"|"frame"|"launch"|"error";detail:string}|{op:"finish";generation:number;reason:"ended"|"error"|"autoplay-error"};
export interface IntroBridge { request<T>(request:IntroRequest):Promise<T>; subscribe(listener:(value:unknown)=>void):Promise<()=>void>; source(path:string):string }
export function startupIntroRequest<T>(request:IntroRequest,injected:unknown=window.__startupIntroBootstrap):Promise<T>{
  // The native owner freezes this object into the dedicated window before its
  // first page script.  Bootstrap therefore cannot be lost to an early IPC/
  // capability race; later acknowledgements still use the narrow command.
  if(request.op==="bootstrap"&&injected!==undefined)return Promise.resolve(injected as T);
  return invoke<T>("startup_intro_event",{request});
}
const bridge:IntroBridge = { request:startupIntroRequest, subscribe:async listener=>listen("startup-intro-control",event=>listener(event.payload)), source:convertFileSrc };
const record = (v:unknown):v is Record<string,unknown>=>v!==null&&typeof v==="object"&&!Array.isArray(v);
const positive = (v:unknown):v is number=>typeof v==="number"&&Number.isSafeInteger(v)&&v>0;
export function validVolume(v:unknown):v is {volume:number;muted:boolean} { return record(v)&&typeof v.volume==="number"&&Number.isFinite(v.volume)&&v.volume>=0&&v.volume<=1&&typeof v.muted==="boolean"; }
export function validateBootstrap(v:unknown):IntroBootstrap {
  if(!record(v)||!positive(v.generation)||typeof v.source!=="string"||v.source.length>32768||v.source.includes("\0")||!(/^[a-z]:[\\/]/i.test(v.source)||v.source.startsWith("\\\\"))||!v.source.toLowerCase().endsWith(".mp4")||typeof v.title!=="string"||v.title.length>1024||typeof v.audioStatus!=="string"||!validVolume(v))throw Error("Invalid owned startup-video bootstrap");
  return v as unknown as IntroBootstrap;
}
export function validateControl(v:unknown,generation:number,previous:number):IntroControl|null {
  if(!record(v)||v.generation!==generation||!positive(v.sequence)||v.sequence<=previous||!["volume","play","silence"].includes(String(v.op))||(v.op==="volume"&&!validVolume(v.volume)))return null;
  return v as unknown as IntroControl;
}
export function introLaunchProgress(value:unknown):LaunchProgress|null {
  if(!record(value)||typeof value.phase!=="string"||typeof value.providerPhase!=="string"||typeof value.failed!=="boolean")return null;
  if(value.failed || ["closed","needs-review","held-until-game-exit","waiting-for-safe-cleanup"].includes(value.phase))
    return {...launchProgress("needs-review"),step:"Launch needs attention"};
  if(value.phase==="game-running"&&value.steamOnly===true)
    return {...launchProgress("loader-active"),step:"Game running"};
  const phase=value.providerPhase!=="unknown"?value.providerPhase:value.phase;
  const result=launchProgress(phase);
  if(phase==="loader-active")return {...result,step:"Session ready"};
  if(["unknown","game-running"].includes(phase))return {...result,step:"Waiting for session status"};
  return result;
}
export function IntroLaunchMeter({value,bar}:{value:LaunchProgress;bar:number}) {
  return <div className="startup-intro-loading">
    <span className="startup-intro-loading-label" role="status" aria-live="polite">{value.step}</span>
    <div className="startup-intro-loading-track" role="progressbar" aria-label="Background launch stages" aria-valuetext={value.step}>
      <span style={{width:`${bar}%`}} />
    </div>
  </div>;
}
export function supportedSkipKey(key:string):boolean{return ["Escape","Enter"," "].includes(key);}
// WPF video-overlay-page.js: one error text, shown 2.5 s before returning;
// a click or Escape/Enter/Space during that time returns at once.
export const INTRO_ERROR_MESSAGE="Unable to play this MP4. Click or press Escape to return.";
export const INTRO_ERROR_DELAY_MS=2500;

export function StartupIntro({client=bridge}:{client?:IntroBridge}) {
  const video=useRef<HTMLVideoElement>(null);
  const generation=useRef(0);
  const prepared=useRef(false);
  const closing=useRef(false);
  const failure=useRef<{reason:"error"|"autoplay-error";timer:ReturnType<typeof setTimeout>}|null>(null);
  const [source,setSource]=useState("");
  const [title,setTitle]=useState("Startup video");
  const [error,setError]=useState("");
  const lastLaunchStep=useRef("");
  const [launch,setLaunch]=useState(()=>launchProgress("queued"));
  const [launchBar,setLaunchBar]=useState(8);
  const [audio,setAudio]=useState("Waiting for NBA 2K27 Windows mixer");
  const progress=(stage:"bootstrap"|"metadata"|"canplay"|"playing"|"frame"|"launch"|"error",detail="")=>{
    if(generation.current)void client.request({op:"progress",generation:generation.current,stage,detail:detail.slice(0,512)}).catch(()=>{});
  };
  const finishFailure = () => {
    const pending=failure.current;if(!pending)return;
    clearTimeout(pending.timer);failure.current=null;
    if(generation.current)void client.request({op:"finish",generation:generation.current,reason:pending.reason}).catch(()=>{});
  };
  const fail = (reason:"error"|"autoplay-error",detail="") => {
    if(closing.current)return;
    progress("error",`${reason}: ${detail || video.current?.error?.message || "unknown playback error"}`);
    closing.current=true;video.current?.pause();setError(INTRO_ERROR_MESSAGE);
    failure.current={reason,timer:setTimeout(finishFailure,INTRO_ERROR_DELAY_MS)};
  };
  useEffect(()=>{
    let cancelled=false;
    let remove:(()=>void)|undefined;
    let sequence=0;
    const controller=async()=>{
      remove=await client.subscribe(value=>{
        if(cancelled)return;
        const command=validateControl(value,generation.current,sequence);if(!command)return;sequence=command.sequence;
        const v=video.current;if(!v)return;
        if(command.op==="volume"&&command.volume){v.volume=command.volume.volume;v.muted=command.volume.muted;}
        else if(command.op==="silence"){closing.current=true;v.muted=true;v.pause();}
        else if(command.op==="play"&&!closing.current){void v.play().catch(error=>fail("autoplay-error",String(error)));}
      });
      if(cancelled){remove();return;}
      const boot=validateBootstrap(await client.request<unknown>({op:"bootstrap"}));
      if(cancelled)return;
      generation.current=boot.generation;
      progress("bootstrap");
      const v=video.current;if(v){v.volume=boot.volume;v.muted=boot.muted;}
      video.current?.parentElement?.focus({preventScroll:true});
      setTitle(boot.title);setAudio(boot.audioStatus);setSource(client.source(boot.source));
    };
    void controller().catch(error=>{if(!cancelled)fail("error",String(error));});
    return()=>{cancelled=true;remove?.();video.current?.pause();if(failure.current)clearTimeout(failure.current.timer);};
  },[client]);
  useEffect(()=>{
    if(!source||!generation.current)return;
    let cancelled=false;let timer:ReturnType<typeof setTimeout>|undefined;
    const token=generation.current;
    const poll=async()=>{
      try {
        const value=introLaunchProgress(await client.request({op:"launchStatus",generation:token}));
        if(!cancelled&&!closing.current&&generation.current===token&&value){
          if(lastLaunchStep.current!==value.step){lastLaunchStep.current=value.step;progress("launch",value.step);}
          setLaunch(value);setLaunchBar(previous=>value.state==="failed"?previous:forwardProgress(previous,value));
        }
      } catch { /* A closing/replaced intro cannot retain a background poll. */ }
      if(!cancelled&&!closing.current)timer=setTimeout(poll,500);
    };
    void poll();return()=>{cancelled=true;if(timer)clearTimeout(timer);};
  },[client,source]);
  const ready=()=>{
    if(!generation.current||prepared.current||closing.current)return;
    prepared.current=true;
    progress("canplay");
    void client.request({op:"prepared",generation:generation.current}).catch(()=>fail("error"));
  };
  return <main className="startup-intro" aria-label={title}
    onPointerUp={event=>{event.preventDefault();event.stopPropagation();if(failure.current){finishFailure();return;}if(generation.current&&!closing.current)void client.request({op:"pointerReleased",generation:generation.current}).catch(()=>{});}}
    onKeyDown={event=>{if(supportedSkipKey(event.key)){event.preventDefault();event.stopPropagation();}}}
    onKeyUp={event=>{if(supportedSkipKey(event.key)){event.preventDefault();event.stopPropagation();if(failure.current){finishFailure();return;}if(generation.current&&!closing.current)void client.request({op:"keyReleased",generation:generation.current,key:event.key}).catch(()=>{});}}}
    tabIndex={0}>
    <video ref={video} src={source||undefined} playsInline preload="auto" onCanPlay={ready}
      onLoadedMetadata={()=>progress("metadata",`${video.current?.videoWidth}x${video.current?.videoHeight}`)}
      onPlaying={()=>{progress("playing");video.current?.requestVideoFrameCallback?.((_now,meta)=>{
        const v=video.current;if(!v)return;
        const rect=v.getBoundingClientRect();
        const meter=v.parentElement?.querySelector(".startup-intro-loading");const m=meter?.getBoundingClientRect();
        const fill=meter?.querySelector(".startup-intro-loading-track>span");
        progress("frame",JSON.stringify({decoded:[meta.width,meta.height],time:meta.mediaTime,frames:meta.presentedFrames,viewport:[innerWidth,innerHeight],rect:[rect.x,rect.y,rect.width,rect.height],dpr:devicePixelRatio,fit:getComputedStyle(v).objectFit,meter:m?[m.x,m.y,m.width,m.height]:null,green:fill?getComputedStyle(fill).backgroundImage:null}));
      });}}
      onEnded={()=>{if(!closing.current&&generation.current){closing.current=true;void client.request({op:"finish",generation:generation.current,reason:"ended"}).catch(()=>{});}}}
      onError={()=>fail("error")}/>
    <div className="startup-intro-hint"><span>2K LAUNCHER</span><strong>Click · Enter · Esc · Controller A/B to skip</strong><small>NBA 2K27 loads independently during Play</small></div>
    <IntroLaunchMeter value={launch} bar={launchBar}/>
    <span className="startup-intro-audio" role="status">{audio}</span>
    {error&&<div className="startup-intro-error" role="alert">{error}</div>}
  </main>;
}
