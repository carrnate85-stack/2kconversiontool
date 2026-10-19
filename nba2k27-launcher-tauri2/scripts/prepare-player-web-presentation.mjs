// Build-time reuse of the WPF editor's pure presentation code. No Node runtime.
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createRequire } from "node:module";
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const source = path.join(root, "../nba2k27-mod-launcher/src/renderer");
const original = fs.readFileSync(path.join(source, "player-editor-layout.js"), "utf8");
const lines = original.split(/\r?\n/);
if (!lines.shift()?.startsWith("/* Presentation groups only.")) throw Error("Review the WPF presentation header.");
if (!lines.shift()?.endsWith("()=>{")) throw Error("Review the WPF presentation wrapper.");
while (lines.at(-1) === "") lines.pop();
if (lines.pop() !== "});") throw Error("Review the WPF presentation footer.");
const body = lines.join("\n");
if (/\b(?:require|window|module|document)\b/.test(body)) throw Error("Unexpected presentation runtime dependency.");
const ratings = fs.readFileSync(path.join(source, "player-ratings.js"), "utf8");
const display = ratings.match(/const display=(\(f,raw\)=>[^;]+);/)?.[1];
const change = ratings.match(/const raw=([^;]+);changes\.push/)?.[1];
if (!display || !change || !change.startsWith("field.money?Math.round(value*1000000)"))
  throw Error("Review WPF rating conversions before reusing them.");
const data = "// Generated from WPF pure web-editor presentation. No Node backend.\n" +
  "export const playerEditorLayout = (() => {\n" + body + "\n})();\n" +
  "export const playerDisplay = " + display + ";\n" +
  "export const playerRaw = (field,value) => " + change + ";\n";
const write = (file, text) => { if (!fs.existsSync(file) || fs.readFileSync(file, "utf8") !== text) fs.writeFileSync(file, text); };
write(path.join(root, "frontend/player-web-presentation.js"), data);
const require = createRequire(import.meta.url);
const layout = require(path.join(source, "player-editor-layout.js"));
const schema = JSON.parse(fs.readFileSync(path.join(source, "../data/player-field-schema.json"), "utf8"));
const maximum=require(path.join(source,"../core/player-field-edits.js")).maximum;
const tiers=ratings.match(/const tiers=(\[[^;]+\]);/)?.[1];
const choices=ratings.match(/const choices=(f=>[^;]+);/)?.[1];
if(!tiers||!choices)throw Error("Review WPF choice labels.");
const fieldChoices=Function("const tiers="+tiers+";return "+choices)();
const rawDisplay = Function("return " + display)();
const rawChange = Function("field", "value", "return " + change);
const fields = schema.map(f => ({ key:f.key,group:f.group,money:!!f.money,maximum:maximum(f),choices:fieldChoices(f)||[],category:layout.category(f),order:layout.order(f) }));
const conversions = [];
for (const group of ["attributes", "tendencies", "contract"])
  for (const money of [false,true]) {
    const f = {group,money};
    for (let raw=0;raw<=255;raw++) conversions.push({field:f,raw,display:rawDisplay(f,raw)});
    for (const value of [0,25,26,50,80,98,99,100,110,0.000001,1.234567,200])
      conversions.push({field:f,value,changed:rawChange(f,value)});
  }
write(path.join(root,"assets/player-web-regressions.json"), JSON.stringify({fields,conversions,categories:layout.categories,personalityPairs:layout.personalityPairs}));
console.log(JSON.stringify({playerWebPresentation:"WPF exact body",fields:fields.length,conversionCases:conversions.length,nodeRuntime:false}));
