import { describe,it,expect } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { StartupIntro,validateBootstrap,validateControl,validVolume,supportedSkipKey,INTRO_ERROR_MESSAGE,INTRO_ERROR_DELAY_MS } from "./StartupIntro";
const boot={generation:1,source:"C:\\Portable\\Library\\Videos\\mp4\\intro.mp4",title:"NBA 2K12",volume:0.25,muted:false,audioStatus:"Following game mixer"};
describe("owned fullscreen startup intro",()=>{
  it("keeps the proven full-bleed styling and skip feedback without broad launcher RPC",()=>{const html=renderToStaticMarkup(<StartupIntro/>);expect(html).toContain("startup-intro-hint");expect(html).toContain("Controller A/B");expect(html).toContain("playsInline");expect(html).not.toContain("autoplay");expect(html).not.toContain("controls=");});
  it("requires exact bounded owned MP4 bootstrap and validated mixer scalar",()=>{expect(validateBootstrap(boot)).toEqual(boot);for(const source of ["https://evil.invalid/a.mp4","file:///C:/other.mp4","C:\\x.mov","C:\\x.mp4\0"]){expect(()=>validateBootstrap({...boot,source})).toThrow();}for(const volume of [NaN,Infinity,-1,2])expect(validVolume({...boot,volume})).toBe(false);});
  it("drops stale generation/sequence and unknown control commands",()=>{const valid={generation:1,sequence:3,op:"play",volume:null};expect(validateControl(valid,1,2)).toEqual(valid);expect(validateControl(valid,2,2)).toBeNull();expect(validateControl(valid,1,3)).toBeNull();expect(validateControl({...valid,op:"launch"},1,2)).toBeNull();expect(validateControl({...valid,op:"volume",volume:{volume:2,muted:false}},1,2)).toBeNull();});
  it("only consumes the original supported keyboard skip keys",()=>{for(const key of ["Escape","Enter"," "])expect(supportedSkipKey(key)).toBe(true);for(const key of ["F8","a","Tab","ArrowRight"])expect(supportedSkipKey(key)).toBe(false);});
  it("uses the WPF error text and 2.5 s return delay",()=>{expect(INTRO_ERROR_MESSAGE).toBe("Unable to play this MP4. Click or press Escape to return.");expect(INTRO_ERROR_DELAY_MS).toBe(2500);});
});
