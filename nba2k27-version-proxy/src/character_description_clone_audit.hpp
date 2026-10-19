#pragma once
#include "character_queue_noop.hpp"
#include "session_diagnostics.hpp"
namespace character_description_clone_audit {
namespace mem=character_queue_noop;
struct Snapshot {
  std::uintptr_t address{},array{},owner{},owned_array{};
  unsigned count{};
  std::array<unsigned char,0x2970> bytes{};
  std::array<std::uintptr_t,7> items{};
  std::array<std::array<wchar_t,256>,7> names{};
};
inline bool capture(std::uintptr_t address,std::uintptr_t base,Snapshot &out) {
  out={};out.address=address;
  if(!mem::read(address,out.bytes.data(),out.bytes.size()))return false;
  auto ptr=[&](std::size_t at){std::uintptr_t result{};std::memcpy(&result,out.bytes.data()+at,8);return result;};
  if(ptr(0)!=base+0x682AB58||ptr(0xB8)!=address+0x378||(out.bytes[0x7D]&1))return false;
  out.array=ptr(0xC8);out.owner=ptr(0x368);
  std::memcpy(&out.count,out.bytes.data()+0xD0,4);
  if(out.count<1||out.count>7||!out.owner||mem::q(out.owner)!=base+0x683A4C8||
      mem::q(out.owner+0x10)!=out.array||mem::i(out.owner+8)!=out.count||
      !mem::read(out.array,out.items.data(),out.count*8))return false;
  out.owned_array=mem::q(out.owner+0x20);
  std::array<std::uintptr_t,7> owned_items{};
  if(!mem::read(out.owned_array,owned_items.data(),out.count*8)||owned_items!=out.items)return false;
  for(unsigned n=0;n<out.count;++n) {
    bool terminated=false;
    for(unsigned c=0;c<256;++c) {
      if(!mem::value(out.items[n]+c*2,&out.names[n][c]))return false;
      if(!out.names[n][c]){terminated=c>0;break;}
    }
    if(!terminated)return false;
  }
  return std::wcscmp(out.names[0].data(),L"VTXANI_JERSEY")!=0;
}
inline bool unchanged(const Snapshot &a,const Snapshot &b) {
  return a.address==b.address&&a.bytes==b.bytes&&a.items==b.items&&a.names==b.names&&a.owned_array==b.owned_array;
}
inline bool independent(const Snapshot &a,const Snapshot &b) {
  if(a.address==b.address||a.array==b.array||a.owner==b.owner||
      a.owned_array==b.owned_array||a.owned_array==b.array||a.array==b.owned_array)return false;
  // These pointers are released by the native description destructor. Reject
  // any shared non-null allocation, even across different owned fields.
  constexpr std::size_t owned_offsets[]={0x368,0x370,0x2940,0x2948,0x2950,0x2958};
  for(auto x:owned_offsets)for(auto y:owned_offsets) {
    std::uintptr_t ap{},bp{};std::memcpy(&ap,a.bytes.data()+x,8);std::memcpy(&bp,b.bytes.data()+y,8);
    if(ap&&ap==bp)return false;
  }
  for(unsigned n=0;n<a.count;++n)
    for(unsigned m=0;m<b.count;++m)if(a.items[n]==b.items[m])return false;
  return true;
}
inline bool first_item_changed_only(const Snapshot &before,const Snapshot &after,const wchar_t *expected) {
  if(before.address!=after.address||before.bytes!=after.bytes||before.count!=after.count||
      before.owner!=after.owner||before.array!=after.array||before.owned_array!=after.owned_array||
      std::wcscmp(after.names[0].data(),expected)!=0)return false;
  for(unsigned n=1;n<before.count;++n)
    if(before.items[n]!=after.items[n]||before.names[n]!=after.names[n])return false;
  return true;
}
#ifdef NBA2K27_SAME_OUTFIT_REBUILD
#include "character_same_outfit_rebuild.hpp"
#endif
#ifdef NBA2K27_EXPAND_COPY
#include "character_expand_copy.hpp"
#endif
#ifdef NBA2K27_SAVED_OUTFIT_PLAN
#include "character_saved_plan.hpp"
inline DWORD worker_once(bool (*anti_cheat)(),const SavedChoice *choice,unsigned ordinal) {
#else
inline DWORD worker(bool (*anti_cheat)()) {
#endif
  wchar_t exe[32768]{};
  if(!GetModuleFileNameW(nullptr,exe,32768))return 1;
  const auto leaf=wcsrchr(exe,L'\\');
  if(!leaf||_wcsicmp(leaf+1,L"NBA2K27.exe"))return 0;
  const auto base=reinterpret_cast<std::uintptr_t>(GetModuleHandleW(nullptr));
  const auto pid=GetCurrentProcessId();
#if defined(NBA2K27_EXPAND_COPY)
#ifdef NBA2K27_FULL_OUTFIT_REBUILD
#ifdef NBA2K27_SAVED_OUTFIT_PLAN
  constexpr auto path_prefix=L"nba2k27_saved_outfits_";
  constexpr auto schema="blacktop-saved-outfit-player-v1";
#else
  constexpr auto path_prefix=L"nba2k27_full_outfit_";
  constexpr auto schema="blacktop-full-outfit-rebuild-v1";
#endif
  constexpr auto token_expected="FULL_OUTFIT_V1";
  same_outfit_rebuild::Result rebuilt_result{};
#else
  constexpr auto path_prefix=L"nba2k27_expand_copy_";
  constexpr auto schema="blacktop-expand-copy-v1";
  constexpr auto token_expected="EXPAND_COPY_V1";
#endif
  unsigned releases=0,inits=0,setters=0;bool expanded=false;
#elif defined(NBA2K27_SAME_OUTFIT_REBUILD)
#ifdef NBA2K27_CHANGE_TOP_REBUILD
  constexpr auto path_prefix=L"nba2k27_change_top_rebuild_";
  constexpr auto schema="blacktop-change-top-rebuild-v1";
  constexpr auto token_expected="CHANGE_TOP_REBUILD_V1";
#else
  constexpr auto path_prefix=L"nba2k27_same_outfit_rebuild_";
  constexpr auto schema="blacktop-same-outfit-rebuild-v1";
  constexpr auto token_expected="SAME_OUTFIT_REBUILD_V1";
#endif
  same_outfit_rebuild::Result rebuilt_result{};
#elif defined(NBA2K27_DESCRIPTION_CLONE_EDIT)
  constexpr auto path_prefix=L"nba2k27_description_edit_";
  constexpr auto schema="blacktop-description-edit-v1";
  constexpr auto token_expected="DESCRIPTION_EDIT_V1";
  unsigned edits=0;bool edit_ok=false,restore_ok=false;
#else
  constexpr auto path_prefix=L"nba2k27_description_clone_";
  constexpr auto schema="blacktop-description-clone-v1";
  constexpr auto token_expected="DESCRIPTION_CLONE_V1";
#endif
  auto directory=std::wstring(exe,leaf-exe+1);
#ifdef NBA2K27_SESSION_OUTFITS
  directory=session_diagnostics_directory();if(directory.empty())return 1;
#endif
  std::wstring prefix=directory+path_prefix+std::to_wstring(pid);
#ifdef NBA2K27_SAVED_OUTFIT_PLAN
  prefix+=L"_player"+std::to_wstring(ordinal);
  (void)token_expected;
#endif
  unsigned calls=0,deletes=0,slot=99;bool equal=false,source_ok=false;std::uintptr_t clone=0;
  auto report=[&](const char *state,const char *reason) {
    char text[2048]{};
#ifdef NBA2K27_SAME_OUTFIT_REBUILD
#if defined(NBA2K27_SAVED_OUTFIT_PLAN)
    const char *note="Session-bound saved outfit, matched by original roster identity. Check appearance and gameplay.";
#elif defined(NBA2K27_FULL_OUTFIT_REBUILD)
    const char *note="Four-piece saved Embiid outfit on first verified player; no hooks. Check visible outfit and gameplay.";
#elif defined(NBA2K27_CHANGE_TOP_REBUILD)
    const char *note="Cloned top changed to tshirt_ss_team_016_orl. Stock rebuild; no hooks or forced rollback. Check shirt and gameplay.";
#else
    const char *note="Stock queue may rebuild one live player with same outfit. No hooks. Gameplay check required; no manual rollback.";
#endif
    const char *source_status=rebuilt_result.submitted?"NOT_CHECKED_AFTER_TRANSFER":(source_ok?"YES":"NO");
#else
    const char *note="Native temporary allocation and disposal only. No player rebuild or replacement. Stay paused during test.";
    const char *source_status=source_ok?"YES":"NO";
#endif
    std::snprintf(text,sizeof(text),
      "SCHEMA=%s\r\nPID=%lu\r\nSTATE=%s\r\nREASON=%s\r\n"
      "TABLE_INDEX=%u\r\nCLONE_CALLS=%u\r\nCLONE_DESTRUCTOR_RETURNS=%u\r\n"
      "INDEPENDENT_EQUAL_CLOTHING=%s\r\nSOURCE_UNCHANGED=%s\r\n"
      "GAME_CODE_HOOKS=0\r\nOBJECT_HOOKS=0\r\nPROVIDER_REGISTRATION=OFF\r\nLIVE_OUTFIT_WRITES=0\r\n"
      "NOTE=%s\r\n",
      schema,pid,state,reason,slot,calls,deletes,equal?"YES":"NO",source_status,note);
#ifdef NBA2K27_EXPAND_COPY
    const auto used=std::strlen(text);
    std::snprintf(text+used,sizeof(text)-used,
      "PRIVATE_LIST_RELEASES=%u\r\nPRIVATE_LIST_INITS=%u\r\nPRIVATE_SETTERS=%u\r\nEXPANSION_2_TO_4=%s\r\n",
      releases,inits,setters,expanded?"PASS":"NOT_VERIFIED");
#endif
#ifdef NBA2K27_DESCRIPTION_CLONE_EDIT
    const auto used=std::strlen(text);
    std::snprintf(text+used,sizeof(text)-used,
      "TEMP_COPY_SETTER_CALLS=%u\r\nTEMP_EDIT_VERIFIED=%s\r\nTEMP_RESTORE_VERIFIED=%s\r\n",
      edits,edit_ok?"YES":"NO",restore_ok?"YES":"NO");
#endif
#ifdef NBA2K27_SAME_OUTFIT_REBUILD
    const auto rebuild_used=std::strlen(text);
    std::snprintf(text+rebuild_used,sizeof(text)-rebuild_used,
      "STOCK_REBUILD_REQUEST=%s\r\nQUEUE_DRAIN_OBSERVED=%s\r\nNEW_PLAYER_OBSERVED=%s\r\n"
      "BINDINGS_UNCHANGED=%s\r\nEXPECTED_CLOTHING_MATCH=%s\r\nNEW_PLAYER=0x%llX\r\n"
      "LIVE_MODEL_REBUILD=%s\r\nGAMEPLAY_REQUIRES_USER_CHECK=YES\r\nREQUEST_WITNESSED=%s\r\n",
      rebuilt_result.submitted?"SUBMITTED":"NOT_SUBMITTED",rebuilt_result.drained?"YES":"NO",
      rebuilt_result.replaced?"YES":"NO",rebuilt_result.bindings?"YES":"NO",rebuilt_result.clothing?"YES":"NO",
      static_cast<unsigned long long>(rebuilt_result.new_player),rebuilt_result.submitted?"REQUESTED":"NO",rebuilt_result.witnessed?"YES":"NO");
#endif
    return mem::write_log(prefix+L".log",text);
  };
  if(!mem::exact_build(base)||anti_cheat()){report("FAILED","Build or anti-cheat guard");return 2;}
#ifndef NBA2K27_SAVED_OUTFIT_PLAN
  if(!report("ARMED","Enter and pause actual Blacktop match, then trigger copy-only audit"))return 3;
  bool triggered=false;
  for(unsigned n=0;n<1800;++n) {
    HANDLE file=CreateFileW((prefix+L".trigger").c_str(),GENERIC_READ,FILE_SHARE_READ,nullptr,OPEN_EXISTING,0,nullptr);
    if(file!=INVALID_HANDLE_VALUE) {
      char token[64]{};DWORD got=0;
      const bool ok=ReadFile(file,token,63,&got,nullptr)!=FALSE;CloseHandle(file);
      triggered=ok&&got==std::strlen(token_expected)&&std::memcmp(token,token_expected,got)==0;
      break;
    }
    Sleep(500);
  }
  if(!triggered||!DeleteFileW((prefix+L".trigger").c_str())){report("FAILED","Missing or invalid single-use trigger");return 4;}
#endif
  // Native worker branch 619537 ->5F9A20 calls description virtual+A8 at5F9CBD
  // BEFORE taking create queue lock at5F9CEC. No queue or player creation here.
  if(!mem::exact_build(base)||anti_cheat()||!mem::in_match(base)||
      !mem::i(base+0x89986D0)||mem::i(base+0x89986D0)==GetCurrentThreadId()||
      mem::q(base+0x682AB58)!=base+0x81DDD0||mem::q(base+0x682AC00)!=base+0x729150||
      !mem::match_hex(base+0x729150,"5541565657534883ec50488d6c245048c745f8feffffff4889d74889ce488b01ff90d80000004885c00f84aa000000488b08488d158f181006e83257440489c34885ff750ab13de854a74f034889c7c644243800c644243001c744242808000000c7442420d9000000b9702900004889fa4531c041b971fd4630e8015f3f044989c64885c074444c89f14889f24989f8e85b97ffff84db7432498b8e680300004885c9740a488b01ba01000000ff1049c7866803000000000000498b064c89f131d24531c0ff90c80000004c89f04883c4505b5f5e415e5dc331db4885ff0f856bffffffe95cffffff")||
      !mem::match_hex(base+0x81DDD0,"5556574883ec30488d6c243048c745f8feffffff89d74889cee86255f0ff85ff74084889f1e8461530044889f04883c4305f5e5dc3")||
      !mem::match_hex(base+0x619537,"4983c6e84c89f14889da4989f04189f9")||
      !mem::match_hex(base+0x5F9CAC,"488b152525d809498b04244c89e1c5f877ff90a8000000")) {
    report("FAILED","Runtime or native worker-clone guard rejected");return 5;
  }
  Snapshot source{},copy{},after{};std::uintptr_t record=0,manager=0,player=0;
  for(unsigned n=0;n<30;++n) {
    const auto candidate=base+0xAAC79B0+n*0x50;
    if(mem::q(candidate)!=base+0x75BD1F8)continue;
#ifdef NBA2K27_SAVED_OUTFIT_PLAN
    if(mem::q(mem::q(candidate+0x30)+0x500)!=choice->roster)continue;
#endif
    const auto m=mem::q(candidate+0x10),p=mem::q(m+0x98);
    if(mem::q(m)!=base+0x681D088||mem::q(p)!=base+0x6829528)continue;
    if(capture(p+0xF20,base,source)){record=candidate;manager=m;player=p;slot=n;break;}
  }
  auto stable=[&](){return mem::in_match(base)&&!anti_cheat()&&mem::q(record)==base+0x75BD1F8&&
#ifdef NBA2K27_SAVED_OUTFIT_PLAN
    mem::q(mem::q(record+0x30)+0x500)==choice->roster&&
#endif
    mem::q(record+0x10)==manager&&mem::q(manager)==base+0x681D088&&mem::q(manager+0x98)==player&&
    mem::q(player)==base+0x6829528;};
  if(!record||!stable()||!capture(source.address,base,after)||!unchanged(source,after)) {
    report("FAILED","No stable, owned ordinary outfit description; no clone called");return 6;
  }
  const auto heap=mem::q(base+0xA37C1D8);
  if(!heap||!mem::q(heap)){report("FAILED","Native heap unavailable");return 7;}
#if defined(NBA2K27_DESCRIPTION_CLONE_EDIT) || defined(NBA2K27_CHANGE_TOP_REBUILD) || defined(NBA2K27_EXPAND_COPY)
  if(!mem::match_hex(base+0x7AAA70,"5657534883ec604489c34889d74889ce448944244c83790800740685db7917eb4c4c8b46184889f1ba08000000e80ef8ffff85db78373b5e087d32488b462089db488b0cd8e8864837044885ff0f848c000000488b56184889f941b8702eb9ca41b96b030000e885e83c04eb74c5f82805dbbb0406c5f829442450488d44244c4889442450488d4424504889442440488d0584f808064889442430488d05d0f208064889442420c744243801000000c744242863030000488d0dd9044f05488d15d529fb054c8d05b15e05064c8d0d1dfa0806e8e839370484c0741dcceb1a31c0488b4e20488904d9488b4610488b4e20488b0cd948890cd84883c4605b5f5ec3")) {
    report("FAILED","Native clothing setter fingerprint rejected; no clone called");return 13;
  }
#endif
  if(!report("CLONING","One native copy call; leave match paused"))return 8;
  using Clone=void *(*)(void *,void *);
  ++calls;clone=reinterpret_cast<std::uintptr_t>(reinterpret_cast<Clone>(base+0x729150)(reinterpret_cast<void *>(source.address),reinterpret_cast<void *>(heap)));
  if(!clone){report("FAILED","Native clone returned null");return 9;}
  // Never dispose an alias into the live player or an unrecognized result.
  if((clone>=player&&clone<player+0x38B0)||mem::q(clone)!=base+0x682AB58) {
    report("INCONCLUSIVE","Unexpected clone identity; no speculative disposal, close game");return 10;
  }
  if(!capture(clone,base,copy)||!independent(source,copy)) {
    report("INCONCLUSIVE","Clone ownership not independent; no speculative disposal, close game");return 11;
  }
  equal=source.count==copy.count&&source.names==copy.names;
#ifdef NBA2K27_EXPAND_COPY
  if(!equal||copy.count!=2||!expand_fingerprints(base)||!stable()||
      !capture(source.address,base,after)||!unchanged(source,after)) {
    using Dispose=void *(*)(void *,unsigned);
    reinterpret_cast<Dispose>(base+0x81DDD0)(reinterpret_cast<void *>(clone),1);++deletes;
    report("FAILED","Expansion prerequisites rejected; private clone disposed");return 20;
  }
  report("EXPANDING_COPY","Private list only; extra markers are never loaded as assets");
  using DisposeList=void *(*)(void *,unsigned);
  reinterpret_cast<DisposeList>(base+0x823830)(reinterpret_cast<void *>(copy.owner),0);++releases;
  if(mem::i(copy.owner+8)||mem::q(copy.owner+0x10)||mem::q(copy.owner+0x20)) {
    report("INCONCLUSIVE","Private list release unverified; close game");return 21;
  }
  using InitList=void (*)(void *,int,void *);
  unsigned pieces=4;
#ifdef NBA2K27_SAVED_OUTFIT_PLAN
  pieces=choice->pieces;
#endif
  reinterpret_cast<InitList>(base+0x7AA2B0)(reinterpret_cast<void *>(copy.owner),static_cast<int>(pieces),reinterpret_cast<void *>(heap));++inits;
  const auto exposed=mem::q(copy.owner+0x10),owned=mem::q(copy.owner+0x20);
  std::array<std::uintptr_t,7> empty{};
  std::array<std::uintptr_t,7> check{};
  if(mem::i(copy.owner+8)!=pieces||!exposed||!owned||exposed==owned||
      !mem::read(owned,check.data(),pieces*sizeof(std::uintptr_t))||check!=empty) {
    report("INCONCLUSIVE","New private list allocation unverified; close game");return 22;
  }
  using SetItem=void (*)(void *,const wchar_t *,int);
#ifdef NBA2K27_FULL_OUTFIT_REBUILD
#ifdef NBA2K27_SAVED_OUTFIT_PLAN
  const wchar_t *names[kSavedMaxPieces]{};
  for(unsigned n=0;n<pieces;++n)names[n]=choice->names[n].data();
#else
  const wchar_t *names[]={L"basketball_shorts_classic_2kmycareer_elevate_logo2_usa_wht",L"tanktop_team_008_mem",L"acc_sock_nba_nike_wht_crew",L"shoe_5002_wolfgrey"};
#endif
#else
  const wchar_t *names[]={copy.names[0].data(),copy.names[1].data(),L"CODEX_PRIVATE_EXTRA_2",L"CODEX_PRIVATE_EXTRA_3"};
#endif
  for(unsigned n=0;n<pieces;++n){reinterpret_cast<SetItem>(base+0x7AAA70)(reinterpret_cast<void *>(copy.owner),names[n],static_cast<int>(n));++setters;}
  using Bind=void (*)(void *,void *,unsigned);
  reinterpret_cast<Bind>(base+0x17B970)(reinterpret_cast<void *>(clone),reinterpret_cast<void *>(exposed),pieces);
  Snapshot grown{};
  if(!capture(clone,base,grown)||!independent(source,grown)) {
    report("INCONCLUSIVE","Expanded ownership unverified; close game");return 23;
  }
  expanded=expanded_copy_matches(copy,grown,names,pieces);
#endif
#ifdef NBA2K27_SAME_OUTFIT_REBUILD
  Snapshot desired=copy;
  bool prepared=equal;
#ifdef NBA2K27_FULL_OUTFIT_REBUILD
  desired=grown;prepared=expanded;
  if(!prepared)rebuilt_result.reason="Private outfit copy verification failed";
#endif
#ifdef NBA2K27_CHANGE_TOP_REBUILD
  // Restrict this first visual test to a recognized standalone top, not a
  // combined uniform or a layered/unknown first clothing entry.
  const auto top=copy.names[0].data();
  prepared=equal&&copy.count>=2&&
    (std::wcsncmp(top,L"hoodie_pullover_team_",20)==0||std::wcsncmp(top,L"tshirt_ss_team_",14)==0)&&
    std::wcscmp(top,L"tshirt_ss_team_016_orl")!=0&&stable();
  if(prepared) {
    using SetItem=void (*)(void *,const wchar_t *,int);
    reinterpret_cast<SetItem>(base+0x7AAA70)(reinterpret_cast<void *>(copy.owner),L"tshirt_ss_team_016_orl",0);
    if(!capture(clone,base,desired)||!independent(source,desired)) {
      report("INCONCLUSIVE","Changed-copy ownership uncertain; close game");return 19;
    }
    prepared=first_item_changed_only(copy,desired,L"tshirt_ss_team_016_orl");
  }
  if(!prepared)rebuilt_result.reason="Top not eligible or single-item edit unverified; no rebuild";
#endif
  if(prepared)rebuilt_result=same_outfit_rebuild::run(base,record,manager,player,clone,source,anti_cheat,
    [&](const char *state,const char *reason){rebuilt_result.submitted=true;rebuilt_result.witnessed=true;return report(state,reason);},&desired);
  if(rebuilt_result.submitted) {
    // Clone ownership transferred to STOCK request destructor; no manual disposal.
    const bool ready=rebuilt_result.drained&&rebuilt_result.replaced&&rebuilt_result.bindings&&rebuilt_result.clothing;
    report(ready?"REBUILT":"INCONCLUSIVE",rebuilt_result.reason);
    return ready?0:17;
  }
  using RebuildDestroy=void *(*)(void *,unsigned);
  reinterpret_cast<RebuildDestroy>(base+0x81DDD0)(reinterpret_cast<void *>(clone),1);++deletes;
  source_ok=stable()&&capture(source.address,base,after)&&unchanged(source,after);
  report("FAILED",rebuilt_result.reason);return 18;
#endif
#ifdef NBA2K27_DESCRIPTION_CLONE_EDIT
  if(equal&&stable()&&capture(source.address,base,after)&&unchanged(source,after)) {
    using SetItem=void (*)(void *,const wchar_t *,int);
    const auto set_item=reinterpret_cast<SetItem>(base+0x7AAA70);
    constexpr auto marker=L"CODEX_TEMP_COPY_ONLY";
    report("EDITING_COPY","Temporary private copy only; marker is never loaded as an asset");
    ++edits;set_item(reinterpret_cast<void *>(copy.owner),marker,0);
    Snapshot edited{};
    if(!capture(clone,base,edited)||!independent(source,edited)) {
      report("INCONCLUSIVE","Edited-copy ownership uncertain; no further edits/disposal, close game");return 14;
    }
    edit_ok=first_item_changed_only(copy,edited,marker)&&stable()&&
      capture(source.address,base,after)&&unchanged(source,after);
    // Use saved string bytes, never the pointer the setter just freed.
    ++edits;set_item(reinterpret_cast<void *>(edited.owner),copy.names[0].data(),0);
    Snapshot restored_copy{};
    if(!capture(clone,base,restored_copy)||!independent(source,restored_copy)) {
      report("INCONCLUSIVE","Restored-copy ownership uncertain; no speculative disposal, close game");return 15;
    }
    restore_ok=first_item_changed_only(copy,restored_copy,copy.names[0].data())&&
      restored_copy.names==copy.names;
  }
#endif
  // Dispose a verified independent result even if names differ. Never free its
  // exposed strings/array separately; its destructor owns those allocations.
  report("DISPOSING","Calling native destructor on temporary clone only");
  using Destroy=void *(*)(void *,unsigned);
  reinterpret_cast<Destroy>(base+0x81DDD0)(reinterpret_cast<void *>(clone),1);++deletes;
  source_ok=stable()&&capture(source.address,base,after)&&unchanged(source,after);
#ifdef NBA2K27_DESCRIPTION_CLONE_EDIT
  const bool passed=equal&&source_ok&&edit_ok&&restore_ok&&edits==2&&deletes==1;
  report(passed?"PASS":"INCONCLUSIVE",passed?
    "Temporary copy edited, restored, disposed; live source unchanged":
    "Copy disposed but edit/restore/source evidence incomplete; close game");
  return passed?0:16;
#elif defined(NBA2K27_EXPAND_COPY)
  const bool passed=expanded&&source_ok&&releases==1&&inits==1&&setters==4&&deletes==1;
  report(passed?"PASS":"INCONCLUSIVE",passed?"Private copy expanded and disposed; live source unchanged":"Expansion comparison incomplete; close game");
  return passed?0:24;
#else
  report(equal&&source_ok?"PASS":"INCONCLUSIVE",equal&&source_ok?
    "Independent clothing copy verified and disposed; original description unchanged":
    "Clone disposed but comparison or source stability failed; close game");
  return equal&&source_ok?0:12;
#endif
}
#ifdef NBA2K27_SAVED_OUTFIT_PLAN
inline DWORD saved_match_worker(bool (*anti_cheat)(),unsigned match_number) {
  wchar_t exe[32768]{};GetModuleFileNameW(nullptr,exe,32768);
  const auto leaf=wcsrchr(exe,L'\\');if(!leaf||_wcsicmp(leaf+1,L"NBA2K27.exe"))return 0;
  const auto pid=GetCurrentProcessId();
  const auto base=reinterpret_cast<std::uintptr_t>(GetModuleHandleW(nullptr));
  auto directory=std::wstring(exe,leaf-exe+1);
#ifdef NBA2K27_SESSION_OUTFITS
  directory=session_diagnostics_directory();if(directory.empty())return 1;
#endif
  const auto prefix=directory+L"nba2k27_saved_outfits_"+std::to_wstring(pid)+
    (match_number>1?L"_match"+std::to_wstring(match_number):L"");
  unsigned complete=0;
  auto log=[&](const char *state,const char *reason){char text[512]{};std::snprintf(text,sizeof(text),
    "SCHEMA=blacktop-saved-outfits-v1\r\nPID=%lu\r\nSTATE=%s\r\nREASON=%s\r\nPLAYERS_REBUILT=%u\r\nGAME_CODE_HOOKS=0\r\nOBJECT_HOOKS=0\r\n",pid,state,reason,complete);return mem::write_log(prefix+L".log",text);};
  if(!mem::exact_build(base)||anti_cheat()){log("FAILED","Build or anti-cheat guard");return 1;}
  log("ARMED","Select outfits in launcher, then pause actual match and submit saved choices");
  bool trigger=false;
#ifdef NBA2K27_SESSION_OUTFITS
  for(;;){
#else
  for(unsigned n=0;n<1800;++n){
#endif
    if(anti_cheat()){log("FAILED","Anti-cheat guard while waiting");return 1;}
    HANDLE f=CreateFileW((prefix+L".trigger").c_str(),GENERIC_READ,FILE_SHARE_READ,nullptr,OPEN_EXISTING,0,nullptr);
    if(f!=INVALID_HANDLE_VALUE){char b[32]{};DWORD got=0;const bool ok=ReadFile(f,b,32,&got,nullptr)!=FALSE;CloseHandle(f);
      trigger=ok&&saved_trigger_matches(b,got);break;}
    Sleep(500);
  }
  if(!trigger||!DeleteFileW((prefix+L".trigger").c_str())){log("FAILED","Missing trigger");return 2;}
  std::array<unsigned char,kSavedMaxBytes+1> bytes{};DWORD got=0;
  HANDLE f=CreateFileW((prefix+L".plan").c_str(),GENERIC_READ,FILE_SHARE_READ,nullptr,OPEN_EXISTING,0,nullptr);
  if(f==INVALID_HANDLE_VALUE){log("FAILED","Saved plan missing");return 3;}
  const bool read=ReadFile(f,bytes.data(),static_cast<DWORD>(bytes.size()),&got,nullptr)!=FALSE;CloseHandle(f);
  FILETIME created{},exit{},kernel{},user{};SavedPlan plan{};
  if(!read||!GetProcessTimes(GetCurrentProcess(),&created,&exit,&kernel,&user)||
      !parse_saved_plan(bytes.data(),got,pid,(std::uint64_t(created.dwHighDateTime)<<32)|created.dwLowDateTime,plan)) {
    log("FAILED","Invalid or stale saved plan");return 4;
  }
  // All choices must resolve uniquely before any rebuild is attempted.
  for(unsigned i=0;i<plan.count;++i){unsigned matches=0;
    for(unsigned j=0;j<30;++j){const auto r=base+0xAAC79B0+j*0x50;
      if(mem::q(r)==base+0x75BD1F8&&mem::q(mem::q(r+0x30)+0x500)==plan.choices[i].roster)++matches;}
    if(matches!=1){log("FAILED","Saved player missing or ambiguous in match");return 5;}
  }
  for(unsigned i=0;i<plan.count;++i){
    log("RUNNING","Rebuilding next saved player; stay paused");
    if(worker_once(anti_cheat,&plan.choices[i],i)!=0){log("INCONCLUSIVE","Stopped at player report; no retry or forced rollback");return 6;}
    ++complete;
  }
  log("REBUILT","Saved outfits rebuilt sequentially; verify each player and gameplay");return 0;
}
inline DWORD worker(bool (*anti_cheat)()) {
  unsigned match_number=1;
  for(;;){
    const DWORD result=saved_match_worker(anti_cheat,match_number);
    if(result)return result; // Never retry uncertain ownership or failed work.
#ifndef NBA2K27_SESSION_OUTFITS
    return 0;
#else
    const auto base=reinterpret_cast<std::uintptr_t>(GetModuleHandleW(nullptr));
    unsigned selection_samples=0;
    while(selection_samples<4){
      if(anti_cheat())return 1;
      const bool selection=mem::q(base+0xAEB29D0)==base+0x78AABA8&&
        mem::i(base+0xAEB29D0+0xEA8)==3&&
        mem::i(mem::q(base+0xAAC79A0)+0xA04)==6;
      selection_samples=selection?selection_samples+1:0;
      Sleep(500);
    }
    ++match_number;
#endif
  }
}
#endif
} // namespace character_description_clone_audit
