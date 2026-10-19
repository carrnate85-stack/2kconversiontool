#pragma once
#include "character_queue_noop.hpp"
#include "session_diagnostics.hpp"
#include <cmath>
namespace coach_clone_audit {
namespace mem=character_queue_noop;
struct Snapshot {
 const char *failure="Not captured";
 std::uintptr_t address{},owner{},array{},owned_array{},override_data{};
 unsigned count{},resource_count{};
 std::array<std::uintptr_t,32> resources{};
 std::array<unsigned char,0xf38> bytes{};
 std::array<std::uintptr_t,8> items{};
 std::array<std::array<wchar_t,256>,8> names{};
};
inline bool capture(std::uintptr_t at,std::uintptr_t base,Snapshot &s) {
 s={};s.address=at;
 auto reject=[&](const char *reason){s.failure=reason;return false;};
 if(!mem::read(at,s.bytes.data(),s.bytes.size())||mem::q(at)!=base+0x6804818||
    mem::q(at+0xe8)!=at+0x128)return reject("Description identity or embedded roster");
 auto sentinel=at+0x100,node=mem::q(at+0x110),previous=sentinel;
 while(node!=sentinel){
  if(node<0x10000||s.resource_count==s.resources.size()||mem::q(node+8)!=previous)return reject("Resource list links or bound");
  s.resources[s.resource_count++]=node;previous=node;node=mem::q(node+0x10);
 }
 if(mem::q(at+0x108)!=previous)return reject("Resource list tail");
 s.owner=mem::q(at+0x118);s.override_data=mem::q(at+0x120);s.array=mem::q(at+0xc8);s.count=mem::i(at+0xd0);
 if(s.count>8)return reject("Clothing count exceeds eight");
 if(!s.count){if(s.owner||s.array)return reject("Empty clothing owner mismatch");s.failure=nullptr;return true;}
 if(!s.owner||mem::q(s.owner)!=base+0x683a4c8||mem::i(s.owner+8)!=s.count||
    mem::q(s.owner+0x10)!=s.array||!mem::read(s.array,s.items.data(),s.count*8))return reject("Clothing owner or exposed array");
 s.owned_array=mem::q(s.owner+0x20);std::array<std::uintptr_t,8> owned_items{};
 if(!s.owned_array||!mem::read(s.owned_array,owned_items.data(),s.count*8)||owned_items!=s.items)return reject("Owned array contents mismatch");
 for(unsigned n=0;n<s.count;++n){if(!s.items[n])continue;bool end=false;for(unsigned k=0;k<256;++k){
  if(!mem::value(s.items[n]+2*k,&s.names[n][k]))return reject("Clothing string unreadable");
  if(!s.names[n][k]){end=k>0;break;}
 }if(!end)return reject("Clothing string empty or unterminated");}
 s.failure=nullptr;
 return true;
}
inline bool independent(const Snapshot &a,const Snapshot &b) {
 if(a.address==b.address||a.count!=b.count||a.names!=b.names||b.resource_count)return false;
 for(auto x:{a.owner,a.array,a.owned_array,a.override_data})for(auto y:{b.owner,b.array,b.owned_array,b.override_data})if(x&&x==y)return false;
 for(unsigned i=0;i<a.count;++i){
  if(bool(a.items[i])!=bool(b.items[i]))return false;
  for(unsigned k=0;k<b.count;++k)if(a.items[i]&&a.items[i]==b.items[k])return false;
 }
 // Flags and label are copied identity; pointer-bearing fields are checked separately.
 return std::memcmp(a.bytes.data()+0x7c,b.bytes.data()+0x7c,0x10)==0;
}
inline bool active(std::uintptr_t base,std::uintptr_t manager,std::uintptr_t actor) {
 const auto sentinel=base+0xa37bfb0;auto node=mem::q(sentinel+0x10),prev=sentinel;unsigned matches=0;
 for(unsigned n=0;n<256;++n){
  if(node==sentinel)return matches==1&&mem::q(sentinel+8)==prev&&mem::q(manager+0x98)==actor;
  if(mem::q(node)!=base+0x681ccc8||mem::q(node+8)!=prev)return false;
  if(node+0x78==manager)++matches;prev=node;node=mem::q(node+0x10);
 }return false;
}
#ifdef NBA2K27_COACH_SAME_SIZE_REBUILD
#include "coach_same_size_rebuild.hpp"
#endif
inline DWORD worker(bool (*anti_cheat)()) {
 wchar_t exe[32768]{};if(!GetModuleFileNameW(nullptr,exe,32768))return 1;
 auto leaf=wcsrchr(exe,L'\\');if(!leaf||_wcsicmp(leaf+1,L"NBA2K27.exe"))return 0;
 const auto base=reinterpret_cast<std::uintptr_t>(GetModuleHandleW(nullptr));
 auto directory=session_diagnostics_directory();if(directory.empty())return 1;
#ifdef NBA2K27_COACH_ALL_SCALE
 const auto prefix=directory+L"nba2k27_coach_all_scale_"+std::to_wstring(GetCurrentProcessId());
 constexpr auto schema="coach-all-scale-v1",token_expected="COACH_ALL_SCALE_V1";
 coach_rebuild::Result rebuild{};unsigned completed=0,total=0;std::string history;
#elif defined(NBA2K27_COACH_SAME_SIZE_REBUILD)
 const auto prefix=directory+L"nba2k27_coach_rebuild_"+std::to_wstring(GetCurrentProcessId());
 constexpr auto schema="coach-same-size-rebuild-v1",token_expected="COACH_REBUILD_V1";
 coach_rebuild::Result rebuild{};
#else
 const auto prefix=directory+L"nba2k27_coach_clone_"+std::to_wstring(GetCurrentProcessId());
 constexpr auto schema="coach-description-clone-v1",token_expected="COACH_CLONE_V1";
#endif
 unsigned calls=0,disposed=0;bool equal=false,unchanged=false;
 auto report=[&](const char *state,const char *reason){char text[1200]{};
#ifdef NBA2K27_COACH_SAME_SIZE_REBUILD
  if(std::strcmp(state,"PENDING")==0){rebuild.submitted=true;rebuild.witnessed=true;}
#endif
  const char *source_status=unchanged?"YES":"NOT_CHECKED";
#ifdef NBA2K27_COACH_SAME_SIZE_REBUILD
  if(rebuild.submitted)source_status="NOT_CHECKED_AFTER_TRANSFER";
#endif
  std::snprintf(text,sizeof(text),"SCHEMA=%s\r\nPID=%lu\r\nSTATE=%s\r\nREASON=%s\r\nCLONE_CALLS=%u\r\nDESTRUCTOR_RETURNS=%u\r\nINDEPENDENT_COPY=%s\r\nSOURCE_UNCHANGED=%s\r\nGAME_CODE_HOOKS=0\r\nLIVE_SCALE_WRITES=0\r\n",schema,GetCurrentProcessId(),state,reason,calls,disposed,equal?"YES":"NO",source_status);
#ifdef NBA2K27_COACH_SAME_SIZE_REBUILD
  auto used=std::strlen(text);std::snprintf(text+used,sizeof(text)-used,"REQUEST_SUBMITTED=%s\r\nREQUEST_WITNESSED=%s\r\nQUEUE_DRAINED=%s\r\nCOACH_REPLACED=%s\r\nCLOTHING_UNCHANGED=%s\r\nSIZE_UNCHANGED=%s\r\nSIZE_BEFORE=%.9g\r\nSIZE_AFTER=%.9g\r\n",rebuild.submitted?"YES":"NO",rebuild.witnessed?"YES":"NO",rebuild.drained?"YES":"NO",rebuild.replaced?"YES":"NO",rebuild.clothing?"YES":"NO",rebuild.same_size?"YES":"NO",rebuild.before,rebuild.after);
#else
  std::strcat(text,"REBUILD=OFF\r\n");
#endif
#ifdef NBA2K27_COACH_ALL_SCALE
  auto end=std::strlen(text);std::snprintf(text+end,sizeof(text)-end,"COACHES_COMPLETED=%u\r\nCOACHES_FOUND=%u\r\nEXPECTED_FACTOR=%.9g\r\nMEASURED_FACTOR=%.9g\r\nSCALE_MATCH=%s\r\n",completed,total,rebuild.factor,rebuild.before>0?rebuild.after/rebuild.before:0,rebuild.same_size?"YES":"NO");
  // Preserve per-coach results without exposing a terminal state until the batch ends.
  auto batch_text=std::string(text);const auto size_label=batch_text.find("SIZE_UNCHANGED=");
  if(size_label!=std::string::npos)batch_text.replace(size_label,15,"SIZE_MATCHES_EXPECTED=");
  return mem::write_log(prefix+L".log",(batch_text+history).c_str());
#else
  return mem::write_log(prefix+L".log",text);
#endif
 };
 if(!mem::exact_build(base)||anti_cheat()){report("FAILED","Build or anti-cheat guard");return 2;}
 if(!report("ARMED","Enter a regular match, pause, then trigger the diagnostic"))return 3;
 bool triggered=false;
 for(unsigned n=0;n<2400;++n){
  HANDLE f=CreateFileW((prefix+L".trigger").c_str(),GENERIC_READ,FILE_SHARE_READ,nullptr,OPEN_EXISTING,0,nullptr);
  if(f!=INVALID_HANDLE_VALUE){char token[64]{};DWORD got=0;bool ok=ReadFile(f,token,63,&got,nullptr)!=FALSE;CloseHandle(f);
   triggered=ok&&got==std::strlen(token_expected)&&std::memcmp(token,token_expected,got)==0;break;}
  Sleep(500);
 }
 if(!triggered||!DeleteFileW((prefix+L".trigger").c_str())){report("FAILED","Missing trigger");return 4;}
 if(!mem::exact_build(base)||anti_cheat()||mem::q(base+0x68048c0)!=base+0x64f340||mem::q(base+0x6804818)!=base+0x8072f0||
   !mem::match_hex(base+0x64f340,"5556574883ec50488d6c245048c745f8feffffff4889d74889ce4885d2750ab13de88a455d034889c7c644243800c644243001c744242808000000c744242041000000b9380f00004889fa4531c041b9a37b658de837fd4c044885c074144889c14889f24989f84889c6e8310000004889f04883c4505f5e5dc3")||
   !mem::match_hex(base+0x8072f0,"555657534883ec28488d6c242048c74500feffffff89d74889ce488d05f723ff05488981f0000000488d9900010000488b8910010000eb0f0f1f840000000000488b8e100100004839d9742d488b4108488b511048895010488b41084889420848894908488949104885c974d3488b01ba01000000ff10ebc7488b8e180100004885c9740a488b01ba01000000ff1048c7861801000000000000488b8e200100004885c97405e8a57f310448c7862001000000000000488d051b20ff05488986f8000000488d055d1fff0548898600010000c5f857c0c5f811860801000085ff74084889f1e8667f31044889f04883c4285b5f5e5dc3")){
  report("FAILED","Clone fingerprint rejected");return 5;
 }
#ifdef NBA2K27_COACH_ALL_SCALE
 std::array<std::pair<std::uintptr_t,std::uintptr_t>,8> coaches{};
 const auto list=base+0xa37bfb0;auto cursor=mem::q(list+0x10),previous=list;
 for(unsigned n=0;n<256&&cursor!=list;++n){
  if(mem::q(cursor)!=base+0x681ccc8||mem::q(cursor+8)!=previous){report("FAILED","Active manager list rejected");return 6;}
  auto m=cursor+0x78,p=mem::q(m+0x98);
  if(mem::q(p)==base+0x6804e58){if(total==coaches.size()||!active(base,m,p)){report("FAILED","Coach list bound or identity rejected");return 6;}coaches[total++]={m,p};}
  previous=cursor;cursor=mem::q(cursor+0x10);
 }
 if(!total||cursor!=list||mem::q(list+8)!=previous){report("FAILED","Coach list incomplete");return 6;}
 const auto batch_root=mem::q(base+0xa3a69e8);
 for(unsigned index=0;index<total;++index){Snapshot preflight{};
  const auto [m,p]=coaches[index];
  if(!active(base,m,p)||!capture(p+0xf20,base,preflight)||(preflight.bytes[0x7d]&2)||
     !same_outfit_rebuild::idle(base,m,p,mem::i(p+0xf20+0x88))){report("FAILED","Not all coaches ready; no batch submitted");return 6;}
 }
 // Snapshot manager/actor identities once; never adopt a new coach after a transition.
 for(unsigned index=0;index<total;++index){
 auto [manager,actor]=coaches[index];rebuild={};equal=false;unchanged=false;
 if(mem::q(base+0xa3a69e8)!=batch_root){report("INCONCLUSIVE","Match changed during batch; close game");return 6;}
#else
 std::uintptr_t manager=0,actor=0;auto sentinel=base+0xa37bfb0;auto node=mem::q(sentinel+0x10);
 for(unsigned n=0;n<256&&node!=sentinel;++n){if(mem::q(node)!=base+0x681ccc8)break;auto m=node+0x78,p=mem::q(m+0x98);
  if(mem::q(p)==base+0x6804e58&&active(base,m,p)){manager=m;actor=p;break;}node=mem::q(node+0x10);}
#endif
 const auto root=mem::q(base+0xa3a69e8);Snapshot source{},copy{},after{};
 auto stable=[&](){unsigned char pending=1;return !anti_cheat()&&mem::i(mem::q(base+0xaac79a0)+0xa04)==1&&
  mem::i(base+0x89986d0)&&mem::i(base+0x89986d0)!=GetCurrentThreadId()&&root&&mem::q(root)==base+0x683d918&&mem::q(base+0xa3a69e8)==root&&
  active(base,manager,actor)&&mem::q(actor)==base+0x6804e58&&!mem::q(manager+0xa0)&&mem::i(manager+0x18)==1&&
  mem::value(manager+0xb8,&pending)&&pending==0;};
 const auto heap=mem::q(base+0xa37c1d8);
 if(!manager){report("FAILED","No active coach manager");return 6;}
 if(!heap||!mem::q(heap)){report("FAILED","Native heap not ready");return 6;}
 if(!stable()){report("FAILED","Coach manager or match is not idle and stable");return 6;}
 if(!capture(actor+0xf20,base,source)){report("FAILED",source.failure);return 6;}
 if(!capture(source.address,base,after)||source.bytes!=after.bytes){report("FAILED","Source description changed during preflight");return 6;}
 if(!report("CLONING","One private coach copy; live coach unchanged"))return 7;
 using Clone=void *(*)(void *,void *);++calls;
 auto clone=reinterpret_cast<std::uintptr_t>(reinterpret_cast<Clone>(base+0x64f340)(reinterpret_cast<void *>(source.address),reinterpret_cast<void *>(heap)));
 if(!clone){report("FAILED","Native copy returned null");return 8;}
 if(!capture(clone,base,copy)||!independent(source,copy)){report("INCONCLUSIVE","Copy ownership not verified; close game, no speculative disposal");return 9;}
 equal=true;using Dispose=void *(*)(void *,unsigned);
#ifdef NBA2K27_COACH_SAME_SIZE_REBUILD
 rebuild=coach_rebuild::run(base,manager,actor,clone,source,anti_cheat,report);
 if(rebuild.submitted){
#ifdef NBA2K27_COACH_ALL_SCALE
  char row[240]{};std::snprintf(row,sizeof(row),"COACH_%u_BEFORE=%.9g AFTER=%.9g EXPECTED_FACTOR=%.9g SCALE_MATCH=%s CLOTHING_MATCH=%s\r\n",index+1,rebuild.before,rebuild.after,rebuild.factor,rebuild.same_size?"YES":"NO",rebuild.clothing?"YES":"NO");history+=row;
  if(!rebuild.replaced||!rebuild.clothing||!rebuild.same_size){report("INCONCLUSIVE","Stopped batch: coach result not verified; close game");return 11;}
  ++completed;report("PENDING","Coach verified; continuing sequential batch");continue;
#else
  report(rebuild.replaced&&rebuild.clothing&&rebuild.same_size?"REBUILT":"INCONCLUSIVE",rebuild.reason);return rebuild.replaced&&rebuild.clothing&&rebuild.same_size?0:11;
#endif
 }
#endif
 reinterpret_cast<Dispose>(base+0x8072f0)(reinterpret_cast<void *>(clone),1);++disposed;
 unchanged=stable()&&capture(source.address,base,after)&&source.bytes==after.bytes&&source.names==after.names&&source.resources==after.resources;
#ifdef NBA2K27_COACH_SAME_SIZE_REBUILD
 report("FAILED",rebuild.reason);return 12;
#else
 report(unchanged?"PASS":"INCONCLUSIVE",unchanged?"Independent coach copy disposed; live source unchanged":"Copy disposed; source stability changed");return unchanged?0:10;
#endif
#ifdef NBA2K27_COACH_ALL_SCALE
 }
 report("REBUILT","All captured coaches rebuilt and scale verified; check appearance and gameplay");return 0;
#endif
}
}
