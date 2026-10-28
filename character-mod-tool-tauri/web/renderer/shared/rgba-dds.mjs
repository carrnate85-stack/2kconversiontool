// Preview-only top mip for legacy 32-bit RGBA/BGRA DDS files.
export function uncompressedDds(bytes) {
  if(bytes.byteLength<128)return null;
  const header=new DataView(bytes.buffer,bytes.byteOffset,128),u32=offset=>header.getUint32(offset,true);
  if(u32(0)!==0x20534444 || u32(4)!==124 || u32(76)!==32 || !(u32(80)&0x40) || u32(84)!==0 || u32(88)!==32 || (u32(112)&0x200))return null;
  const width=u32(16),height=u32(12),pitch=(u32(8)&8)?u32(20):width*4;
  if(!width||!height||width>16384||height>16384||width*height>16*1024*1024||pitch<width*4||128+pitch*height>bytes.byteLength)return null;
  const masks=[u32(92),u32(96),u32(100),u32(104)];
  const channels=masks.map(mask=>[0xff,0xff00,0xff0000,0xff000000].indexOf(mask));
  if(channels.slice(0,3).some(c=>c<0)||channels[3]<0&&masks[3]!==0)return null;
  const data=new Uint8Array(width*height*4);
  for(let y=0;y<height;y++)for(let x=0;x<width;x++)for(let c=0;c<4;c++)data[(y*width+x)*4+c]=channels[c]<0?255:bytes[128+y*pitch+x*4+channels[c]];
  return {data,width,height};
}
