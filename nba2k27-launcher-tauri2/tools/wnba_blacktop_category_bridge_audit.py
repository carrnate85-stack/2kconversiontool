"""Offline bounded PE/ABI audit for a proposed V2 Blacktop category bridge.

No process access, native calls, code publication, keys, or game writes. Emits
create-only metadata evidence in project artifacts. File mmap is read-only.
"""
import argparse
import bisect
import hashlib
import json
import mmap
import pathlib
import struct

ROOT = pathlib.Path(__file__).resolve().parent.parent
SWITCH = 0x230BDD0
TABLE = 0x7425F4C
CONTINUATION = 0x230BE2B
LABEL = 0xACFC4D0


class Image:
    def __init__(self, path):
        self.file = path.open('rb')
        self.data = mmap.mmap(self.file.fileno(), 0, access=mmap.ACCESS_READ)
        pe = struct.unpack_from('<I', self.data, 0x3c)[0]
        if self.data[pe:pe+4] != b'PE\0\0':
            raise ValueError('Not PE')
        self.stamp = struct.unpack_from('<I', self.data, pe+8)[0]
        optional = pe+24
        self.base = struct.unpack_from('<Q', self.data, optional+24)[0]
        self.image_size = struct.unpack_from('<I', self.data, optional+56)[0]
        if (self.stamp, self.image_size) != (0x6AC53765, 0x35BDE000):
            raise ValueError('Not audited V2 tuple')
        self.sections = []
        count = struct.unpack_from('<H', self.data, pe+6)[0]
        first = optional + struct.unpack_from('<H', self.data, pe+20)[0]
        for i in range(count):
            at = first+i*40
            name = self.data[at:at+8].rstrip(b'\0').decode('ascii')
            vsize, rva, size, offset = struct.unpack_from('<IIII', self.data, at+8)
            flags = struct.unpack_from('<I', self.data, at+36)[0]
            self.sections.append(dict(name=name,rva=rva,vsize=vsize,size=size,offset=offset,flags=flags))
        exception_rva, exception_size = struct.unpack_from('<II', self.data, optional+112+3*8)
        at = self.offset(exception_rva)
        self.functions = [struct.unpack_from('<III',self.data,at+i) for i in range(0,exception_size,12)]
        self.starts = [item[0] for item in self.functions]

    def section(self, rva):
        return next(s for s in self.sections if s['rva'] <= rva < s['rva']+max(s['vsize'],s['size']))

    def offset(self,rva):
        s=self.section(rva)
        if rva-s['rva'] >= s['size']:
            raise ValueError('Not file backed')
        return s['offset']+rva-s['rva']

    def read(self,rva,n):
        at=self.offset(rva)
        s=self.section(rva)
        if rva+n>s['rva']+s['size']:
            raise ValueError('Cross-section read')
        return self.data[at:at+n]

    def function(self,rva):
        i=bisect.bisect_right(self.starts,rva)-1
        if i<0 or not self.functions[i][0]<=rva<self.functions[i][1]:
            raise ValueError(f'No exception span for {rva:X}')
        start,end,unwind=self.functions[i]
        raw=self.read(start,end-start)
        s=self.section(start)
        return dict(start=f'{start:X}',end=f'{end:X}',bytes=end-start,unwind=f'{unwind:X}',
                    sha256=hashlib.sha256(raw).hexdigest().upper(),section=s['name'],
                    sectionFlags=f'{s["flags"]:08X}',executable=bool(s['flags']&0x20000000),
                    writable=bool(s['flags']&0x80000000),readable=bool(s['flags']&0x40000000))

    def direct_refs(self,targets):
        result=[]
        for s in self.sections:
            if not s['flags']&0x20000000:
                continue
            end=s['offset']+s['size']
            for op,kind in [(b'\xe8','call'),(b'\xe9','tail-jump')]:
                at=s['offset']
                while (at:=self.data.find(op,at,end))>=0:
                    if at+5>end:
                        break
                    rva=s['rva']+at-s['offset']
                    target=rva+5+struct.unpack_from('<i',self.data,at+1)[0]
                    if target in targets:
                        try:
                            owner=self.function(rva)
                            result.append(dict(rva=f'{rva:X}',kind=kind,target=f'{target:X}',owner=owner,
                                               nearbyBytes=self.read(max(int(owner['start'],16),rva-16),32).hex()))
                        except (ValueError,StopIteration):
                            pass
                    at+=1
        return result

    def pointer_refs(self, target):
        result=[]
        encoded=struct.pack('<Q',self.base+target)
        for section in self.sections:
            at=section['offset']
            end=at+section['size']
            while (at:=self.data.find(encoded,at,end))>=0:
                rva=section['rva']+at-section['offset']
                result.append(dict(rva=f'{rva:X}',section=section['name'],bytes=self.read(max(section['rva'],rva-16),40).hex()))
                at+=1
        return result

    def rip_lea_refs(self,target):
        result=[]
        for section in self.sections:
            if not section['flags']&0x20000000:
                continue
            at=section['offset']
            end=at+section['size']
            while (at:=self.data.find(b'\x8d',at,end))>=0:
                if at>section['offset'] and at+6<end and self.data[at-1] in (0x48,0x4c) and self.data[at+1]&0xc7==5:
                    rva=section['rva']+at-1-section['offset']
                    dest=rva+7+struct.unpack_from('<i',self.data,at+2)[0]
                    if dest==target:
                        result.append(dict(rva=f'{rva:X}',owner=self.function(rva)))
                at+=1
        return result


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--exe',type=pathlib.Path,default=pathlib.Path(r'C:\Program Files (x86)\Steam\steamapps\common\NBA 2K27\NBA2K27.exe'))
    p.add_argument('--out',type=pathlib.Path,required=True)
    a=p.parse_args()
    out=a.out.resolve()
    if out.exists() or not out.is_relative_to((ROOT/'artifacts').resolve()):
        p.error('Output must be new and below project artifacts')
    image=Image(a.exe)
    raw=image.read(SWITCH,0xfb)
    if raw[:7]!=bytes.fromhex('5657534883ec20') or image.read(CONTINUATION,8)!=bytes.fromhex('4889f1e87dbf4d00'):
        raise ValueError('Bridge prologue/continuation changed')
    table=image.read(TABLE,24)
    targets=[TABLE+struct.unpack_from('<i',table,i*4)[0] for i in range(6)]
    if targets!=[0x230BE18,0x230BE26,0x230BE1F,0x230BEA4,0x230BE7A,0x230BE83]:
        raise ValueError('Dispatch targets changed')
    methods=[image.function(rva) for rva in [SWITCH,0x27E7DB0,0x27E7DF0,0x27E7900,0x27E7B10,0x230C310,0x3119780]]
    refs=image.direct_refs({SWITCH,0x27E7DB0,0x27E7DF0,0x230C310})
    table_section=image.section(TABLE)
    label_section=image.section(LABEL)
    bridge_rva=0x40000000
    to_cont=CONTINUATION-(bridge_rva+10)
    table_rel=bridge_rva-TABLE
    bridge=b'\xbe\0\0\0\x20\xe9'+struct.pack('<i',to_cont)
    report=dict(schema='nba2k27-blacktop-category-bridge-abi-audit-v1',processAccess=False,gameWrites=0,
                nativeCalls=0,codePublished=False,exe=str(a.exe),peTimestamp=f'{image.stamp:X}',imageSize=f'{image.image_size:X}',
                methods=methods,directRefs=refs,switchPointerRefs=image.pointer_refs(SWITCH),switchLeaRefs=image.rip_lea_refs(SWITCH),
                dispatch=dict(rva=f'{TABLE:X}',bytes=table.hex(),targets=[f'{v:X}' for v in targets],
                              slot=f'{TABLE+12:X}',originalRel32=table[12:16].hex(),page=f'{TABLE&~4095:X}',
                              pageOffset=TABLE&4095,section=table_section,
                              untouchedBytes=table[:12].hex()+':'+table[16:].hex()),
                label=dict(rva=f'{LABEL:X}',page=f'{LABEL&~4095:X}',pageOffset=LABEL&4095,section=label_section,
                           diskBytes=None,diskFileBacked=False,initializerRva='3119780',
                           expectedId='BA3C982D',expectedReserved=0,originalKey='6FC058B0442652CD',womenKey='0F39A44C3426C962'),
                bridge=dict(exampleRva=f'{bridge_rva:X}',bytes=bridge.hex(),size=10,
                            continuation=f'{CONTINUATION:X}',tableRel32=table_rel,jumpRel32=to_cont,
                            allowedRvaMin=max(TABLE-(1<<31),CONTINUATION-10-((1<<31)-1),0),
                            allowedRvaMax=min(TABLE+((1<<31)-1),CONTINUATION-10+(1<<31))),
                abi=dict(entry='Stock jump table indirect-jumps after push RSI/RDI/RBX and sub RSP,20h; no call return address added.',
                         rsi='ESI holds category selector3. No stock case3 enumeration occurs. Bridge mov ESI,20000000 changes only RSI as stock cases0/1/2 do.',
                         stack='Three nonvolatile pushes and32byte stack reservation stay intact. No bridge push/pop/call or stack adjustment. Continuation call uses existing Win64 shadow space.',
                         continuation='230BE2B mov RCX,RSI; call27E7DB0 counts mask, then27E7DF0 obtains each row pointer. Common epilogue restores RSI/RDI/RBX and tailjumps230C310.',
                         flags='MOV/JMP preserve flags; first consumer count routine establishes its own comparisons. No branch uses old switch flags before the count call.'),
                limitations=['Direct references are byte-scan candidates; confirm instruction boundaries through emitted disassembly before admitting callers.',
                             'PE sections establish disk permissions only; live VirtualQuery allocation/type/protection must independently match.',
                             'ABI evidence does not establish concurrent publication/restore, user-input races, or match compatibility.',
                             'Source two-entry pack and its allocations must remain retained until process exit. Never unregister priority to toggle categories.',
                             'A bridge allocation reachable by either table dispatch or stale game thread must never be freed before process exit.'])
    with out.open('x',encoding='utf8') as stream:
        json.dump(report,stream,indent=2)
        stream.write('\n')
    print(json.dumps(dict(report=str(out),fullMethodProofs=len(methods),directRefCandidates=len(refs),bridgeBytes=bridge.hex())))


if __name__=='__main__':
    main()
