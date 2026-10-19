// Isolated, silent decode acceptance fixture. Stage outside frontend until root
// wires its main-only smoke route/channel. Never starts the game or guardian.
import { useEffect, useRef, useState } from "react";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";

export interface DecodeRow { id:string;name:string;path:string;durationMs:number;codecs:string[];sha256:string }
export interface DecodePoint { target:number;mediaTime:number;presentedFrames:number;width:number;height:number }
export interface DecodeResult { id:string;name:string;passed:boolean;duration?:number;width?:number;height?:number;catalogCodecs:string[];points:DecodePoint[];error?:string }
export interface Scheduler { set(callback:()=>void,ms:number):ReturnType<typeof setTimeout>;clear(handle:ReturnType<typeof setTimeout>):void }
const timers:Scheduler={set:(fn,ms)=>setTimeout(fn,ms),clear:handle=>clearTimeout(handle)};
const object=(v:unknown):v is Record<string,unknown>=>typeof v==="object"&&v!==null&&!Array.isArray(v);
export function decodeRows(value:unknown):DecodeRow[]{
  if(!object(value)||!Array.isArray(value.rows)||value.rows.length<1||value.rows.length>16)throw Error("Invalid owned MP4 decode fixture rows");
  const ids=new Set<string>();
  return value.rows.map(v=>{
    if(!object(v)||typeof v.id!=="string"||v.id.length<1||v.id.length>128||ids.has(v.id)||typeof v.name!=="string"||v.name.length>1024||typeof v.path!=="string"||v.path.length>32768||v.path.includes("\0")||!(/^[a-z]:[\\/]/i.test(v.path)||v.path.startsWith("\\\\"))||!v.path.toLowerCase().endsWith(".mp4")||typeof v.durationMs!=="number"||!Number.isSafeInteger(v.durationMs)||v.durationMs<=0||v.durationMs>600000||!Array.isArray(v.codecs)||v.codecs.length<1||v.codecs.length>4||!v.codecs.every(c=>["H.264","AV1","VP9"].includes(c))||typeof v.sha256!=="string"||!/^[a-f0-9]{64}$/i.test(v.sha256))throw Error("Untrusted/duplicate MP4 fixture entry");
    ids.add(v.id);return v as unknown as DecodeRow;
  });
}
function abortError():Error{return new Error("MP4 decode fixture cancelled");}
export function videoEvent(video:EventTarget,event:string,signal:AbortSignal,timeoutMs:number,scheduler=timers):Promise<void>{
  return new Promise((resolve,reject)=>{
    if(signal.aborted){reject(abortError());return;}
    let finished=false;
    let timer:ReturnType<typeof setTimeout>;
    const clean=()=>{scheduler.clear(timer);video.removeEventListener(event,done);video.removeEventListener("error",error);signal.removeEventListener("abort",aborted);};
    const settle=(failure?:Error)=>{if(finished)return;finished=true;clean();failure?reject(failure):resolve();};
    const done=()=>settle();const error=()=>settle(Error(`Video error while waiting for ${event}`));const aborted=()=>settle(abortError());
    video.addEventListener(event,done,{once:true});video.addEventListener("error",error,{once:true});signal.addEventListener("abort",aborted,{once:true});timer=scheduler.set(()=>settle(Error(`Timed out waiting for video ${event}`)),timeoutMs);
  });
}
type FrameVideo=HTMLVideoElement&{requestVideoFrameCallback?:(callback:(now:number,meta:VideoFrameCallbackMetadata)=>void)=>number;cancelVideoFrameCallback?:(handle:number)=>void};
export function decodedFrame(video:FrameVideo,target:number,signal:AbortSignal,timeoutMs:number,scheduler=timers):Promise<DecodePoint>{
  return new Promise((resolve,reject)=>{
    if(signal.aborted){reject(abortError());return;}
    if(typeof video.requestVideoFrameCallback!=="function"||typeof video.cancelVideoFrameCallback!=="function"){reject(Error("Actual decoded-frame callback unavailable; metadata alone is not a decode pass"));return;}
    let finished=false;let frame:number|undefined;let timer:ReturnType<typeof setTimeout>;
    const cleanup=()=>{scheduler.clear(timer);if(frame!==undefined)video.cancelVideoFrameCallback?.(frame);video.removeEventListener("error",error);signal.removeEventListener("abort",aborted);};
    const fail=(reason:Error)=>{if(finished)return;finished=true;cleanup();reject(reason);};
    const error=()=>fail(Error("MP4 decoder rejected a requested frame"));const aborted=()=>fail(abortError());
    const receive=(_now:number,meta:VideoFrameCallbackMetadata)=>{
      frame=undefined;if(finished)return;
      const width=video.videoWidth,height=video.videoHeight;
      if(Number.isFinite(meta.mediaTime)&&Math.abs(meta.mediaTime-target)<=1&&meta.presentedFrames>0&&width>0&&height>0){finished=true;cleanup();resolve({target,mediaTime:meta.mediaTime,presentedFrames:meta.presentedFrames,width,height});}
      else{frame=video.requestVideoFrameCallback!(receive);}
    };
    video.addEventListener("error",error,{once:true});signal.addEventListener("abort",aborted,{once:true});timer=scheduler.set(()=>fail(Error(`No actual decoded frame near ${target.toFixed(2)}s`)),timeoutMs);frame=video.requestVideoFrameCallback(receive);
  });
}
async function boundedPlay(video:HTMLVideoElement,signal:AbortSignal,timeoutMs:number,scheduler=timers):Promise<void>{
  return new Promise((resolve,reject)=>{
    if(signal.aborted){reject(abortError());return;}
    let done=false;
    const finish=(error?:unknown)=>{if(done)return;done=true;scheduler.clear(timer);signal.removeEventListener("abort",abort);error?reject(error):resolve();};
    const abort=()=>finish(abortError());
    const timer=scheduler.set(()=>finish(Error("Muted playback did not start within its bound")),timeoutMs);signal.addEventListener("abort",abort,{once:true});
    try{Promise.resolve(video.play()).then(()=>finish(),error=>finish(error));}catch(error){finish(error);}
  });
}
export async function decodeVideo(row:DecodeRow,source:(path:string)=>string,host:HTMLElement,signal:AbortSignal):Promise<DecodeResult>{
  const result:DecodeResult={id:row.id,name:row.name,passed:false,catalogCodecs:row.codecs,points:[]};
  const video=document.createElement("video") as FrameVideo;
  video.muted=true;video.volume=0;video.autoplay=false;video.controls=false;video.playsInline=true;video.preload="auto";video.style.cssText="width:100%;max-height:45vh;object-fit:contain;background:#000";
  host.replaceChildren(video);
  const local=new AbortController();const abort=()=>local.abort();signal.addEventListener("abort",abort,{once:true});if(signal.aborted)local.abort();
  try{
    const loaded=videoEvent(video,"loadedmetadata",local.signal,12000);void loaded.catch(()=>{});video.src=source(row.path);video.load();await loaded;
    if(!Number.isFinite(video.duration)||video.duration<=0||video.duration>601||Math.abs(video.duration*1000-row.durationMs)>2500)throw Error("Decoded media duration differs from trusted fixture metadata");
    result.duration=video.duration;
    const positions=[0,video.duration/2,Math.max(0,video.duration-Math.min(0.5,video.duration/4))];
    for(const target of positions){
      if(local.signal.aborted)throw abortError();
      video.pause();
      // Register before seek/play so neither a synchronous seek event nor the
      // first decoded frame can be lost. Finally aborts every still-pending wait.
      const frame=decodedFrame(video,target,local.signal,12000);
      // Attach rejection immediately; later waits must not leave an unhandled
      // frame timeout/abort if seek/play itself fails first.
      void frame.catch(()=>{});
      if(Math.abs(video.currentTime-target)>0.01){const seeked=videoEvent(video,"seeked",local.signal,12000);void seeked.catch(()=>{});video.currentTime=target;await seeked;}
      await boundedPlay(video,local.signal,8000);result.points.push(await frame);video.pause();
    }
    result.width=video.videoWidth;result.height=video.videoHeight;result.passed=result.points.length===3;
  }catch(error){result.error=error instanceof Error?error.message:String(error);}
  finally{local.abort();signal.removeEventListener("abort",abort);video.pause();video.removeAttribute("src");video.load();video.remove();}
  return result;
}
export interface ProbeBridge{rows():Promise<unknown>;finish(result:unknown):Promise<void>;source(path:string):string}
const bridge:ProbeBridge={rows:()=>invoke("launcher",{channel:"startup-decode-fixture",args:null}),finish:result=>invoke("finish_smoke",{result}),source:convertFileSrc};
export function StartupDecodeProbe({client=bridge}:{client?:ProbeBridge}){
  const host=useRef<HTMLDivElement>(null);const [status,setStatus]=useState("Preparing silent MP4 decoder fixture…");
  useEffect(()=>{
    const abort=new AbortController();
    const run=async()=>{
      const results:DecodeResult[]=[];
      try{const rows=decodeRows(await client.rows());for(const row of rows){if(abort.signal.aborted)return;setStatus(`Decoding ${row.name}: start, midpoint, near end (muted)`);if(!host.current)throw Error("Fixture host disappeared");results.push(await decodeVideo(row,client.source,host.current,abort.signal));}}
      catch(error){if(abort.signal.aborted)return;results.push({id:"fixture",name:"Fixture validation",catalogCodecs:[],points:[],passed:false,error:String(error)});}
      if(abort.signal.aborted)return;
      const pass=results.length>0&&results.every(row=>row.passed);setStatus(pass?"All saved MP4s decoded at three positions.":"MP4 decode failure; see the fixture report.");
      await client.finish({schema:"nba2k27-startup-mp4-decode-v1",pass,fixtureOnly:true,gameAccess:false,guardianStarted:false,audioOutput:false,fullscreen:false,rows:results});
    };
    void run().catch(error=>{if(!abort.signal.aborted)setStatus(`Decode result could not be saved: ${String(error)}`);});
    return()=>abort.abort();
  },[client]);
  return <section style={{padding:18,background:"#07121a",color:"#e8eff3",minHeight:"100vh",fontFamily:"Segoe UI,sans-serif"}}><h2>Saved MP4 decode check</h2><p role="status">{status}</p><div ref={host}/><p>Silent local fixture only · no game launch, fullscreen or audio guardian.</p></section>;
}
