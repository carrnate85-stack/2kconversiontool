'use strict';
// Experimental game-facing container, not a replacement for library/preview WAVs.
// Exact-build evidence: audio_header_parser.txt (magic/descriptor-size) and
// vorbis_descriptor.txt (game-generated PCM16 descriptor and WAVEFORMATEX).
const MAGIC = 0xBBCFA17E;
const PCM16 = 0x063543BD;

function wrapPcmWave(wave) {
  if(require('./native-opus').isNativeOpus(wave)){require('./native-opus').parse(wave);return wave;}
  if (!Buffer.isBuffer(wave) || wave.length < 44 || wave.length > 256*1024*1024
      || wave.toString('ascii',0,4)!=='RIFF' || wave.toString('ascii',8,12)!=='WAVE'
      || wave.readUInt32LE(4)!==wave.length-8) throw new Error('Invalid or oversized RIFF WAV.');
  let fmt, pcm;
  for (let cursor=12; cursor<wave.length;) {
    if (cursor+8>wave.length) throw new Error('Truncated WAV chunk.');
    const length=wave.readUInt32LE(cursor+4), end=cursor+8+length;
    if (end+(length&1)>wave.length) throw new Error('Truncated WAV payload.');
    const id=wave.toString('ascii',cursor,cursor+4), data=wave.subarray(cursor+8,end);
    if (id==='fmt ') {if(fmt) throw new Error('Duplicate WAV format.'); fmt=data;}
    if (id==='data') {if(pcm) throw new Error('Duplicate WAV data.'); pcm=data;}
    cursor=end+(length&1);
  }
  if (!fmt || fmt.length<16 || !pcm || !pcm.length) throw new Error('Missing WAV format or audio.');
  const channels=fmt.readUInt16LE(2), rate=fmt.readUInt32LE(4), align=channels*2;
  if (fmt.readUInt16LE(0)!==1 || channels!==2 || rate!==48000 || fmt.readUInt16LE(14)!==16
      || fmt.readUInt16LE(12)!==align || fmt.readUInt32LE(8)!==rate*align
      || pcm.length%align || pcm.length/(rate*align)>1200) throw new Error('Test requires complete stereo 48 kHz PCM16 under 20 minutes.');
  const header=Buffer.alloc(8+46);
  header.writeUInt32LE(MAGIC,0);
  header.writeUInt32LE(46,4);
  const d=header.subarray(8);
  d.writeUInt32LE(PCM16,0);
  d.writeUInt32LE(channels,4);
  d.writeUInt32LE(rate,8);
  // Duration consumer divides this scalar-sample count by channels * rate.
  d.writeUInt32LE(pcm.length/2,12);
  d.writeUInt32LE(18,16); // WAVEFORMATEX size
  d.writeUInt32LE(pcm.length,20);
  d.writeUInt32LE(align,24); // Stream buffer alignment; never zero.
  fmt.copy(d,28,0,16);
  d.writeUInt16LE(0,44); // PCM has no codec extra bytes.
  return Buffer.concat([header,pcm]);
}
module.exports={wrapPcmWave,MAGIC,PCM16};
