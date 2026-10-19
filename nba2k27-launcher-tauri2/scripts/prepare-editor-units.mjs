// Reuse existing pure web-editor units/colors, not its Node game backend.
import fs from "node:fs";import path from "node:path";import {fileURLToPath} from "node:url";import {createRequire} from "node:module";
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),"..");
const source=path.join(root,"../nba2k27-mod-launcher/src/renderer");
const lines=fs.readFileSync(path.join(source,"player-measurements.js"),"utf8").split(/\r?\n/);
if(!lines.shift()?.startsWith("/* Display units only.")||!lines.shift()?.endsWith("()=>{"))throw Error("Review WPF measurement wrapper.");
while(lines.at(-1)==="")lines.pop();if(lines.pop()!=="});")throw Error("Review measurement footer.");
const body=lines.join("\n");if(/\b(?:window|document|module|require)\b/.test(body))throw Error("Unexpected runtime dependency.");
const uniform=fs.readFileSync(path.join(source,"team-uniforms.js"),"utf8");
const colorNames=["colorHex","argbToInput","inputToArgb"];
const colors=colorNames.map(name=>{
  const original=uniform.match(new RegExp("  function "+name+"\\([^)]*\\) \\{[\\s\\S]*?\\n  \\}"))?.[0];
  if(!original)throw Error("Review WPF color helper: "+name);return "export "+original.trim();
});
const write=(file,text)=>{if(!fs.existsSync(file)||fs.readFileSync(file,"utf8")!==text)fs.writeFileSync(file,text);};
write(path.join(root,"frontend/editor-units.js"),"// Generated from WPF pure web-editor units/colors; no Node runtime.\nexport const playerMeasurements=(()=>{\n"+body+"\n})();\n"+colors.join("\n")+"\n");
const require=createRequire(import.meta.url),measure=require(path.join(source,"player-measurements.js")),team=require(path.join(source,"team-uniforms.js"));
const input=(value="")=>({value,dataset:{}}),attempt=fn=>{try{return fn();}catch{return null;}};
const measurementCases=[];
for(let i=0;i<=800;i++){
  const cm=120+i/4+0.000123,feet=input(),inches=input(),wing=input();
  measure.writeHeight(feet,inches,cm);measure.writeWingspan(wing,cm);
  measurementCases.push({cm,parts:measure.parts(cm),inches:measure.inches(cm),height:attempt(()=>measure.readHeight(feet,inches)),wingspan:attempt(()=>measure.readWingspan(wing))});
}
const editedCases=[];
for(const feet of ["","-1","4","5","6","7","8","9","6.5"])
 for(const inches of ["","-1","0","3.5","11.999","12","20"]){
  editedCases.push({feet,inches,expected:attempt(()=>measure.readHeight(input(feet),input(inches)))});
 }
const colorCases=[];
for(let alpha=0;alpha<=255;alpha++)for(const rgb of [0,1,0x123456,0xff0000,0x00ff00,0x0000ff,0xffffff]){
  const original=(alpha*0x1000000+rgb)>>>0;
  for(const choice of ["#000000","#abcdef","#ffffff"])
    colorCases.push({original,choice,hex:team.colorHex(original),input:team.argbToInput(original),changed:team.inputToArgb(choice,original)});
}
write(path.join(root,"assets/editor-units-regressions.json"),JSON.stringify({measurementCases,editedCases,colorCases}));
console.log(JSON.stringify({measurementCases:measurementCases.length+editedCases.length,colorCases:colorCases.length,source:"WPF exact bodies",runtime:"web UI"}));
