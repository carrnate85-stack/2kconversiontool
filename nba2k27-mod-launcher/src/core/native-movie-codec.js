'use strict';
// Port of the game-tested all-intra encoder in scripts/build-native-movie-calibration.py.
// Runs in the isolated conversion renderer, never on the launcher's UI thread.
(function(root){
const L=[16,11,10,16,24,40,51,61,12,12,14,19,26,58,60,55,14,13,16,24,40,57,69,56,14,17,22,29,51,87,80,62,18,22,37,56,68,87,103,77,24,35,55,64,81,104,113,92,49,64,78,87,103,121,120,101,72,92,95,98,112,100,103,99];
const C=[17,18,24,47,99,99,99,99,18,21,26,66,99,99,99,99,24,26,56,99,99,99,99,99,47,66,99,99,99,99,99,99,...Array(32).fill(99)];
const Z=[0,1,8,16,9,2,3,10,17,24,32,25,18,11,4,5,12,19,26,33,40,48,41,34,27,20,13,6,7,14,21,28,35,42,49,56,57,50,43,36,29,22,15,23,30,37,44,51,58,59,52,45,38,31,39,46,53,60,61,54,47,55,62,63];
const basis=Array.from({length:8},(_,x)=>Array.from({length:8},(_,k)=>Math.cos((2*x+1)*k*Math.PI/16)/2/(k===0?Math.SQRT2:1)));
class Bits{constructor(){this.data=[];this.byte=0;this.n=0;}write(v,n){for(let i=n-1;i>=0;i--){this.byte=this.byte*2+((v>>>i)&1);if(++this.n===8){this.data.push(this.byte);this.byte=0;this.n=0;}}}bytes(){return Uint8Array.from(this.n?[...this.data,this.byte<<(8-this.n)]:this.data);}}
function join(parts){const b=new Uint8Array(parts.reduce((n,p)=>n+p.length,0));let o=0;for(const p of parts){b.set(p,o);o+=p.length;}return b;}
function component(values){
 const tokens=[],extra=new Bits(),ranges=[[1,0,9],[2,0,10],[3,0,11],[4,0,13],[5,0,14],[6,0,15],[7,1,16],[9,2,17],[13,3,18],[21,4,19],[37,5,20],[69,9,21]],runs=[[16,4,0],[12,2,1],[8,2,2],[6,1,3],[4,1,4],[3,0,5],[2,0,6],[1,0,7]];
 for(let p=0;p<values.length;){const v=values[p];if(v){const r=ranges.find(([b,w])=>Math.abs(v)>=b&&Math.abs(v)<b+2**w);if(!r)throw Error('Video coefficient exceeds native bounds.');tokens.push(r[2]);extra.write(v<0?1:0,1);extra.write(Math.abs(v)-r[0],r[1]);p++;}else{const end=p+64-p%64;let q=p;while(q<end&&!values[q])q++;if(q===end){tokens.push(8);p=end;}else{const r=runs.find(([b])=>q-p>=b),n=Math.min(q-p,r[0]+2**r[1]-1);tokens.push(r[2]);extra.write(n-r[0],r[1]);p+=n;}}}
 const counts=Array.from({length:22},()=>Array(22).fill(0));let prev=8;for(const t of tokens){counts[prev][t]++;prev=t;}
 const table=new Bits(),codes=[];table.write(21,5);
 for(const row of counts){const present=row.map((v,i)=>v?i:-1).filter(i=>i>=0),map={};codes.push(map);table.write(present.length?1:0,1);if(!present.length)continue;const first=present[0],last=present.at(-1),max=Math.max(...row),nodes=[];table.write(first,5);table.write(last,5);
  for(let i=first;i<=last;i++){const w=row[i]?Math.max(1,Math.floor(row[i]*255/max)):0;if(w<2)table.write(w,2);else if(w<18){table.write(2,2);table.write(w-2,4);}else{table.write(3,2);table.write(w,8);}if(w)nodes.push({w,s:i,id:nodes.length});}
  let id=nodes.length;while(nodes.length>1){nodes.sort((a,b)=>a.w-b.w||a.id-b.id);const a=nodes.shift(),b=nodes.shift();nodes.push({w:a.w+b.w,a,b,id:id++});}
  function walk(n,v,len){if(n.s!==undefined)map[n.s]=[v,len];else{walk(n.a,v*2,len+1);walk(n.b,v*2+1,len+1);}}walk(nodes[0],0,0);
 }
 const coded=new Bits();coded.write(0,8);prev=8;for(const t of tokens){coded.write(...codes[prev][t]);prev=t;}
 const tb=table.bytes(),cb=coded.bytes(),h=new Uint8Array(9),view=new DataView(h.buffer);h[1]=tokens.length&255;h[2]=(tokens.length>>>8)&255;h[3]=tokens.length>>>16;view.setUint16(4,tb.length,true);h[6]=cb.length&255;h[7]=(cb.length>>>8)&255;h[8]=cb.length>>>16;
 return join([h,tb,cb,extra.bytes()]);
}
function roundEven(v){const a=Math.floor(v),f=v-a;return f===0.5?(a%2===0?a:a+1):Math.round(v);}
function transform(plane,w,h,table,quality){
 const out=new Int16Array(w*h),tmp=new Float64Array(64),quant=table.map((v,i)=>Math.max(i<2||i===8||i===9?4:1,Math.min(255,Math.floor((v*(200-2*quality)+50)/100))));let offset=0;
 for(let y=0;y<h;y+=8)for(let x=0;x<w;x+=8){
  for(let ky=0;ky<8;ky++)for(let px=0;px<8;px++){let sum=0;for(let py=0;py<8;py++)sum+=basis[py][ky]*(plane[(y+py)*w+x+px]-128);tmp[ky*8+px]=sum;}
  for(let z=0;z<64;z++){const i=Z[z],ky=i>>3,kx=i&7;let sum=0;for(let px=0;px<8;px++)sum+=tmp[ky*8+px]*basis[px][kx];out[offset++]=roundEven(sum/quant[i]);}
 }return out;
}
function encode(rgba,width=960,height=544,quality=75){
 if(rgba.length!==width*height*4||width%16||height%16)throw Error('Invalid video frame dimensions.');
 const y=new Float64Array(width*height),cb=new Float64Array(width*height/4),cr=new Float64Array(cb.length);
 for(let py=0;py<height;py++)for(let px=0;px<width;px++){const p=((height-1-py)*width+px)*4,r=rgba[p],g=rgba[p+1],b=rgba[p+2],u=(py>>1)*(width>>1)+(px>>1);y[py*width+px]=.299*r+.587*g+.114*b;cb[u]+=(-.168736*r-.331264*g+.5*b+128)/4;cr[u]+=(.5*r-.418688*g-.081312*b+128)/4;}
 const yy=transform(y,width,height,L,quality),u=transform(cb,width/2,height/2,C,quality),v=transform(cr,width/2,height/2,C,quality),uv=new Int16Array(u.length+v.length),row=width/2*8;
 for(let i=0;i<u.length;i+=row){uv.set(u.subarray(i,i+row),i*2);uv.set(v.subarray(i,i+row),i*2+row);}
 const a=component(yy),b=component(uv),h=new Uint8Array(32),d=new DataView(h.buffer);h[0]=quality;h[1]=1;d.setUint32(4,24+a.length+b.length,true);d.setUint32(12,a.length,true);d.setUint32(16,b.length,true);h[30]=quality;h[31]=1;return join([h,a,b]);
}
const api={encode,component};if(typeof module!=='undefined')module.exports=api;else root.nativeMovieCodec=api;
})(globalThis);
