// Staged outside frontend until QA0.35 checkpoint. Integrate by rendering this
// component ONLY for ?startupIntro=1 before App/broad launcher clients mount.
import { useEffect, useRef, useState } from "react";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./StartupIntro.css";

export interface IntroBootstrap { generation:number; source:string; title:string; volume:number; muted:boolean; audioStatus:string }
export interface IntroControl { generation:number; sequence:number; op:"volume"|"play"|"silence"; volume:{volume:number;muted:boolean}|null }
export type IntroRequest = {op:"bootstrap"}|{op:"prepared"|"pointerReleased";generation:number}|{op:"keyReleased";generation:number;key:string}|{op:"finish";generation:number;reason:"ended"|"error"|"autoplay-error"};
export interface IntroBridge { request<T>(request:IntroRequest):Promise<T>; subscribe(listener:(value:unknown)=>void):Promise<()=>void>; source(path:string):string }
const bridge:IntroBridge = { request:request=>invoke("startup_intro_event",{request}), subscribe:async listener=>listen("startup-intro-control",event=>listener(event.payload)), source:convertFileSrc };
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
export function supportedSkipKey(key:string):boolean{return ["Escape","Enter"," "].includes(key);}

export function StartupIntro({client=bridge}:{client?:IntroBridge}) {
  const video=useRef<HTMLVideoElement>(null);
  const generation=useRef(0);
  const prepared=useRef(false);
  const closing=useRef(false);
  const [source,setSource]=useState("");
  const [title,setTitle]=useState("Startup video");
  const [error,setError]=useState("");
  const [audio,setAudio]=useState("Waiting for NBA 2K27 Windows mixer");
  const fail = (reason:"error"|"autoplay-error", message:string) => {
    if(closing.current)return;
    closing.current=true;video.current?.pause();setError(message);
    if(generation.current)void client.request({op:"finish",generation:generation.current,reason}).catch(()=>{});
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
        else if(command.op==="play"&&!closing.current){void v.play().catch(()=>fail("autoplay-error","Unable to start this MP4. Returning to the game…"));}
      });
      if(cancelled){remove();return;}
      const boot=validateBootstrap(await client.request<unknown>({op:"bootstrap"}));
      if(cancelled)return;
      generation.current=boot.generation;
      const v=video.current;if(v){v.volume=boot.volume;v.muted=boot.muted;}
      video.current?.parentElement?.focus({preventScroll:true});
      setTitle(boot.title);setAudio(boot.audioStatus);setSource(client.source(boot.source));
    };
    void controller().catch(()=>{if(!cancelled)fail("error","Unable to prepare this MP4. Returning to the game…");});
    return()=>{cancelled=true;remove?.();video.current?.pause();};
  },[client]);
  const ready=()=>{
    if(!generation.current||prepared.current||closing.current)return;
    prepared.current=true;
    void client.request({op:"prepared",generation:generation.current}).catch(()=>fail("error","The owned video session closed."));
  };
  return <main className="startup-intro" aria-label={title}
    onPointerUp={event=>{event.preventDefault();event.stopPropagation();if(generation.current&&!closing.current)void client.request({op:"pointerReleased",generation:generation.current}).catch(()=>{});}}
    onKeyDown={event=>{if(supportedSkipKey(event.key)){event.preventDefault();event.stopPropagation();}}}
    onKeyUp={event=>{if(supportedSkipKey(event.key)){event.preventDefault();event.stopPropagation();if(generation.current&&!closing.current)void client.request({op:"keyReleased",generation:generation.current,key:event.key}).catch(()=>{});}}}
    tabIndex={0}>
    <video ref={video} src={source||undefined} playsInline preload="auto" onCanPlay={ready}
      onEnded={()=>{if(!closing.current&&generation.current){closing.current=true;void client.request({op:"finish",generation:generation.current,reason:"ended"}).catch(()=>{});}}}
      onError={()=>fail("error","Unable to decode this MP4. Returning to the game…")}/>
    <div className="startup-intro-hint"><span>2K LAUNCHER</span><strong>Click · Enter · Esc · Controller A/B to skip</strong><small>NBA 2K27 loads independently during Play</small></div>
    <span className="startup-intro-audio" role="status">{audio}</span>
    {error&&<div className="startup-intro-error" role="alert">{error}</div>}
  </main>;
}
