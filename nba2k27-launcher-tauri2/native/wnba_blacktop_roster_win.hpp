#pragma once
#include "wnba_blacktop_category_win.hpp"
#include "wnba_blacktop_roster_core.hpp"
#include "wnba_blacktop_roster_profile.hpp"
#include <sstream>
#include "wnba_blacktop_roster_telemetry.hpp"

namespace wnba_blacktop_roster {
// Only a native InstallDelegate invocation executes the existing general
// loader. This host never calls that function or changes any instruction.
template<class CategoryHost> class NativeHost {
 CategoryHost* category_{};Binding binding_;std::uintptr_t base_{};
 bool owned_{},uncertain_{};Endpoint pending_from_{Endpoint::unknown},pending_to_{Endpoint::unknown};
 std::string reason_="Blacktop roster callback has not opened";
 mutable std::string profile_failure_;
 std::string last_telemetry_="ret=0 ctx=? d=?/?/? uid=?";
 static std::string hex(std::uintptr_t value) {
  std::ostringstream out;out<<"0x"<<std::hex<<value;return out.str();
 }
 // Fixed image metadata only, captured at the refusal instead of adopting a
 // later, initialized delegate. No pointer traversal or arbitrary data dump.
 std::string admission_metadata()const {
  namespace p=wnba_blacktop_roster_profile;
  if(!base_ || base_!=reinterpret_cast<std::uintptr_t>(GetModuleHandleW(nullptr)))return "";
  MEMORY_BASIC_INFORMATION manager{},delegate{};
  const bool mq=VirtualQuery(reinterpret_cast<void*>(base_+p::kManager),&manager,sizeof manager)==sizeof manager;
  const bool dq=VirtualQuery(reinterpret_cast<void*>(base_+p::kDelegate),&delegate,sizeof delegate)==sizeof delegate;
  std::uintptr_t tag{};
  const bool read=outfit_match_v2::value(base_+p::kDelegate,tag);
  const bool in_image=read && tag>=base_ && tag<base_+0x36532000;
  return " tag="+(read?(tag==0?std::string("zero"):(in_image?hex(tag-base_):std::string("outside-image"))):std::string("unreadable"))+
   " mgr="+(mq?hex(manager.Type)+"/"+hex(manager.Protect):std::string("query-failed"))+
   " del="+(dq?hex(delegate.Type)+"/"+hex(delegate.Protect):std::string("query-failed"));
 }
 bool fail(const std::string& reason){reason_=reason+admission_metadata();return false;}
 bool profile_refuse(const std::string& reason)const {profile_failure_=reason;return false;}
 static std::uintptr_t endpoint_rva(Endpoint e) {
  namespace p=wnba_blacktop_roster_profile;
  return e==Endpoint::stock?p::kStock:e==Endpoint::general_loader?p::kGeneral:0;
 }
 bool native_profile()const {
  using namespace outfit_match_v2;
  namespace p=wnba_blacktop_roster_profile;
  profile_failure_.clear();
  for(const auto& method:p::kMethods) {
   std::vector<unsigned char> data(method.bytes);
   if(!wnba_blacktop_category::win_detail::image_span(base_,base_+method.rva,method.bytes,PAGE_EXECUTE_READ))
    return profile_refuse("method image span "+hex(method.rva));
   if(!read_memory(base_+method.rva,data.data(),data.size()))return profile_refuse("method read "+hex(method.rva));
   if(digest(data)!=method.hash)return profile_refuse("complete method hash "+hex(method.rva));
  }
  std::array<unsigned char,sizeof(p::kAsset)> asset{};
  if(!wnba_blacktop_category::win_detail::image_span(base_,base_+p::kAssetRva,asset.size(),PAGE_READONLY) ||
     !read_memory(base_+p::kAssetRva,asset.data(),asset.size()))return profile_refuse("asset string span/read");
  for(std::size_t i=0;i<asset.size();++i)if(asset[i]!=std::uint8_t(p::kAsset[i]))return profile_refuse("asset string bytes");
  MEMORY_BASIC_INFORMATION manager{},delegate{};
  if(VirtualQuery(reinterpret_cast<void*>(base_+p::kManager),&manager,sizeof manager)!=sizeof manager ||
     manager.State!=MEM_COMMIT || manager.Type!=MEM_IMAGE || manager.AllocationBase!=reinterpret_cast<void*>(base_) ||
     manager.Protect!=PAGE_READWRITE ||
     !wnba_blacktop_category::win_detail::image_span(base_,base_+p::kInstalledDelegateRva,8,PAGE_READWRITE) ||
     VirtualQuery(reinterpret_cast<void*>(base_+p::kDelegate),&delegate,sizeof delegate)!=sizeof delegate ||
     !p::delegate_image_metadata(delegate.Type,delegate.Protect,delegate.State==MEM_COMMIT,
      delegate.AllocationBase==reinterpret_cast<void*>(base_)))return profile_refuse("manager/delegate image regions");
  std::uintptr_t tag{};
  if(!value(base_+p::kDelegate,tag))return profile_refuse("delegate class tag unreadable");
  if(tag!=base_+p::kVtable)return profile_refuse("delegate class tag differs");
  return true;
 }
 bool table(Endpoint& endpoint,DWORD protection=PAGE_READONLY)const {
  using namespace outfit_match_v2;
  namespace p=wnba_blacktop_roster_profile;
  if(!base_ || uncertain_ ||
     !wnba_blacktop_category::win_detail::image_span(base_,base_+p::kVtable,p::kVtablePrefixBytes,protection))return false;
  std::vector<unsigned char> raw(p::kVtablePrefixBytes),again(raw.size());
  if(!read_memory(base_+p::kVtable,raw.data(),raw.size()) ||
     !read_memory(base_+p::kVtable,again.data(),again.size()) || raw!=again)return false;
  std::uintptr_t callback{};std::memcpy(&callback,raw.data()+p::kOffset,8);
  endpoint=callback==base_+p::kStock?Endpoint::stock:
   callback==base_+p::kGeneral?Endpoint::general_loader:Endpoint::foreign;
  // Never normalize a General value seen before our successful CAS.
  if(endpoint==Endpoint::general_loader && !owned_)return false;
  if(endpoint==Endpoint::foreign)return false;
  for(std::size_t offset=0;offset<raw.size();offset+=8) {
   std::uintptr_t pointer{};std::memcpy(&pointer,raw.data()+offset,8);
   if(!pointer || pointer<base_ || pointer>=base_+0x36532000)return false;
   auto rva=pointer-base_;
   if(offset==p::kOffset && rva==p::kGeneral)rva=p::kStock;
   std::memcpy(raw.data()+offset,&rva,8);
  }
  return digest(raw)==p::kNormalizedVtableSha256;
 }
 bool menu(outfit_match_v2::Context& snapshot)const {
  using namespace outfit_match_v2;
  Context a{},b{};
  if(!context(base_,a) || !context(base_,b) || a!=b)return false;
  Input input{};input.stable=true;input.current=a.mode;input.alternate=a.other;
  input.stage=a.stage;input.active=a.active;input.kind=a.kind;
  if(!launch_menu(input))return false;
  snapshot=a;return true;
 }
 Telemetry telemetry()const {
  Telemetry value;
  if(!category_ || !category_->roster_checkpoint(binding_,false))return value;
  value.retained=true;
  outfit_match_v2::Context first{},second{};
  const bool read_first=outfit_match_v2::context(base_,first);
  const bool read_second=outfit_match_v2::context(base_,second);
  value.context_read=read_first;value.stable=read_first && read_second && first==second;
  if(read_first) {value.current=first.mode;value.other=first.other;value.stage=first.stage;value.active=first.active;value.kind=first.kind;}
  auto image=[&](std::uintptr_t rva) {
   std::uintptr_t pointer{};
   const bool read=outfit_match_v2::value(base_+rva,pointer);
   return ImageMetadata::from(read,pointer,base_);
  };
  namespace p=wnba_blacktop_roster_profile;
  value.installed=image(p::kInstalledDelegateRva);
  value.requested=image(p::kRequestedDelegateRva);
  value.playnow=image(p::kPlayNowDelegateRva);
  value.uid_read=outfit_match_v2::value(base_+0xAB2F1B0,value.uid);
  if(!category_->roster_checkpoint(binding_,false))return {};
  return value;
 }
 bool record(const char* action,Endpoint from,Endpoint to) {
  if(!category_)return false;
  last_telemetry_=format_telemetry(telemetry());
  return category_->roster_record(binding_,std::string("ACTION=")+action+
   "\nCALLBACK_RVA="+std::to_string(wnba_blacktop_roster_profile::kSlot)+
   "\nBEFORE_RVA="+std::to_string(endpoint_rva(from))+"\nAFTER_RVA="+std::to_string(endpoint_rva(to))+
   "\nEFFECTS=exact-aligned-callback-CAS;readback;restore-original-RO-protection"+
   "\nREADONLY_TELEMETRY="+last_telemetry_);
 }
public:
 bool open(CategoryHost& category) {
  if(category_)return false;
  category_=&category;binding_=category.binding();base_=category.game_base();
  Endpoint initial{};
  if(!binding_.valid())return fail("Blacktop roster admission invalid session binding");
  if(!category.roster_checkpoint(binding_,true))return fail(category.reason());
  if(!native_profile())return fail("Blacktop roster profile: "+profile_failure_);
  if(!table(initial))return fail("Blacktop roster original table proof differs");
  if(initial!=Endpoint::stock)return fail("Blacktop roster original endpoint is not stock");
  reason_="Blacktop native roster callback opened";return true;
 }
 const char* reason()const {return reason_.c_str();}
 std::uint64_t monotonic_ms()const {return GetTickCount64();}
 bool publication_budget()const {return category_ && category_->roster_publication_budget();}
 std::string event_detail(const char* reason)const {return std::string(reason)+" "+last_telemetry_;}
 bool transition_ready()const {
  if(!category_ || !category_->roster_checkpoint(binding_,true))return false;
  namespace p=wnba_blacktop_roster_profile;
  std::uintptr_t first{},second{};
  return outfit_match_v2::value(base_+p::kInstalledDelegateRva,first) &&
   outfit_match_v2::value(base_+p::kInstalledDelegateRva,second) && first==second && first!=base_+p::kDelegate;
 }
 bool checkpoint(const Binding& binding,bool publishing) {
  if(!category_ || binding!=binding_ || uncertain_ ||
     !category_->roster_checkpoint(binding,publishing) || !native_profile())return false;
  Endpoint endpoint{};if(!table(endpoint))return false;
  if(!publishing)return true;
  outfit_match_v2::Context snapshot{};
  return menu(snapshot) && transition_ready();
 }
 bool read_endpoint(Endpoint& endpoint)const {return table(endpoint);}
 bool journal_intent(const Binding& binding,Endpoint from,Endpoint to) {
  if(binding!=binding_ || pending_from_!=Endpoint::unknown ||
     !((from==Endpoint::stock && (to==Endpoint::stock || to==Endpoint::general_loader)) ||
       (from==Endpoint::general_loader && to==Endpoint::stock)))return false;
  if(!record("ROSTER_CALLBACK_INTENT",from,to))return false;
  pending_from_=from;pending_to_=to;return true;
 }
 bool write_endpoint(Endpoint from,Endpoint to) {
  using namespace outfit_match_v2;
  namespace p=wnba_blacktop_roster_profile;
  const bool publishing=from==Endpoint::stock && to==Endpoint::general_loader;
  if(pending_from_!=from || pending_to_!=to || from==to ||
     !checkpoint(binding_,publishing))return false;
  Endpoint before{};if(!table(before) || before!=from)return false;
  Context boundary{};if(publishing && !menu(boundary))return false;
  const auto original=base_+endpoint_rva(from),next=base_+endpoint_rva(to);
  std::vector<unsigned char> raw(p::kVtablePrefixBytes),normalized(raw.size());
  DWORD old{};
  if(!VirtualProtect(reinterpret_cast<void*>(base_+p::kSlot),8,PAGE_READWRITE,&old))return false;
  bool page_restored=false;
  auto restore_page=[&](void* address) noexcept {
   DWORD changed{};
   page_restored=VirtualProtect(address,8,old,&changed)!=FALSE && changed==PAGE_READWRITE && old==PAGE_READONLY;
   if(!page_restored)uncertain_=true;
  };
  // Hashing/readback can allocate. Every exception/return after VirtualProtect
  // must restore the exact old protection before unwinding into the hold.
  std::unique_ptr<void,decltype(restore_page)> protection(reinterpret_cast<void*>(base_+p::kSlot),restore_page);
  bool exchanged=false;
  if(old==PAGE_READONLY) {
   // Only this one page changed protection; the full pinned vtable may span
   // neighboring pages which remain RO. Re-read every pointer before CAS.
   bool valid=read_memory(base_+p::kVtable,raw.data(),raw.size());normalized=raw;
   for(std::size_t offset=0;valid && offset<raw.size();offset+=8) {
    std::uintptr_t pointer{};std::memcpy(&pointer,raw.data()+offset,8);
    if(!pointer || pointer<base_ || pointer>=base_+0x36532000){valid=false;break;}
    auto rva=pointer-base_;
    if(offset==p::kOffset) {
     if(pointer!=original){valid=false;break;}
     rva=p::kStock;
    }
    std::memcpy(normalized.data()+offset,&rva,8);
   }
   valid=valid && digest(normalized)==p::kNormalizedVtableSha256;
   if(valid && publishing) {
    Context immediately_before{};
    valid=category_->roster_checkpoint(binding_,true) && transition_ready() &&
     menu(immediately_before) && immediately_before==boundary;
   }
   if(valid) {
    const auto prior=InterlockedCompareExchange64(reinterpret_cast<volatile LONG64*>(base_+p::kSlot),LONG64(next),LONG64(original));
    if(std::uintptr_t(prior)==original) {
     owned_=to==Endpoint::general_loader;exchanged=true;
    } else {uncertain_=true;reason_="Blacktop roster callback CAS conflicted; no foreign value adopted";}
   }
  }
  protection.reset();
  Endpoint after{};
  const bool verified=page_restored && !uncertain_ && table(after) && after==to;
  // A normal native Home->Blacktop transition can start immediately after the
  // CAS. Do not misclassify a verified publication as failure because it did
  // its job; the controller owns subsequent transition/restoration handling.
  return exchanged && verified;
 }
 bool journal_applied(const Binding& binding,Endpoint from,Endpoint to) {
  if(binding!=binding_ || pending_from_!=from || pending_to_!=to)return false;
  if(!record("ROSTER_CALLBACK_APPLIED",from,to))return false;
  if(from==Endpoint::stock && to==Endpoint::stock)owned_=false;
  pending_from_=pending_to_=Endpoint::unknown;return true;
 }
};
} // namespace wnba_blacktop_roster
