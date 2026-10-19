"""One fixed startup-manager metadata read from the exact QA066 three-process lease.
No game functions, event APIs, threads, keys, process writes or game-file changes.
"""
import ctypes as c
from ctypes import wintypes as w
import hashlib,json,os,pathlib,struct,subprocess
from qa055_carry_readonly import ROOT,k
from wnba_priority_loaded_roster_probe import EXE_SHA,parse_lease
from wnba_blacktop_category_readonly_probe import Region,ordinary
SESSION='000000000000000018dfb3c81f8ce1080000000000000002'
PID=14320
MANAGER=0xBD7CD88

def main():
 lease=ROOT/'Dev/Portable/Data'/f'blacktop-outfit-v2-{SESSION}.lease'
 ordinary(lease);body=lease.read_bytes();pid,birth,image=parse_lease(lease,body)
 if pid!=PID:raise ValueError('different authorized game PID')
 ordinary(pathlib.Path(image));sha=hashlib.sha256()
 with open(image,'rb') as f:
  for block in iter(lambda:f.read(1024*1024),b''):sha.update(block)
 if sha.hexdigest()!=EXE_SHA:raise ValueError('exact executable SHA mismatch')
 handles=[];scopes=[];reads=[]
 try:
  for p,b,s in [(8,24,96),(12,32,608),(16,40,1120)]:
   proc=struct.unpack_from('<I',body,p)[0];born=struct.unpack_from('<Q',body,b)[0]
   path=body[s:s+512].decode('utf-16le').split('\0',1)[0]
   # Read-only + query/synchronize only. QUERY_INFORMATION is needed for VirtualQueryEx.
   h=k.OpenProcess(0x100410 if p==8 else 0x101000,False,proc)
   if not h:raise c.WinError(c.get_last_error())
   handles.append(h);scopes.append((h,proc,born,path))
  def check():
   if lease.read_bytes()!=body:raise ValueError('lease changed')
   for h,proc,born,path in scopes:
    times=[w.FILETIME() for _ in range(4)];seen=c.create_unicode_buffer(32768);size=w.DWORD(32768)
    if (not k.GetProcessTimes(h,*(c.byref(t) for t in times)) or times[0].dwLowDateTime|(times[0].dwHighDateTime<<32)!=born or k.WaitForSingleObject(h,0)!=258 or not k.QueryFullProcessImageNameW(h,0,seen,c.byref(size)) or seen.value.casefold()!=path.casefold()):raise ValueError('exact retained process identity changed')
  check()
  base=int(subprocess.check_output(['powershell','-NoProfile','-Command',f'(Get-Process -Id {pid}).MainModule.BaseAddress.ToInt64()'],text=True).strip())
  check()
  def read(at,n,label):
   if n not in [4,8] or at<0x10000 or at+n>=0x800000000000:raise ValueError('fixed metadata read bound')
   check();buf=c.create_string_buffer(n);got=c.c_size_t()
   if not k.ReadProcessMemory(handles[0],at,buf,n,c.byref(got)) or got.value!=n:raise c.WinError(c.get_last_error())
   check();reads.append({'label':label,'address':hex(at),'bytes':n});return int.from_bytes(buf.raw,'little')
  def region(at):
   check();r=Region()
   if k.VirtualQueryEx(handles[0],at,c.byref(r),c.sizeof(r))!=c.sizeof(r):raise c.WinError(c.get_last_error())
   check();return {'base':hex(r.base),'allocation':hex(r.allocation),'allocationProtect':hex(r.allocationProtect),'size':r.size,'state':hex(r.state),'protect':hex(r.protect),'type':hex(r.type)}
  global_region=region(base+MANAGER)
  samples=[]
  for i in range(2):
   pointer=read(base+MANAGER,8,'fixed-manager-global')
   if not 0x10000<=pointer<0x800000000000-0x1F8:raise ValueError('invalid manager pointer')
   r=region(pointer);vtable=read(pointer,8,'manager-vtable');default=read(pointer+0x1E0,4,'manager-default-device')
   end_region=region(pointer+0x1F8-1)
   again=read(base+MANAGER,8,'fixed-manager-global-recheck')
   samples.append({'manager':hex(pointer),'managerRva':hex(pointer-base) if base<=pointer<base+0x35BDE000 else None,'managerRegion':r,'managerEndRegion':end_region,'vtable':hex(vtable),'vtableRva':hex(vtable-base),'defaultId':hex(default),'pointerStableWithinSample':pointer==again,'complete1F8WithinRegion':int(r['base'],16)<=pointer and pointer+0x1F8<=int(r['base'],16)+r['size']})
  check()
  report={'schema':'qa066-startup-manager-readonly-v1','session':SESSION,'pid':PID,'imageSha256':EXE_SHA,'base':hex(base),'retainedProcesses':[{'pid':proc,'birth':str(born),'path':path,'identityStable':True} for _,proc,born,path in scopes],'access':'game QUERY_INFORMATION|VM_READ|SYNCHRONIZE; other two QUERY_LIMITED_INFORMATION|SYNCHRONIZE','globalRegion':global_region,'samples':samples,'readLog':reads,'totalMemoryReadBytes':sum(r['bytes'] for r in reads),'gameWrites':0,'gameCalls':0,'eventsOpenedOrSignaled':0}
  out=ROOT/'artifacts/qa066-startup-manager-readonly-14320-20261018.json'
  with out.open('x',encoding='utf-8',newline='\n') as f:json.dump(report,f,indent=2);f.write('\n');f.flush();os.fsync(f.fileno())
  print(json.dumps(report,indent=2))
 finally:
  for h in handles:k.CloseHandle(h)
if __name__=='__main__':main()
