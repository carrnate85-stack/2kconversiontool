// SELF-OWNED resources only. No NativeHost opening, authority or game access.
#include "../native/wnba_blacktop_category_win.hpp"
#include <cassert>
#include <filesystem>
#include <iostream>
using namespace wnba_blacktop_category;
using namespace wnba_blacktop_category::win_detail;
using outfit_match_v2::Handle;
namespace fs=std::filesystem;
struct Source {bool category_source_ready(){return false;}};
int main(){
 for(auto elapsed:{9999u,10000u,10001u}) {PreparationClock clock;assert(clock.bind(100,100));assert(clock.valid(100+elapsed)==(elapsed<10000));}
 {PreparationClock clock;assert(!clock.valid(1));assert(!clock.bind(100,99));assert(clock.bind(100,100));assert(!clock.bind(101,101));assert(clock.bind(100,101));assert(!clock.valid(100));}
 {PreparationClock clock;assert(clock.bind(0,0));assert(clock.valid(9999));assert(!clock.bind(0,10000));}

 {NativeHost<Source> h;assert(h.preinitialize_proof(false)==PreinitializeProof::failed);
 assert(h.preallocation_proof()==PreinitializeProof::failed);assert(!h.passive_preparation_scope());
 assert(!h.initialize_bridge_rw(0,Code{}));assert(h.monotonic_ms()>0);}
 DeepDiagnostic base{};base.first_ok=true;base.second_attempted=true;
 base.captures[1].failure=CaptureFailure::ending_context_changed;
 base.captures[1].count=403;base.captures[1].nba=30;base.captures[1].clubs=15;
 base.scope_attempted=base.scope_ok=base.departure_first=base.departure_second=true;
 for(unsigned mask=0;mask<1024;++mask){auto d=base;d.captures[1].context_difference=mask;
 assert(typed_context_wait(d,true)==(mask&&!(mask&~224u)));assert(!typed_context_wait(d,false));}
 base.captures[1].context_difference=224;
 for(unsigned f=0;f<13;++f){auto d=base;
 if(f==0)d.scope_attempted=false;if(f==1)d.scope_ok=false;if(f==2)d.departure_first=false;
 if(f==3)d.departure_second=false;if(f==4)d.captures[1].count=402;if(f==5)d.captures[1].nba=29;
 if(f==6)d.captures[1].clubs=14;if(f==7)d.captures[1].failure=CaptureFailure::ending_table;
 if(f==8)d.first_ok=false;if(f==9)d.second_attempted=false;if(f==10)d.second_ok=true;
 if(f==11)d.difference=1;if(f==12)d.context_difference=64;
 assert(!typed_context_wait(d,true));}
 {auto d=base;d.first_ok=d.second_attempted=false;d.captures[0]=d.captures[1];d.captures[1]={};assert(typed_context_wait(d,true));}
 {
  std::vector<unsigned char> source(kRosterBytes);
  for(std::size_t n=0;n<kMixedTeams;++n){const std::uint32_t id=0xABCD0000u+unsigned(n),kind=unsigned(n)<<26;
   const auto off=n*kTeamStride;std::memcpy(source.data()+off+0xF20,&id,4);
   std::memcpy(source.data()+off+0x123C,&kind,4);
   std::array<wchar_t,24> name{};name[0]=wchar_t(L'A'+n%26);name[23]=wchar_t(n);
   std::memcpy(source.data()+off+0x2FA,name.data(),sizeof name);}
  std::vector<unsigned char> captured;unsigned calls{};
  assert(bounded_roster_bytes(reinterpret_cast<std::uintptr_t>(source.data()),captured,
   [&](auto at,void* out,std::size_t bytes){++calls;assert(bytes<=65536);return outfit_match_v2::read_memory(at,out,bytes);}));
  assert(calls==34&&captured==source);
  for(std::size_t n=0;n<kMixedTeams;++n){std::uint32_t id{},kind{},oldid{},oldkind{};std::array<wchar_t,24> name{},oldname{};
   assert(decode_roster_fields(captured,n,id,kind,name));const auto off=n*kTeamStride;
   std::memcpy(&oldid,source.data()+off+0xF20,4);std::memcpy(&oldkind,source.data()+off+0x123C,4);
   std::memcpy(oldname.data(),source.data()+off+0x2FA,sizeof oldname);assert(id==oldid&&kind==oldkind&&name==oldname);}
  std::uint32_t x{},y{};std::array<wchar_t,24> name{};assert(!decode_roster_fields(captured,403,x,y,name));
  captured.pop_back();assert(!decode_roster_fields(captured,0,x,y,name));
  for(unsigned fail=1;fail<=34;++fail){calls=0;assert(!bounded_roster_bytes(reinterpret_cast<std::uintptr_t>(source.data()),captured,
    [&](auto at,void* out,std::size_t bytes){++calls;if(calls==fail){std::memcpy(out,reinterpret_cast<void*>(at),bytes-1);return false;}
    return outfit_match_v2::read_memory(at,out,bytes);}));assert(calls==fail&&captured.empty());}
 }
 {
  void* allocation=VirtualAlloc(nullptr,8192,MEM_RESERVE|MEM_COMMIT,PAGE_READWRITE);assert(allocation);
  const auto at=reinterpret_cast<std::uintptr_t>(allocation);ZeroBridgeSnapshot a{},b{};
  assert(zero_bridge_snapshot(at,a)&&zero_bridge_snapshot(at,b)&&a==b);
  static_cast<unsigned char*>(allocation)[8191]=1;assert(!zero_bridge_snapshot(at,b));static_cast<unsigned char*>(allocation)[8191]=0;
  DWORD old{};assert(VirtualProtect(static_cast<char*>(allocation)+4096,4096,PAGE_READONLY,&old));assert(!zero_bridge_snapshot(at,b));
  assert(VirtualProtect(static_cast<char*>(allocation)+4096,4096,PAGE_READWRITE,&old));assert(zero_bridge_snapshot(at,b));
  assert(VirtualFree(allocation,0,MEM_RELEASE));assert(!zero_bridge_snapshot(at,b));
  for(SIZE_T bytes:{4096u,16384u}){void* p=VirtualAlloc(nullptr,bytes,MEM_RESERVE|MEM_COMMIT,PAGE_READWRITE);assert(p);assert(!zero_bridge_snapshot(reinterpret_cast<std::uintptr_t>(p),b));assert(VirtualFree(p,0,MEM_RELEASE));}
 }
 {
  auto* allocation=static_cast<unsigned char*>(VirtualAlloc(nullptr,8192,MEM_RESERVE|MEM_COMMIT,PAGE_READWRITE));assert(allocation);
  const auto at=reinterpret_cast<std::uintptr_t>(allocation),image=at-0x40000000;
  Code code{};std::int32_t dispatch{};assert(bridge_plan(image,at,code,dispatch));
  std::copy(code.begin(),code.end(),allocation);constexpr auto chain=wnba_blacktop_bridge_unwind::kChainInfo;
  std::copy(chain.begin(),chain.end(),allocation+4096);
  const RUNTIME_FUNCTION function{DWORD(at-image),DWORD(at-image+code.size()),DWORD(at-image+4096)};
  std::memcpy(allocation+4128,&function,sizeof function);
  assert(prepared_bridge_state(at,image,code,PAGE_READWRITE,PAGE_READWRITE));
  for(unsigned offset:{0u,9u,10u,4095u,4096u,4111u,4112u,4127u,4128u,4139u,4140u,8191u}) {
   allocation[offset]^=1;assert(!prepared_bridge_state(at,image,code,PAGE_READWRITE,PAGE_READWRITE));allocation[offset]^=1;
  }
  auto wrong=code;wrong[0]^=1;assert(!prepared_bridge_state(at,image,wrong,PAGE_READWRITE,PAGE_READWRITE));
  assert(!prepared_bridge_state(at,image+1,code,PAGE_READWRITE,PAGE_READWRITE));
  assert(!prepared_bridge_state(at,image,code,PAGE_EXECUTE_READ,PAGE_READWRITE));
  assert(!prepared_bridge_state(at,image,code,PAGE_READONLY,PAGE_READWRITE));
  DWORD old{};assert(VirtualProtect(allocation,4096,PAGE_EXECUTE_READ,&old));
  assert(prepared_bridge_state(at,image,code,PAGE_EXECUTE_READ,PAGE_READWRITE));
  assert(!prepared_bridge_state(at,image,code,PAGE_READWRITE,PAGE_READWRITE));
  allocation[8191]=1;assert(!prepared_bridge_state(at,image,code,PAGE_EXECUTE_READ,PAGE_READWRITE));allocation[8191]=0;
  assert(VirtualProtect(allocation+4096,4096,PAGE_READONLY,&old));
  assert(prepared_bridge_state(at,image,code,PAGE_EXECUTE_READ,PAGE_READONLY));
  assert(!prepared_bridge_state(at,image,code,PAGE_EXECUTE_READ,PAGE_READWRITE));
  assert(VirtualProtect(allocation+4096,4096,PAGE_READWRITE,&old));allocation[4128]^=1;
  assert(VirtualProtect(allocation+4096,4096,PAGE_READONLY,&old));
  assert(!prepared_bridge_state(at,image,code,PAGE_EXECUTE_READ,PAGE_READONLY));
  assert(VirtualFree(allocation,0,MEM_RELEASE));
 }
 const auto root=fs::canonical(L"C:/Users/carrn/CodexProjects/2kconversiontool/nba2k27-launcher-tauri2/artifacts");
 auto folder=root/(L"qa078-host-integration-self-"+std::to_wstring(GetCurrentProcessId())+L"-"+std::to_wstring(GetTickCount64()));
 folder.make_preferred();assert(fs::create_directory(folder));outfit_match_v2::PinnedDirectory directory;if(!directory.open(folder.wstring())){std::wcerr<<L"DIR="<<folder.wstring()<<L" error="<<GetLastError()<<L"\n";assert(false);}
 FILETIME birth{},e{},k{},u{};assert(GetProcessTimes(GetCurrentProcess(),&birth,&e,&k,&u));
 Binding binding{GetCurrentProcessId(),outfit_match_v2::ticks(birth),"0123456789abcdef0123456789abcdef0123456789abcdef"};
 auto fixture=[&](const wchar_t* name,bool mutable_file){
  IntentProof proof;proof.binding=binding;proof.base=0x140000000;proof.ordinal=5;proof.bytes="self-owned-original-intent\n";proof.path=(folder/name).wstring();
  Handle h(CreateFileW(proof.path.c_str(),GENERIC_READ|GENERIC_WRITE,mutable_file?7:FILE_SHARE_READ,nullptr,CREATE_NEW,FILE_FLAG_OPEN_REPARSE_POINT|FILE_FLAG_WRITE_THROUGH,nullptr));assert(h);
  assert(outfit_match_v2::file_id(h.value,false,proof.id));DWORD written{};assert(WriteFile(h.value,proof.bytes.data(),DWORD(proof.bytes.size()),&written,nullptr)&&written==proof.bytes.size()&&FlushFileBuffers(h.value));
  return std::pair{std::move(h),std::move(proof)};
 };
 auto [normal,proof]=fixture(L"normal.intent",false);
 const auto verify=[&](auto& p,HANDLE h){return p.unchanged(h,binding,0x140000000,5,[&]{return directory.unchanged();});};
 assert(verify(proof,normal.value));
 for(unsigned f=0;f<7;++f){auto p=proof;if(f==0)++p.binding.pid;if(f==1)++p.binding.birth;if(f==2)p.binding.nonce[0]='a';if(f==3)++p.ordinal;if(f==4)++p.base;if(f==5)p.id.index[0]^=1;if(f==6)p.bytes[0]='X';assert(!verify(p,normal.value));}
 assert(!proof.unchanged(normal.value,binding,0x140000000,5,[]{return false;}));
 Handle denied(CreateFileW(proof.path.c_str(),GENERIC_WRITE,7,nullptr,OPEN_EXISTING,0,nullptr));assert(!denied);
 assert(!MoveFileExW(proof.path.c_str(),(folder/L"renamed.intent").c_str(),0));
 auto [retained,mutableproof]=fixture(L"mutable.intent",true);
 assert(verify(mutableproof,retained.value));
 auto overwrite=[&](std::string bytes){LARGE_INTEGER zero{};assert(SetFilePointerEx(retained.value,zero,nullptr,FILE_BEGIN));DWORD wrote{};assert(WriteFile(retained.value,bytes.data(),DWORD(bytes.size()),&wrote,nullptr)&&wrote==bytes.size()&&SetEndOfFile(retained.value)&&FlushFileBuffers(retained.value));};
 auto changed=mutableproof.bytes;changed[0]='X';overwrite(changed);assert(!verify(mutableproof,retained.value));overwrite(mutableproof.bytes);assert(verify(mutableproof,retained.value));
 overwrite(mutableproof.bytes.substr(1));assert(!verify(mutableproof,retained.value));overwrite(mutableproof.bytes);
 assert(MoveFileExW(mutableproof.path.c_str(),(folder/L"original-retained.intent").c_str(),0));
 Handle replacement(CreateFileW(mutableproof.path.c_str(),GENERIC_READ|GENERIC_WRITE,7,nullptr,CREATE_NEW,0,nullptr));assert(replacement);DWORD wrote{};
 assert(WriteFile(replacement.value,mutableproof.bytes.data(),DWORD(mutableproof.bytes.size()),&wrote,nullptr)&&FlushFileBuffers(replacement.value));assert(!verify(mutableproof,retained.value));
 assert(!verify(proof,INVALID_HANDLE_VALUE));
 std::cout<<"QA078_HOST_SELF_PROOFS_PASS typed_masks=1024 faults=13 roster_rows=403 bulk_reads=34 old_reads=1209 partial_failures=34 seal_phases=3 seal_tamper_offsets=12 game_access=0\n";
}
