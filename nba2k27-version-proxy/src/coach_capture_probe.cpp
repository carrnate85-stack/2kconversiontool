// Read-only external preflight using exactly the diagnostic capture function.
#include <Windows.h>
static HANDLE coach_read_handle=nullptr;
#define GetCurrentProcess() coach_read_handle
#include "coach_clone_audit.hpp"
#undef GetCurrentProcess
#include <cstdlib>
int main(int argc,char **argv){
 if(argc!=4)return 2;
 const auto pid=static_cast<DWORD>(std::strtoul(argv[1],nullptr,10));
 const auto created=std::strtoull(argv[2],nullptr,10);
 const auto base=static_cast<std::uintptr_t>(std::strtoull(argv[3],nullptr,10));
 coach_read_handle=OpenProcess(PROCESS_QUERY_INFORMATION|PROCESS_VM_READ,FALSE,pid);
 if(!coach_read_handle)return 3;
 FILETIME c{},e{},k{},u{};ULARGE_INTEGER stamp{};
 bool ok=GetProcessTimes(coach_read_handle,&c,&e,&k,&u)!=FALSE;stamp.LowPart=c.dwLowDateTime;stamp.HighPart=c.dwHighDateTime;
 if(!ok||stamp.QuadPart!=created||e.dwLowDateTime||e.dwHighDateTime||!character_queue_noop::exact_build(base)){CloseHandle(coach_read_handle);return 4;}
 namespace mem=character_queue_noop;const auto sentinel=base+0xa37bfb0;auto node=mem::q(sentinel+0x10);unsigned found=0,passed=0;
 for(unsigned n=0;n<256&&node!=sentinel;++n){
  if(mem::q(node)!=base+0x681ccc8)break;
  const auto manager=node+0x78,actor=mem::q(manager+0x98);
  if(mem::q(actor)==base+0x6804e58&&coach_clone_audit::active(base,manager,actor)){
   coach_clone_audit::Snapshot s{};bool good=coach_clone_audit::capture(actor+0xf20,base,s);++found;if(good)++passed;
   std::printf("COACH=%u CAPTURE=%s COUNT=%u RESOURCE_NODES=%u REASON=%s\n",found,good?"PASS":"FAIL",s.count,s.resource_count,good?"Verified":s.failure);
  }
  node=mem::q(node+0x10);
 }
 CloseHandle(coach_read_handle);std::printf("READ_ONLY=YES NATIVE_CALLS=0 WRITES=0 VERIFIED=%u/%u\n",passed,found);return found&&found==passed?0:5;
}
