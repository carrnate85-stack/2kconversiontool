#pragma once
// Opted-in editor Save commands only. Including this header starts nothing.
// Existing game-owned version proxy hosts a bounded command watcher; no remote
// thread injection, game function calls, executable allocation or code hooks.
#include "outfit_match_v2_proxy_bridge.hpp"
#include "roster_appearance_v2_win.hpp"
#include <memory>
#include <atomic>
namespace roster_appearance_v2::bridge {
using outfit_match_v2::Handle;
using outfit_match_v2::StableId;
inline std::atomic<bool> started{false};
inline std::wstring wide(const Id& id){return {id.begin(),id.end()};}
inline std::wstring slot(const outfit_match_v2::bridge::Config& c,const wchar_t* suffix){return c.root+L"\\Data\\ap2-"+std::wstring(c.session.begin(),c.session.end())+suffix;}
inline std::wstring phase_path(const outfit_match_v2::bridge::Config& c,const Request& r){const wchar_t* suffix=r.operation==1?L".intent":r.operation==2?L".undo":L".verified";return c.root+L"\\Data\\ap2-"+wide(r.id)+suffix;}
inline bool reply_file(HANDLE file,const outfit_match_v2::bridge::Config& c,const Request& r) {
 using namespace outfit_match_v2;
 StableId id{};FILE_STANDARD_INFO info{};std::array<wchar_t,1024> name{};
 const DWORD length=GetFinalPathNameByHandleW(file,name.data(),DWORD(name.size()),FILE_NAME_NORMALIZED|VOLUME_NAME_DOS);
 if(!file_id(file,false,id)||id.volume!=r.reply_volume||id.index!=r.reply_index||!GetFileInformationByHandleEx(file,FileStandardInfo,&info,sizeof info)||info.EndOfFile.QuadPart!=0||!length||length>=name.size())return false;
 std::wstring actual(name.data(),length);if(actual.starts_with(L"\\\\?\\"))actual.erase(0,4);
 return equal_path(actual,slot(c,L".reply"));
}
inline bool bytes(HANDLE file,const void* data,std::size_t size) {
 DWORD wrote{};if(size>65536||!WriteFile(file,data,DWORD(size),&wrote,nullptr)||wrote!=size||!FlushFileBuffers(file))return false;
 return true;
}
class Ledger {
 Handle file_;StableId identity_{};std::uint32_t sequence_{};
public:
 bool append(const outfit_match_v2::bridge::Config& c,const Request& r,Phase phase) {
  using namespace outfit_match_v2;
  if(sequence_>=512)return false;
  if(!file_){file_=Handle(CreateFileW(slot(c,L".native").c_str(),GENERIC_READ|GENERIC_WRITE,FILE_SHARE_READ,nullptr,CREATE_NEW,FILE_FLAG_OPEN_REPARSE_POINT|FILE_FLAG_WRITE_THROUGH,nullptr));if(!file_||!file_id(file_.value,false,identity_))return false;}
  StableId before{};FILE_STANDARD_INFO info{};
  if(!file_id(file_.value,false,before)||before!=identity_||!GetFileInformationByHandleEx(file_.value,FileStandardInfo,&info,sizeof info)||info.EndOfFile.QuadPart!=std::int64_t(sequence_)*160)return false;
  std::array<unsigned char,160> frame{};std::memcpy(frame.data(),"C27APN01",8);const auto next=sequence_+1,stage=std::uint32_t(phase);std::memcpy(frame.data()+8,&next,4);std::memcpy(frame.data()+12,&stage,4);std::memcpy(frame.data()+16,r.id.data(),48);
  Sha request{};if(!win_detail::hash_bytes(&r,sizeof r,request))return false;std::memcpy(frame.data()+64,request.data(),32);std::memcpy(frame.data()+96,&r.pid,4);std::memcpy(frame.data()+100,&r.birth,8);std::memcpy(frame.data()+108,r.before.data(),4);std::memcpy(frame.data()+112,r.after.data(),4);
  Sha checksum{};if(!win_detail::hash_bytes(frame.data(),128,checksum))return false;std::memcpy(frame.data()+128,checksum.data(),32);
  LARGE_INTEGER at{};at.QuadPart=std::int64_t(sequence_)*160;if(!SetFilePointerEx(file_.value,at,nullptr,FILE_BEGIN)||!bytes(file_.value,frame.data(),frame.size())||!SetFilePointerEx(file_.value,at,nullptr,FILE_BEGIN))return false;
  std::array<unsigned char,160> check{};DWORD read{};StableId after{};
  if(!ReadFile(file_.value,check.data(),DWORD(check.size()),&read,nullptr)||read!=check.size()||check!=frame||!file_id(file_.value,false,after)||after!=identity_)return false;
  ++sequence_;return true;
 }
};
class Host final:public roster_appearance_v2::Host {
 outfit_match_v2::bridge::Config config_;outfit_match_v2::bridge::Session& binding_;
 outfit_match_v2::Authority& authority_;Ledger& ledger_;Handle cancel_,journal_;StableId journal_id_{};
 Request request_{};NativeScope native_;outfit_match_v2::PinnedDirectory directories_;
 bool bound(const Request& r) const {const auto& b=authority_.binding();return r.pid==b.game_pid&&r.birth==b.game_birth&&std::string(r.session.begin(),r.session.end())==b.session;}
public:
 Host(const outfit_match_v2::bridge::Config& c,outfit_match_v2::bridge::Session& b,outfit_match_v2::Authority& a,Ledger& l,const Request& r):config_(c),binding_(b),authority_(a),ledger_(l),request_(r),native_(a.base(),[this](bool forward){return authority(forward);}) {
  const auto name=L"Local\\NBA2K27AppearanceV2-"+wide(r.session)+L"-"+wide(r.id)+L"-Cancel";
  cancel_=Handle(OpenEventW(SYNCHRONIZE|READ_CONTROL,FALSE,name.c_str()));
  directories_.open(c.root+L"\\Data");
 }
 bool authority(bool forward) override {
  if(!bound(request_)||!directories_.unchanged())return false;
  if(forward)return cancel_&&outfit_match_v2::bridge::private_event(cancel_.value)&&WaitForSingleObject(cancel_.value,0)==WAIT_TIMEOUT&&authority_.safe();
  return binding_.retained_appearance();
 }
 bool journal(const Request& r) override {
  using namespace outfit_match_v2;
  if(!bound(r)||!directories_.unchanged())return false;
  if(!journal_){journal_=Handle(CreateFileW(phase_path(config_,r).c_str(),GENERIC_READ,FILE_SHARE_READ|FILE_SHARE_DELETE,nullptr,OPEN_EXISTING,FILE_FLAG_OPEN_REPARSE_POINT,nullptr));if(!journal_||!file_id(journal_.value,false,journal_id_))return false;}
  std::vector<unsigned char> body;StableId now{};if(!file_id(journal_.value,false,now)||now!=journal_id_||!read_file(journal_.value,65536,body)||body.size()<240||!win_detail::same_hash(body,r.journal))return false;
  std::uint32_t pid{},index{},operation{},reserved{};std::uint64_t birth{};std::memcpy(&pid,body.data()+104,4);std::memcpy(&birth,body.data()+108,8);std::memcpy(&index,body.data()+116,4);std::memcpy(&operation,body.data()+120,4);std::memcpy(&reserved,body.data()+124,4);
  return !std::memcmp(body.data()+128,r.before.data(),4)&&!std::memcmp(body.data()+132,r.after.data(),4)&&!std::memcmp(body.data()+136,r.mask.data(),4)&&!std::memcmp(body.data()+144,r.row.data(),32)&&!std::memcmp(body.data()+176,r.pointers.data(),32)&&!std::memcmp(body.data()+208,r.metadata.data(),32)&&!std::memcmp(body.data(),"C27APJ01",8)&&!std::memcmp(body.data()+8,r.id.data(),48)&&!std::memcmp(body.data()+56,r.session.data(),48)&&pid==r.pid&&birth==r.birth&&index==r.index&&operation==r.operation&&!reserved&&body[140]==0&&body[141]==0&&body[142]==0&&body[143]==0&&directories_.unchanged();
 }
 bool scope(const Request& r,bool after) override {return bound(r)&&native_.check(r,after);}
 bool record(const Request& r,Phase phase) override {return directories_.unchanged()&&ledger_.append(config_,r,phase)&&directories_.unchanged();}
 Atomic compare(const Request& r,bool undo) override {return bound(r)?native_.compare(r,undo):Atomic::conflict;}
 void unpin_journal(){journal_.reset();}
};
struct Start {outfit_match_v2::bridge::Config config;HMODULE module{};bool(*anti_cheat)(){};};
inline DWORD run(Start* start) {
 using namespace outfit_match_v2;
 auto game=retain_current_process();if(!game)return 1;
 PinnedDirectory data;if(!data.open(start->config.root+L"\\Data"))return 2;
 outfit_match_v2::bridge::Session binding;Authority authority;bool opened=false,failed=false;
 roster_appearance_v2::Session session;Ledger ledger;
 StableId last{};Sha last_hash{};bool seen=false;
 while(WaitForSingleObject(game.value,200)!=WAIT_OBJECT_0) {
  if(failed||!data.unchanged())continue;
  try {
   const auto name=slot(start->config,L".request");
   Handle file(CreateFileW(name.c_str(),GENERIC_READ,FILE_SHARE_READ|FILE_SHARE_DELETE,nullptr,OPEN_EXISTING,FILE_FLAG_OPEN_REPARSE_POINT,nullptr));
   if(!file)continue;StableId id{};std::vector<unsigned char> raw;
   if(!file_id(file.value,false,id)||!read_file(file.value,sizeof(Request),raw)||raw.size()!=sizeof(Request)){failed=true;continue;}
   Sha hash{};if(!win_detail::hash_bytes(raw.data(),raw.size(),hash)){failed=true;continue;}
   if(seen&&id==last&&hash==last_hash)continue;
   last=id;last_hash=hash;seen=true;
   Request request{};std::memcpy(&request,raw.data(),sizeof request);if(!valid(request)){failed=true;continue;}
   if(!opened){if(!binding.open(start->config,start->anti_cheat)||!authority.open(binding.native_binding())){failed=true;continue;}opened=true;}
   if(request.pid!=authority.binding().game_pid||request.birth!=authority.binding().game_birth||std::string(request.session.begin(),request.session.end())!=authority.binding().session){failed=true;continue;}
   // The ROOT caller created/holds this exclusive file. Duplicate its exact
   // file object from the authenticated lease parent/worker, not a path we or
   // another process created. Passive result bytes can never mint ownership.
   HANDLE copied{};
   if(!binding.duplicate_appearance_reply(request.reply_owner,request.reply_handle,copied)){failed=true;continue;}
   Handle output(copied);StableId output_id{};
   const auto ready_name=L"Local\\NBA2K27AppearanceV2-"+wide(request.session)+L"-"+wide(request.id)+L"-Ready";
   Handle ready(OpenEventW(EVENT_MODIFY_STATE|READ_CONTROL,FALSE,ready_name.c_str()));
   if(!output||!reply_file(output.value,start->config,request)||!file_id(output.value,false,output_id)||!ready||!outfit_match_v2::bridge::private_event(ready.value)){failed=true;continue;}
   Host host(start->config,binding,authority,ledger,request);
   const auto result=session.execute(request,host);
   host.unpin_journal(); // The client may publish its next immutable phase now.
   Reply response{};response.magic={'C','2','7','A','P','S','0','1'};response.session=request.session;response.id=request.id;response.request=hash;response.outcome=std::uint32_t(result);
   StableId after{};if(!bytes(output.value,&response,sizeof response)||!file_id(output.value,false,after)||after!=output_id){failed=true;continue;}
   // Closing both duplicates precedes Ready. Only then may the caller unlink
   // its exact request/reply handles and publish another command.
   output.reset();file.reset();if(!SetEvent(ready.value))failed=true;
  } catch(...) {failed=true;}
 }
 return failed?3:0;
}
inline DWORD WINAPI worker(void* raw) {
 std::unique_ptr<Start> start(static_cast<Start*>(raw));const auto module=start->module;
 DWORD code{};try{code=run(start.get());}catch(...){code=4;}
 start.reset();FreeLibraryAndExitThread(module,code);
}
inline bool start(HMODULE module,bool(*anti_cheat)()) {
 const auto& c=outfit_match_v2::bridge::config;
 if(!outfit_match_v2::bridge::configured||!c.enabled||!outfit_match_v2::bridge::session_nonce(c.session)||started.exchange(true))return false;
 HMODULE retained{};if(!GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS,reinterpret_cast<LPCWSTR>(&worker),&retained)||retained!=module){if(retained)FreeLibrary(retained);return false;}
 auto context=std::make_unique<Start>();context->config=c;context->module=retained;context->anti_cheat=anti_cheat;
 Handle thread(CreateThread(nullptr,0,&worker,context.get(),0,nullptr));if(!thread){FreeLibrary(retained);return false;}context.release();return true;
}
} // namespace roster_appearance_v2::bridge
