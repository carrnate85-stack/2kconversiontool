#pragma once
// Explicit manual-trial executor. Including this file starts no worker,
// allocates no executable memory, and makes no game call. Production exposure
// requires the independent CHAININFO/unwind fixture and manual category gate.
#include "outfit_match_v2_native.hpp"
#include "wnba_blacktop_category_core.hpp"
#include "wnba_blacktop_bridge_unwind.hpp"
#include "wnba_blacktop_label_profile.hpp"
#include <optional>
#include <cstdio>
namespace wnba_blacktop_category {
namespace win_detail {
struct Method { std::uintptr_t rva; std::size_t bytes; const char* hash; };
inline constexpr Method methods[]={
 {0x230BE80,251,"C8B6AA7952EABD418A8AD788E661728A5F223790031B7BF84DE6EF57F6CF7A92"},
 {0x27E7E60,64,"131C597236431B0419EB704D8BF2FBE4C0B11D2963C66F420EE187B5976317C5"},
 {0x27E7EA0,221,"8FF33BEC3FE5BDD395F25D50151232E618FA138EA9D569D824DBAE13992B94D4"},
 {0x27E79B0,204,"59C7E5904DA647E49F67E95F81BEBBA56BEB1F2EC19554EE260AE2D42D29CC72"},
 {0x27E7BC0,321,"525402F2DC58A69F65DE958974D8744CF68FD478DAF0AB9E65547452A116F3F2"},
 {0x230C3C0,851,"4343DDF7CC2E1696A37ABCA23046E0A31A94BBEFDF0474AE70CD79E6FC4288FF"},
 {0x31154E0,72936,"39372B4A0CDFDB668551C41C3DB3C576CE97465574BFB333DB83274E137B8B5C"},
};
inline constexpr std::array<unsigned char,24> stock_table={
 0x7C,0x5F,0xEE,0xFA,0x8A,0x5F,0xEE,0xFA,0x83,0x5F,0xEE,0xFA,
 0x08,0x60,0xEE,0xFA,0xDE,0x5F,0xEE,0xFA,0xE7,0x5F,0xEE,0xFA};
inline bool image_span(std::uintptr_t base,std::uintptr_t at,std::size_t bytes,DWORD protection) {
 if(!bytes || at<base || at>base+0x36532000 || bytes>base+0x36532000-at)return false;
 MEMORY_BASIC_INFORMATION m{};
 return VirtualQuery(reinterpret_cast<void*>(at),&m,sizeof m)==sizeof m &&
  m.State==MEM_COMMIT && m.Type==MEM_IMAGE &&
  reinterpret_cast<std::uintptr_t>(m.AllocationBase)==base && m.Protect==protection &&
  reinterpret_cast<std::uintptr_t>(m.BaseAddress)<=at && m.RegionSize>=bytes &&
  at-reinterpret_cast<std::uintptr_t>(m.BaseAddress)<=m.RegionSize-bytes;
}
inline bool private_range(std::uintptr_t at,std::size_t bytes,std::uintptr_t expected_allocation=0) {
 if(at<0x10000 || !bytes || bytes>0x300000 || at>0x7FFF00000000ull-bytes)return false;
 const auto end=at+bytes;auto cursor=at;auto allocation=expected_allocation;
 for(unsigned count=0;cursor<end && count<1024;++count) {
  MEMORY_BASIC_INFORMATION m{};
  if(VirtualQuery(reinterpret_cast<const void*>(cursor),&m,sizeof m)!=sizeof m ||
     m.State!=MEM_COMMIT || m.Type!=MEM_PRIVATE ||
     (m.Protect!=PAGE_READONLY && m.Protect!=PAGE_READWRITE && m.Protect!=PAGE_WRITECOPY))return false;
  const auto current=reinterpret_cast<std::uintptr_t>(m.AllocationBase);
  if(!allocation)allocation=current;
  if(!allocation || current!=allocation)return false;
  const auto start=reinterpret_cast<std::uintptr_t>(m.BaseAddress);
  if(start>cursor || !m.RegionSize || start>0x7FFF00000000ull-m.RegionSize || start+m.RegionSize<=cursor)return false;
  cursor=std::min(end,start+m.RegionSize);
 }
 return cursor==end;
}
inline bool chunked_read(std::uintptr_t at,void* output,std::size_t bytes) {
 if(!output || !bytes || bytes>0x300000 || at<0x10000 || at>0x7FFF00000000ull-bytes)return false;
 auto* target=static_cast<unsigned char*>(output);
 for(std::size_t done=0;done<bytes;) {
  const auto n=std::min<std::size_t>(65536,bytes-done);
  if(!outfit_match_v2::read_memory(at+done,target+done,n))return false;
  done+=n;
 }
 return true;
}
inline constexpr std::size_t kTeamStride=0x1550,kMixedTeams=403,kRosterBytes=kTeamStride*kMixedTeams;
template<class Read> bool bounded_roster_bytes(std::uintptr_t at,std::vector<unsigned char>& bytes,Read read) {
 if(at<0x10000||at>0x7FFF00000000ull-kRosterBytes)return false;bytes.resize(kRosterBytes);
 for(std::size_t done=0;done<bytes.size();) {
  const auto n=std::min<std::size_t>(65536,bytes.size()-done);
  if(!read(at+done,bytes.data()+done,n)){bytes.clear();return false;}done+=n;
 }
 return true;
}
inline bool decode_roster_fields(std::span<const unsigned char> bytes,std::size_t row,
 std::uint32_t& id,std::uint32_t& kind,std::array<wchar_t,24>& name) {
 if(bytes.size()!=kRosterBytes||row>=kMixedTeams)return false;
 const auto* start=bytes.data()+row*kTeamStride;
 std::memcpy(&id,start+0xF20,sizeof id);std::memcpy(&kind,start+0x123C,sizeof kind);
 std::memcpy(name.data(),start+0x2FA,sizeof name);return true;
}
struct Roster {
 outfit_match_v2::Context context;
 std::uintptr_t root{},owner{},table{},allocation{};
 std::uint32_t count{};
 struct Team {
  std::uint16_t id{};std::uint32_t kind{};std::array<wchar_t,24> name{};
  bool operator==(const Team&)const=default;
 };
 std::vector<Team> rows;
 bool operator==(const Roster&)const=default;
};
enum class CaptureFailure : unsigned { none,context,selection,root_span,owner_span,owner_query,
 owner_allocation,owner_range,table_count_or_range,row_fields,row_name,club_id,club_name,
 totals,ending_context,ending_context_changed,ending_root,ending_owner,ending_count,ending_table,
 ending_owner_range,ending_table_range };
struct CaptureDiagnostic {
 CaptureFailure failure{};unsigned row{~0u},count{},nba{},clubs{},context_difference{};
};
struct DeepDiagnostic {
 CaptureDiagnostic captures[2]{};
 bool first_ok{},second_attempted{},second_ok{},scope_attempted{},scope_ok{};
 bool departure_first{},departure_second{};
 unsigned difference{},context_difference{};
};
enum class ScopeFailure : unsigned { none,protection,mutation,identity,authority,consent,source,profile,route };
inline unsigned context_difference(const outfit_match_v2::Context& a,const outfit_match_v2::Context& b) {
 return (a.index!=b.index?1u:0u)|(a.alternate!=b.alternate?2u:0u)|(a.mode!=b.mode?4u:0u)|
  (a.other!=b.other?8u:0u)|(a.stage!=b.stage?16u:0u)|(a.kind!=b.kind?32u:0u)|
  (a.game_mode!=b.game_mode?64u:0u)|(a.active!=b.active?128u:0u)|
  (a.owner!=b.owner?256u:0u)|(a.root_tag!=b.root_tag?512u:0u);
}
inline unsigned roster_difference(const Roster& a,const Roster& b) {
 unsigned out=(a.context!=b.context?1u:0u)|(a.root!=b.root?2u:0u)|(a.owner!=b.owner?4u:0u)|
  (a.table!=b.table?8u:0u)|(a.allocation!=b.allocation?16u:0u)|(a.count!=b.count?32u:0u)|
  (a.rows.size()!=b.rows.size()?64u:0u);
 for(std::size_t n=0;n<std::min(a.rows.size(),b.rows.size());++n) {
  if(a.rows[n].id!=b.rows[n].id)out|=128u;
  if(a.rows[n].kind!=b.rows[n].kind)out|=256u;
  if(a.rows[n].name!=b.rows[n].name)out|=512u;
 }
 return out;
}
inline void format_deep_reason(std::array<char,384>& buffer,const DeepDiagnostic& diagnostic,ScopeFailure scope) {
 const auto& x=diagnostic.captures[0];const auto& y=diagnostic.captures[1];
 std::snprintf(buffer.data(),buffer.size(),
  "Blacktop category deep roster capture or repeated departure scope is uncertain; D=%u,%u,%u F=%u,%u R=%u,%u N=%u,%u NBA=%u,%u W=%u,%u DIFF=%u CTX=%u S=%u,%u,%u O=%u,%u",
  unsigned(diagnostic.first_ok),unsigned(diagnostic.second_attempted),unsigned(diagnostic.second_ok),
  unsigned(x.failure),unsigned(y.failure),x.failure==CaptureFailure::none?~0u:x.row,
  y.failure==CaptureFailure::none?~0u:y.row,x.count,y.count,x.nba,y.nba,x.clubs,y.clubs,
  diagnostic.difference,diagnostic.context_difference|x.context_difference|y.context_difference,unsigned(diagnostic.scope_attempted),
  unsigned(diagnostic.scope_ok),unsigned(scope),unsigned(diagnostic.departure_first),unsigned(diagnostic.departure_second));
}
// A fresh context must be stable for each capture, but menu indices are not
// ownership of the immutable roster. Preserve every actual allocation and row.
inline bool same_roster(const Roster& a,const Roster& b) {
 return a.root==b.root && a.owner==b.owner && a.table==b.table &&
  a.allocation==b.allocation && a.count==b.count && a.rows==b.rows;
}
inline bool stable_offline_departure(std::uintptr_t base,const outfit_match_v2::Context& a,const outfit_match_v2::Context& b) {
 return a==b && a.mode==a.other && a.active==0 && a.kind==0 && a.root_tag==base+outfit_match_v2::kRootTag &&
  outfit_match_v2::boundary(base,a)!=outfit_match_v2::Boundary::selection;
}
// These policy helpers have no process/file/call capability. The production
// caller supplies retained proof/read functions; fixtures inject failures at
// precisely the last forward-CAS gate and post-capture departure boundary.
template<class Scope,class Observe> bool forward_selection(std::uintptr_t base,Scope scope,Observe observe) {
 if(!scope())return false;
 outfit_match_v2::Context a{},b{};
 return observe(a) && observe(b) && a==b && a.mode==a.other &&
  outfit_match_v2::boundary(base,a)==outfit_match_v2::Boundary::selection;
}
template<class Capture,class Scope,class Observe>
bool capture_or_departure(std::uintptr_t base,Roster& out,bool& departed,Capture capture,Scope scope,Observe observe,
 DeepDiagnostic* diagnostic=nullptr) {
 departed=false;Roster a{},b{};
 const bool first=capture(a);const bool second=first && capture(b);
 if(diagnostic) {diagnostic->first_ok=first;diagnostic->second_attempted=first;diagnostic->second_ok=second;}
 if(first && second && a==b) {out=std::move(a);return true;}
 if(diagnostic && first && second) {
  diagnostic->difference=roster_difference(a,b);diagnostic->context_difference=context_difference(a.context,b.context);
 }
 // A failed deep capture alone grants nothing. Re-prove the ENTIRE retained
 // scope and exact route before observing a positive stable offline exit.
 if(diagnostic)diagnostic->scope_attempted=true;
 const bool scoped=scope();if(diagnostic)diagnostic->scope_ok=scoped;
 if(!scoped)return false;
 outfit_match_v2::Context before{},after{};
 const bool seen_first=observe(before);const bool seen_second=seen_first && observe(after);
 if(diagnostic) {diagnostic->departure_first=seen_first;diagnostic->departure_second=seen_second;}
 departed=seen_first && seen_second && stable_offline_departure(base,before,after);
 return false;
}
inline bool roster(std::uintptr_t base,Roster& out,std::uint32_t* observed_count=nullptr,CaptureDiagnostic* diagnostic=nullptr,Roster* context_candidate=nullptr) {
 using namespace outfit_match_v2;
 if(diagnostic)*diagnostic={};
 unsigned nba{},wnba{};
 const auto fail=[&](CaptureFailure why){if(diagnostic) {
  diagnostic->failure=why;diagnostic->nba=nba;diagnostic->clubs=wnba;
 }return false;};
 Roster r{};
 if(!context(base,r.context))return fail(CaptureFailure::context);
 if(boundary(base,r.context)!=Boundary::selection)return fail(CaptureFailure::selection);
 r.root=q(base+0x9ACE338);
 if(!private_span(r.root,0xB0))return fail(CaptureFailure::root_span);
 r.owner=q(r.root+0xA8);
 if(!private_span(r.owner,0x90))return fail(CaptureFailure::owner_span);
 MEMORY_BASIC_INFORMATION owner_region{};
 if(VirtualQuery(reinterpret_cast<const void*>(r.owner),&owner_region,sizeof owner_region)!=sizeof owner_region)return fail(CaptureFailure::owner_query);
 r.allocation=reinterpret_cast<std::uintptr_t>(owner_region.AllocationBase);
 if(!r.allocation)return fail(CaptureFailure::owner_allocation);
 if(!private_range(r.owner,0x90,r.allocation))return fail(CaptureFailure::owner_range);
 r.count=d(r.owner+0x80);r.table=q(r.owner+0x88);
 if(observed_count)*observed_count=r.count;
 if(diagnostic)diagnostic->count=r.count;
 if(r.count!=403 || !private_range(r.table,std::size_t(r.count)*0x1550,r.allocation))return fail(CaptureFailure::table_count_or_range);
 std::vector<unsigned char> table_bytes;
 if(!bounded_roster_bytes(r.table,table_bytes,[](std::uintptr_t at,void* out,std::size_t bytes){
   return outfit_match_v2::read_memory(at,out,bytes);}))return fail(CaptureFailure::row_fields);
 std::array<bool,18> women{};
 for(unsigned n=0;n<r.count;++n) {
  if(diagnostic)diagnostic->row=n;
  Roster::Team team{};std::uint32_t id_word{},kind_word{};
  if(!decode_roster_fields(table_bytes,n,id_word,kind_word,team.name))return fail(CaptureFailure::row_fields);
  team.id=std::uint16_t(id_word);team.kind=kind_word>>26;
  if(team.kind==0)++nba;
  if(team.kind==29) {
   if(team.id<300 || team.id>317 || (team.id>=312 && team.id<=314) || women[team.id-300])return fail(CaptureFailure::club_id);
   women[team.id-300]=true;++wnba;
   const auto end=std::find(team.name.begin(),team.name.end(),wchar_t{});
   if(end==team.name.begin())return fail(CaptureFailure::club_name);
   for(auto it=team.name.begin();it!=end;++it)if(*it<32 || *it==127)return fail(CaptureFailure::club_name);
  }
  if(diagnostic) {diagnostic->nba=nba;diagnostic->clubs=wnba;}
  r.rows.push_back(team);
 }
 Context after{};
 if(nba!=30 || wnba!=15)return fail(CaptureFailure::totals);
 if(!context(base,after))return fail(CaptureFailure::ending_context);
 if(after!=r.context) {
  if(diagnostic)diagnostic->context_difference=context_difference(r.context,after);
  // Waiting is observation-only. Finish the ending owner/table proofs even
  // though this capture remains failed; never normalize either Context.
  if(context_candidate && q(base+0x9ACE338)==r.root && q(r.root+0xA8)==r.owner &&
     d(r.owner+0x80)==r.count && q(r.owner+0x88)==r.table &&
     private_range(r.owner,0x90,r.allocation) &&
     private_range(r.table,std::size_t(r.count)*0x1550,r.allocation))
   *context_candidate=r;
  return fail(CaptureFailure::ending_context_changed);
 }
 if(q(base+0x9ACE338)!=r.root)return fail(CaptureFailure::ending_root);
 if(q(r.root+0xA8)!=r.owner)return fail(CaptureFailure::ending_owner);
 if(d(r.owner+0x80)!=r.count)return fail(CaptureFailure::ending_count);
 if(q(r.owner+0x88)!=r.table)return fail(CaptureFailure::ending_table);
 if(!private_range(r.owner,0x90,r.allocation))return fail(CaptureFailure::ending_owner_range);
 if(!private_range(r.table,std::size_t(r.count)*0x1550,r.allocation))return fail(CaptureFailure::ending_table_range);
 out=std::move(r);return true;
}
class PreparationClock {
 bool bound_{};std::uint64_t start_{},last_{};
public:
 bool bind(std::uint64_t start,std::uint64_t now) {
  if(bound_&&start!=start_)return false;
  if(now<start||now-start>=10000||(bound_&&now<last_))return false;
  if(!bound_){bound_=true;start_=start;}last_=now;return true;
 }
 bool valid(std::uint64_t now) {
  if(!bound_||now<last_||now<start_||now-start_>=10000)return false;
  last_=now;return true;
 }
};
struct IntentProof {
 outfit_match_v2::StableId id{};Binding binding;std::uint64_t ordinal{};
 std::uintptr_t base{};std::string bytes;std::wstring path;
 template<class Directory> bool unchanged(HANDLE retained,const Binding& expected,std::uintptr_t image,
   std::uint64_t number,Directory directory)const {
  using namespace outfit_match_v2;
  if(!retained || binding!=expected || !binding.valid() || !ordinal || ordinal!=number ||
     base!=image || bytes.empty() || bytes.size()>4096 || !directory())return false;
  StableId a{},b{},named{};std::vector<unsigned char> got;
  if(!file_id(retained,false,a)||a!=id||!read_file(retained,4096,got)||
     std::string(got.begin(),got.end())!=bytes||!file_id(retained,false,b)||b!=id)return false;
  Handle file(CreateFileW(path.c_str(),GENERIC_READ,FILE_SHARE_READ|FILE_SHARE_WRITE|FILE_SHARE_DELETE,
   nullptr,OPEN_EXISTING,FILE_FLAG_OPEN_REPARSE_POINT,nullptr));
  got.clear();
  return file&&file_id(file.value,false,named)&&named==id&&read_file(file.value,4096,got)&&
   std::string(got.begin(),got.end())==bytes&&file_id(retained,false,b)&&b==id&&directory();
 }
};
struct ZeroBridgeSnapshot {
 std::uintptr_t allocation{},base{};SIZE_T size{};DWORD state{},type{},protect{};
 std::array<unsigned char,8192> bytes{};
 bool operator==(const ZeroBridgeSnapshot&)const=default;
};
inline bool zero_bridge_snapshot(std::uintptr_t at,ZeroBridgeSnapshot& out) {
 MEMORY_BASIC_INFORMATION a{},b{};
 if(!at||VirtualQuery(reinterpret_cast<void*>(at),&a,sizeof a)!=sizeof a||
    a.State!=MEM_COMMIT||a.Type!=MEM_PRIVATE||a.Protect!=PAGE_READWRITE||
    reinterpret_cast<std::uintptr_t>(a.AllocationBase)!=at||
    reinterpret_cast<std::uintptr_t>(a.BaseAddress)!=at||a.RegionSize!=8192)return false;
 if(!outfit_match_v2::read_memory(at,out.bytes.data(),out.bytes.size())||
    std::any_of(out.bytes.begin(),out.bytes.end(),[](auto byte){return byte!=0;})||
    VirtualQuery(reinterpret_cast<void*>(at),&b,sizeof b)!=sizeof b||
    a.AllocationBase!=b.AllocationBase||a.BaseAddress!=b.BaseAddress||a.RegionSize!=b.RegionSize||
    a.State!=b.State||a.Type!=b.Type||a.Protect!=b.Protect)return false;
 out.allocation=at;out.base=at;out.size=a.RegionSize;out.state=a.State;out.type=a.Type;out.protect=a.Protect;return true;
}
inline bool prepared_bridge_state(std::uintptr_t at,std::uintptr_t base,const Code& code,DWORD code_protect,DWORD metadata_protect) {
 if(!((code_protect==PAGE_READWRITE&&metadata_protect==PAGE_READWRITE)||
      (code_protect==PAGE_EXECUTE_READ&&(metadata_protect==PAGE_READWRITE||metadata_protect==PAGE_READONLY))))return false;
 Code expected_code{};std::int32_t dispatch{};
 if(!bridge_plan(base,at,expected_code,dispatch)||expected_code!=code||at<base||at-base>0xFFFFFFFFull-8192)return false;
 const auto regions=[&](MEMORY_BASIC_INFORMATION& first,MEMORY_BASIC_INFORMATION& second){
  if(VirtualQuery(reinterpret_cast<void*>(at),&first,sizeof first)!=sizeof first||
     VirtualQuery(reinterpret_cast<void*>(at+4096),&second,sizeof second)!=sizeof second)return false;
  for(const auto* m:{&first,&second})if(m->State!=MEM_COMMIT||m->Type!=MEM_PRIVATE||
    reinterpret_cast<std::uintptr_t>(m->AllocationBase)!=at)return false;
  if(first.Protect!=code_protect||second.Protect!=metadata_protect)return false;
  if(code_protect==metadata_protect)return reinterpret_cast<std::uintptr_t>(first.BaseAddress)==at&&
    reinterpret_cast<std::uintptr_t>(second.BaseAddress)==at+4096&&first.RegionSize==8192&&second.RegionSize==4096;
  return reinterpret_cast<std::uintptr_t>(first.BaseAddress)==at&&first.RegionSize==4096&&
    reinterpret_cast<std::uintptr_t>(second.BaseAddress)==at+4096&&second.RegionSize==4096;
 };
 MEMORY_BASIC_INFORMATION a{},b{},c{},d{};
 if(!regions(a,b))return false;
 std::array<unsigned char,8192> expected{},actual{};
 std::copy(code.begin(),code.end(),expected.begin());
 constexpr auto chain=wnba_blacktop_bridge_unwind::kChainInfo;
 std::copy(chain.begin(),chain.end(),expected.begin()+4096);
 const RUNTIME_FUNCTION function{DWORD(at-base),DWORD(at-base+code.size()),DWORD(at-base+4096)};
 std::memcpy(expected.data()+4128,&function,sizeof function);
 if(!outfit_match_v2::read_memory(at,actual.data(),actual.size())||actual!=expected||!regions(c,d))return false;
 const auto same=[](const auto& x,const auto& y){return x.BaseAddress==y.BaseAddress&&x.AllocationBase==y.AllocationBase&&
  x.RegionSize==y.RegionSize&&x.State==y.State&&x.Type==y.Type&&x.Protect==y.Protect;};
 return same(a,c)&&same(b,d);
}
inline bool typed_context_wait(const DeepDiagnostic& d,bool generation_matches) {
 if(d.first_ok!=(d.captures[0].failure==CaptureFailure::none)||
    d.second_attempted!=d.first_ok||d.second_ok!=(d.second_attempted&&d.captures[1].failure==CaptureFailure::none)||
    d.difference||d.context_difference)return false;
 if(!generation_matches||!d.scope_attempted||!d.scope_ok||!d.departure_first||!d.departure_second)return false;
 bool changed=false;
 for(unsigned n=0;n<(d.second_attempted?2u:1u);++n) {
  const auto& c=d.captures[n];
  if(c.failure==CaptureFailure::none)continue;
  if(c.failure!=CaptureFailure::ending_context_changed||!c.context_difference||
     (c.context_difference&~224u)||c.count!=403||c.nba!=30||c.clubs!=15)return false;
  changed=true;
 }
 return changed;
}
} // namespace win_detail

// SourceHost must be the retained source NativeHost, exposing the narrowly
// reviewed category_source_ready() two-entry publication proof. Consent is a
// retained exact-session approval handle supplied by the owning trial worker.
// The trial worker may instantiate this class only after two-entry source
// publication. Ordinary Play and the product Blacktop toggle do not activate it.
template<class SourceHost> class NativeHost {
 outfit_match_v2::Authority* authority_{};SourceHost* source_{};
 outfit_match_v2::Handle process_,consent_;
 outfit_match_v2::StableId consent_id_{};
 outfit_match_v2::PinnedDirectory journal_directory_;
 outfit_match_v2::PinnedDirectory consent_directory_;
 Binding binding_;std::uintptr_t base_{},label_rva_{};std::uint64_t sequence_{};
 using LabelTable=std::array<std::uint8_t,wnba_blacktop_label_profile::kBytes>;
 LabelTable label_baseline_{};
 std::vector<outfit_match_v2::Handle> records_;
 std::optional<win_detail::Roster> generation_;
 std::optional<win_detail::IntentProof> allocation_intent_;
 std::size_t allocation_record_index_{};
 bool preinitialize_wait_{},initialization_attempted_{};
 win_detail::PreparationClock preparation_clock_;
 bool preparation_time(){return preparation_clock_.valid(GetTickCount64())||refuse("Blacktop bridge original preparation deadline expired or clock changed; no retry");}
 bool selection_departed_{};
 std::optional<Change> pending_;
 std::uintptr_t bridge_{};bool protection_uncertain_{},mutation_conflict_{},registered_{};
 RUNTIME_FUNCTION* runtime_{};Code prepared_code_{};
 mutable const char* reason_="Blacktop category admission has not opened";
 std::array<char,384> deep_reason_{};
 mutable win_detail::ScopeFailure departure_failure_{};
 bool refuse(const char* reason)const {reason_=reason;return false;}
 bool identity(const Binding& b)const {
  return authority_ && b==binding_ && process_ &&
   outfit_match_v2::process_identity(process_.value,b.birth,authority_->binding().game_executable) &&
   reinterpret_cast<std::uintptr_t>(GetModuleHandleW(nullptr))==base_;
 }
 bool consent()const {
  using namespace outfit_match_v2;
  outfit_match_v2::StableId id{};
  std::vector<unsigned char> bytes;
  if(!consent_ || !consent_directory_.unchanged() || !file_id(consent_.value,false,id) ||
     id!=consent_id_ || !read_file(consent_.value,72,bytes))return false;
  return category_approval_matches(bytes,binding_);
 }
 bool label_table(LabelTable& table)const {
  using namespace outfit_match_v2;
  namespace lp=wnba_blacktop_label_profile;
  unsigned char before{},after{};
  if(!value(base_+lp::kReadyRva,before) || before!=1)return false;
  // Reading the table may cross image regions; only the resolved target row
  // is writable by this host. Never require or change unrelated page access.
  const auto end=base_+lp::kArrayRva+lp::kBytes;
  for(auto cursor=base_+lp::kArrayRva;cursor<end;) {
   MEMORY_BASIC_INFORMATION m{};
   if(VirtualQuery(reinterpret_cast<void*>(cursor),&m,sizeof m)!=sizeof m ||
      m.State!=MEM_COMMIT || m.Type!=MEM_IMAGE ||
      reinterpret_cast<std::uintptr_t>(m.AllocationBase)!=base_ ||
      (m.Protect!=PAGE_READONLY && m.Protect!=PAGE_READWRITE && m.Protect!=PAGE_WRITECOPY))return false;
   const auto start=reinterpret_cast<std::uintptr_t>(m.BaseAddress);
   if(start>cursor || !m.RegionSize || m.RegionSize>0x7FFF00000000ull-start || start+m.RegionSize<=cursor)return false;
   cursor=std::min(end,start+m.RegionSize);
  }
  LabelTable second{};
  return read_memory(base_+lp::kArrayRva,table.data(),table.size()) &&
   read_memory(base_+lp::kArrayRva,second.data(),second.size()) && table==second &&
   value(base_+lp::kReadyRva,after) && after==before;
 }
 bool resolve_label() {
  namespace lp=wnba_blacktop_label_profile;
  LabelTable table{};
  if(!label_table(table))return refuse("Blacktop native label table is not initialized or stable");
  const auto found=lp::resolve_euro(table,1,table,1);
  if(!found || !win_detail::image_span(base_,base_+found->record_rva,16,PAGE_READWRITE))
   return refuse("Blacktop sorted label table does not contain the exact original Euro category");
  label_rva_=found->record_rva;label_baseline_=table;return true;
 }
 bool label_context(Route& route)const {
  namespace lp=wnba_blacktop_label_profile;
  if(label_rva_<lp::kArrayRva || label_rva_>=lp::kArrayRva+lp::kBytes ||
     (label_rva_-lp::kArrayRva)%lp::kStride ||
     !win_detail::image_span(base_,base_+label_rva_,16,PAGE_READWRITE))return false;
  LabelTable table{};if(!label_table(table))return false;
  const auto offset=label_rva_-lp::kArrayRva;
  std::memcpy(&route.label_id,table.data()+offset,4);
  std::memcpy(&route.label_reserved,table.data()+offset+4,4);
  std::memcpy(&route.label,table.data()+offset+8,8);
  if(route.label_id!=kLabelId || route.label_reserved || (route.label!=kEuro && route.label!=kWomen))return false;
  std::memcpy(table.data()+offset+8,&kEuro,8);
  return table==label_baseline_;
 }
 bool profile()const {
  using namespace outfit_match_v2;
  for(const auto& p:win_detail::methods) {
   std::vector<unsigned char> bytes(p.bytes);
   if(!win_detail::image_span(base_,base_+p.rva,p.bytes,PAGE_EXECUTE_READ) ||
      !win_detail::chunked_read(base_+p.rva,bytes.data(),bytes.size()) || digest(bytes)!=p.hash)
       return refuse("Blacktop category native method bytes or protection differ");
  }
  for(const auto& p:wnba_blacktop_label_profile::kMethods) {
   std::vector<unsigned char> bytes(p.bytes);
   if(!win_detail::image_span(base_,base_+p.rva,p.bytes,PAGE_EXECUTE_READ) ||
      !win_detail::chunked_read(base_+p.rva,bytes.data(),bytes.size()) || digest(bytes)!=p.hash)
    return refuse("Blacktop label lookup or sorting method differs");
  }
  // Native getters independently establish the live row ID/name offsets. Kind
  // filter is within the fully hashed function above; packed-file offsets alone
  // are not accepted as a live schema proof.
  constexpr std::array<unsigned char,7> id={0x0F,0xB7,0x81,0x20,0x0F,0,0};
  constexpr std::array<unsigned char,7> name={0x48,0x8D,0x81,0xFA,0x02,0,0};
  std::array<unsigned char,7> a{},b{};
  if(!(win_detail::image_span(base_,base_+0x2DDA733,a.size(),PAGE_EXECUTE_READ) &&
   win_detail::image_span(base_,base_+0x2DDA73E,b.size(),PAGE_EXECUTE_READ) &&
   read_memory(base_+0x2DDA733,a.data(),a.size()) && a==id &&
   read_memory(base_+0x2DDA73E,b.data(),b.size()) && b==name))
    return refuse("Blacktop roster getter bytes or protection differ");
  constexpr auto unwind=wnba_blacktop_bridge_unwind::kStockFrame;
  RUNTIME_FUNCTION pinned{},resolved{};std::array<unsigned char,12> actual{};
  // Prove the exact static .pdata row directly. RtlLookupFunctionEntry is
  // allowed to return a loader-managed copy of that row, so its pointer is not
  // an ownership identity. The returned image base and values still must equal
  // this independently pinned, read-only row before any bridge allocation.
  if(!win_detail::image_span(base_,base_+wnba_blacktop_bridge_unwind::kStockFunctionRva,
       sizeof pinned,PAGE_READONLY) ||
     !read_memory(base_+wnba_blacktop_bridge_unwind::kStockFunctionRva,&pinned,sizeof pinned) ||
     pinned.BeginAddress!=0x230BE80 || pinned.EndAddress!=0x230BF7B || pinned.UnwindData!=0x85CDBBC)
    return refuse("Blacktop static exception-table entry differs");
  DWORD64 image{};const auto* function=RtlLookupFunctionEntry(base_+0x230BEDB,&image,nullptr);
  if(!function || image!=base_ || !read_memory(reinterpret_cast<std::uintptr_t>(function),&resolved,sizeof resolved) ||
     std::memcmp(&resolved,&pinned,sizeof pinned)!=0)
    return refuse("Blacktop runtime exception lookup differs from pinned static entry");
  if(!win_detail::image_span(base_,base_+0x85CDBBC,actual.size(),PAGE_READONLY) ||
     !read_memory(base_+0x85CDBBC,actual.data(),actual.size()) || actual!=unwind)
    return refuse("Blacktop original unwind frame differs");
  return true;
 }
 bool publishing_scope(win_detail::ScopeFailure* diagnostic=nullptr)const {
  using F=win_detail::ScopeFailure;
  if(diagnostic)*diagnostic=F::none;
  const auto failed=[&](F why){if(diagnostic)*diagnostic=why;return false;};
  if(protection_uncertain_)return failed(F::protection);
  if(mutation_conflict_)return failed(F::mutation);
  if(!identity(binding_))return failed(F::identity);
  if(!authority_->safe())return failed(F::authority);
  if(!consent())return failed(F::consent);
  if(!source_->category_source_ready())return failed(F::source);
  return true;
 }
 bool forward_ready()const {
  return win_detail::forward_selection(base_,[&]{return publishing_scope();},
   [&](outfit_match_v2::Context& value){return outfit_match_v2::context(base_,value);});
 }
 bool departure_scope()const {
  if(!publishing_scope(&departure_failure_))return false;
  if(!profile()) {departure_failure_=win_detail::ScopeFailure::profile;return false;}
  Route route{};const bool ready=read_route(route);
  if(!ready)departure_failure_=win_detail::ScopeFailure::route;
  return ready;
 }
 bool record(const Binding& b,const std::string& body,bool allocation_intent=false) {
  using namespace outfit_match_v2;
  if(!identity(b) || !journal_directory_.unchanged() || sequence_>=wnba_blacktop_journal::kCapacity)return false;
  const auto number=++sequence_;
  const auto path=journal_directory_.path()+L"\\wnba-blacktop-category-"+
   std::to_wstring(b.pid)+L"-"+std::wstring(b.nonce.begin(),b.nonce.end())+L"-"+std::to_wstring(number)+L".intent";
  Handle f(CreateFileW(path.c_str(),GENERIC_READ|GENERIC_WRITE,FILE_SHARE_READ,nullptr,
   CREATE_NEW,FILE_FLAG_OPEN_REPARSE_POINT|FILE_FLAG_WRITE_THROUGH,nullptr));
  StableId before{},after{};
  const auto text="SCHEMA=wnba-blacktop-category-v1\nPID="+std::to_string(b.pid)+
   "\nBIRTH="+std::to_string(b.birth)+"\nSESSION="+b.nonce+"\nSEQUENCE="+
   std::to_string(number)+"\nBASE="+std::to_string(base_)+"\nTICK_MS="+
   std::to_string(GetTickCount64())+"\n"+body+"\n";
  DWORD written{};std::vector<unsigned char> readback;
  if(!f || text.size()>4096 || !file_id(f.value,false,before) ||
     !WriteFile(f.value,text.data(),DWORD(text.size()),&written,nullptr) || written!=text.size() ||
     !FlushFileBuffers(f.value) || !file_id(f.value,false,after) || before!=after ||
     !read_file(f.value,4096,readback) || std::string(readback.begin(),readback.end())!=text ||
     !journal_directory_.unchanged())return false;
  if(allocation_intent) {
   if(allocation_intent_)return false;
   allocation_record_index_=records_.size();
   allocation_intent_=win_detail::IntentProof{before,b,number,base_,text,path};
  }
  records_.push_back(std::move(f));return true;
 }
 static bool same(const Change& a,const Change& b) {
  return a.word==b.word && a.at==b.at && a.before==b.before && a.after==b.after;
 }
 bool forward_bridge_preparation(std::uintptr_t at,const Code& code,DWORD code_protect,DWORD metadata_protect) {
  if(!initialization_attempted_||registered_||!runtime_||at!=bridge_||code!=prepared_code_||
     reinterpret_cast<std::uintptr_t>(runtime_)!=at+4128||!preparation_time())return refuse("Bridge forward preparation retained state or deadline differs");
  if(!publishing_scope()||!profile()||!preparation_time()||
     !win_detail::prepared_bridge_state(at,base_,code,code_protect,metadata_protect)||
     !forward_ready()||!preparation_time())return refuse("Bridge forward preparation scope, exact bytes, page phase or deadline differs; no retry");
  return true;
 }
 bool allocation_page(std::size_t offset,DWORD protect)const {
  if(!bridge_)return false;
  MEMORY_BASIC_INFORMATION m{};
  return VirtualQuery(reinterpret_cast<void*>(bridge_+offset),&m,sizeof m)==sizeof m &&
   m.State==MEM_COMMIT && m.Type==MEM_PRIVATE && m.Protect==protect &&
   reinterpret_cast<std::uintptr_t>(m.AllocationBase)==bridge_ &&
   reinterpret_cast<std::uintptr_t>(m.BaseAddress)==bridge_+offset && m.RegionSize>=4096;
 }
public:
 const char* reason()const {return reason_;}
 std::uintptr_t label_rva()const {return label_rva_;}
 std::uintptr_t game_base()const {return base_;}
 const Binding& binding()const {return binding_;}
 // The roster executor has its own exact native callback profile. Restoration
 // depends on retained game identity, not the launcher/source/permit remaining
 // live. Publishing additionally needs those existing exact permissions.
 bool roster_checkpoint(const Binding& b,bool publishing)const {
  if(!identity(b))return refuse("Blacktop roster checkpoint retained identity differs");
  if(!publishing)return true;
  if(!authority_->safe())return refuse("Blacktop roster checkpoint launch authority lost");
  if(!source_->category_source_ready())return refuse("Blacktop roster checkpoint retained source differs");
  if(!consent())return refuse("Blacktop roster checkpoint exact-session consent differs");
  return true;
 }
 // No other thread or component may append to this host's journal. These
 // preflights reserve future cleanup by refusing new forward effects; active
 // observation and exact-owned restoration deliberately do not invoke them.
 bool roster_publication_budget()const {return wnba_blacktop_journal::roster(sequence_);}
 bool publication_budget(bool bridge_ready)const {return wnba_blacktop_journal::category(sequence_,bridge_ready);}
 bool roster_record(const Binding& b,const std::string& body) {
  return record(b,body);
 }
 NativeHost()=default;
 NativeHost(const NativeHost&)=delete;NativeHost& operator=(const NativeHost&)=delete;
 bool open(outfit_match_v2::Authority& authority,SourceHost& source) {
  using namespace outfit_match_v2;
  if(authority_ || !authority.safe())return refuse("Blacktop category owned offline session admission lost");
  if(!source.category_source_ready())return refuse("Blacktop category retained two-entry source proof failed");
  const auto& a=authority.binding();
  Binding b{a.game_pid,a.game_birth,a.session};
  if(!b.valid() || a.game_pid!=GetCurrentProcessId())return refuse("Blacktop category session binding differs");
  if(!consent_directory_.open(a.portable_root+L"\\Data\\wnba-priority-v2") ||
     !journal_directory_.open(a.portable_root+L"\\Logs\\game-session"))return refuse("Blacktop category directory pin failed");
  const auto token_path=consent_directory_.path()+L"\\blacktop-categories-"+
   std::wstring(b.nonce.begin(),b.nonce.end())+L".bin";
  consent_=Handle(CreateFileW(token_path.c_str(),GENERIC_READ,FILE_SHARE_READ,nullptr,
   OPEN_EXISTING,FILE_FLAG_OPEN_REPARSE_POINT,nullptr));
  if(!file_id(consent_.value,false,consent_id_))return refuse("Blacktop category exact-session permission file missing or invalid");
  authority_=&authority;source_=&source;binding_=std::move(b);base_=authority.base();
  process_=retain_current_process();
  if(!identity(binding_))return refuse("Blacktop category retained game identity differs");
  if(!consent())return refuse("Blacktop category exact-session permission bytes differ");
  if(!profile())return false;
  if(!resolve_label())return false;
  HMODULE retained{};
  // A published runtime function must never outlive code belonging to this
  // executor. PIN is process lifetime; deliberately no FreeLibrary counterpart.
  if(GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS|GET_MODULE_HANDLE_EX_FLAG_PIN,
   reinterpret_cast<LPCWSTR>(&win_detail::image_span),&retained)==FALSE || !retained)
    return refuse("Blacktop category process-lifetime module retention failed");
  reason_="Blacktop category admission opened";return true;
 }
 Input sample_input()const {
  using namespace outfit_match_v2;
  Input in{};in.game=binding_;
  if(!identity(binding_))return in;
  in.process=Process::live;in.owned_offline=authority_->safe();
  in.source_trial_verified=source_->category_source_ready();
  in.explicit_category_consent=consent();
  Context first{},second{};
  if(!context(base_,first) || !context(base_,second) || first!=second)return in;
  in.current=first.mode;in.alternate=first.other;in.stage=first.stage;
  in.active=first.active;in.kind=first.kind;in.stable=true;
  if(boundary(base_,first)!=Boundary::selection)return in;
  win_detail::Roster a{},b{};
  if(!initialization_attempted_&&!runtime_&&!registered_&&generation_) {
   std::array<win_detail::Roster,2> candidates{};win_detail::DeepDiagnostic diagnostic{};unsigned captured{};bool departed{};
   const bool complete=win_detail::capture_or_departure(base_,a,departed,[&](win_detail::Roster& value){
    const auto index=captured++;const bool ok=win_detail::roster(base_,value,&in.observed_team_count,&diagnostic.captures[index],&candidates[index]);
    if(ok)candidates[index]=value;return ok;},[&]{return departure_scope();},
    [&](Context& value){return context(base_,value);},&diagnostic);
   if(!complete) {
    bool same_generation=true;
    for(unsigned n=0;n<captured;++n)if(!win_detail::same_roster(*generation_,candidates[n]))same_generation=false;
    if(win_detail::typed_context_wait(diagnostic,same_generation))in.stable=false;
    return in; // Never emit a generation/admission from a failed capture.
   }
  } else if(!win_detail::roster(base_,a,&in.observed_team_count) || !win_detail::roster(base_,b) || a!=b)return in;
  Hash hash;
  if(!hash.update(&a.root,sizeof a.root) || !hash.update(&a.owner,sizeof a.owner) ||
     !hash.update(&a.table,sizeof a.table) || !hash.update(&a.allocation,sizeof a.allocation) ||
     !hash.update(&a.count,sizeof a.count))return in;
  for(const auto& row:a.rows) {
   if(!hash.update(&row.id,sizeof row.id) || !hash.update(&row.kind,sizeof row.kind) ||
      !hash.update(row.name.data(),sizeof row.name))return in;
   if(row.kind==29)in.clubs.push_back(row.id);
  }
  const auto digest=hash.finish();
  if(digest.size()!=64)return in;
  for(unsigned n=0;n<16;++n) {
   const auto c=digest[n];const auto digit=c>='0'&&c<='9'?c-'0':c-'A'+10;
   in.roster_generation=(in.roster_generation<<4)|std::uint64_t(digit);
  }
  in.deep_mixed=in.roster_generation!=0;return in;
 }
 bool selection_departed()const {return selection_departed_;}
 bool checkpoint(const Binding& b,bool publishing) {
  selection_departed_=false;preinitialize_wait_=false;
  if(protection_uncertain_)return refuse("Blacktop category page protection is uncertain");
  if(mutation_conflict_)return refuse("Blacktop category mutation ownership conflicted");
  if(!identity(b))return refuse("Blacktop category retained game identity differs");
  if(!profile())return false;
  Route route{};if(!read_route(route))return refuse("Blacktop category route, label baseline or RX bridge differs");
  if(!publishing) { generation_.reset();return true; }
  if(!authority_->safe())return refuse("Blacktop category launch authority lost");
  if(!consent())return refuse("Blacktop category exact-session consent differs");
  if(!source_->category_source_ready())return refuse("Blacktop category retained source differs");
  outfit_match_v2::Context before{},after{};
  if(!outfit_match_v2::context(base_,before) || !outfit_match_v2::context(base_,after) || before!=after)
   return refuse("Blacktop category current selection context is uncertain");
  if(outfit_match_v2::boundary(base_,before)!=outfit_match_v2::Boundary::selection) {
   // Only a positively established, stable offline departure can request
   // conditional cleanup. Unknown owner/profile/permission failures never do.
   selection_departed_=win_detail::stable_offline_departure(base_,before,after);
   return refuse(selection_departed_?"Blacktop category stable offline selection exited":"Blacktop category selection admission lost");
  }
  win_detail::Roster a{};std::array<win_detail::Roster,2> captured_rows{};win_detail::DeepDiagnostic diagnostic{};unsigned captured{};
  departure_failure_=win_detail::ScopeFailure::none;
  if(!win_detail::capture_or_departure(base_,a,selection_departed_,
    [&](win_detail::Roster& value){const auto index=captured++;
     const bool ok=win_detail::roster(base_,value,nullptr,&diagnostic.captures[index],&captured_rows[index]);
     if(ok)captured_rows[index]=value;return ok;},
    [&]{return departure_scope();},
    [&](outfit_match_v2::Context& value){return outfit_match_v2::context(base_,value);},&diagnostic)) {
   if(selection_departed_)return refuse("Blacktop category stable offline selection exited during deep capture");
   bool same_generation=generation_.has_value();
   for(unsigned n=0;n<captured;++n)if(!generation_||!win_detail::same_roster(*generation_,captured_rows[n]))same_generation=false;
   preinitialize_wait_=!initialization_attempted_&&!runtime_&&!registered_&&!pending_&&
    win_detail::typed_context_wait(diagnostic,same_generation);
   win_detail::format_deep_reason(deep_reason_,diagnostic,departure_failure_);
   return refuse(deep_reason_.data());
  }
  if(generation_ && !win_detail::same_roster(*generation_,a))return refuse("Blacktop category retained roster identity or rows differ");
  generation_=std::move(a);return true;
 }

 bool read_route(Route& out)const {
  using namespace outfit_match_v2;
  if(!base_ || mutation_conflict_ || !win_detail::image_span(base_,base_+kTable,24,PAGE_READONLY))return false;
  std::array<unsigned char,24> table{};Route r{};
  if(!read_memory(base_+kTable,table.data(),table.size()) || !label_context(r))return false;
  std::memcpy(&r.dispatch,table.data()+12,4);
  std::copy(win_detail::stock_table.begin()+12,win_detail::stock_table.begin()+16,table.begin()+12);
  if(table!=win_detail::stock_table || r.label_id!=kLabelId || r.label_reserved ||
     (r.label!=kEuro && r.label!=kWomen))return false;
  if(r.dispatch!=kStockDispatch) {
   Code code{};std::int32_t dispatch{};
   if(!bridge_ || !bridge_plan(base_,bridge_,code,dispatch) || r.dispatch!=dispatch ||
      !bridge_is_private_rx(bridge_,code))return false;
  }
  out=r;return true;
 }
 bool journal_change(const Binding& b,const Change& c) {
  if(pending_)return false;
  if(!record(b,"ACTION=DATA_CAS_INTENT\nWORD="+std::to_string(unsigned(c.word))+
    "\nADDRESS="+std::to_string(c.at)+"\nBEFORE="+std::to_string(c.before)+
    "\nAFTER="+std::to_string(c.after)+"\nEFFECTS=exact-page-protection-if-needed;aligned-CAS;readback;restore-original-protection"))return false;
  pending_=c;return true;
 }
 bool write_change(const Change& c) {
  using namespace outfit_match_v2;
  if(!pending_ || !same(*pending_,c) || !identity(binding_) || !profile() || protection_uncertain_ || mutation_conflict_)return false;
  Route before{};if(!read_route(before))return false;
  if(c.word==Word::label) {
   if(c.at!=base_+label_rva_+8 || c.at%8 || before.label!=c.before ||
      !((c.before==kEuro && c.after==kWomen)||(c.before==kWomen && c.after==kEuro)))return false;
   if(c.after==kWomen && !forward_ready())return refuse("Blacktop label final publishing permission or selection ended; no CAS");
   const auto prior=InterlockedCompareExchange64(reinterpret_cast<volatile LONG64*>(c.at),LONG64(c.after),LONG64(c.before));
   if(std::uint64_t(prior)!=c.before) {
    mutation_conflict_=true;return refuse("Blacktop label CAS conflicted; identical foreign data is not owned");
   }
   std::uint64_t after{};
   return std::uint64_t(prior)==c.before && value(c.at,after) && after==c.after;
  }
  if(c.word!=Word::dispatch || c.at!=base_+kEntry || c.at%4 ||
     std::uint32_t(before.dispatch)!=c.before || c.before>0xFFFFFFFFull || c.after>0xFFFFFFFFull)return false;
  Code code{};std::int32_t dispatch{};
  if(!bridge_ || !bridge_plan(base_,bridge_,code,dispatch) || !bridge_is_private_rx(bridge_,code) ||
     !((c.before==std::uint32_t(kStockDispatch) && c.after==std::uint32_t(dispatch)) ||
       (c.before==std::uint32_t(dispatch) && c.after==std::uint32_t(kStockDispatch))))return false;
  DWORD old{};
  if(!VirtualProtect(reinterpret_cast<void*>(c.at),4,PAGE_READWRITE,&old))return false;
  bool wrote=false;
  auto restore_page=[&](void* address) noexcept {
   DWORD changed{};const bool restored=VirtualProtect(address,4,old,&changed)!=FALSE;
   if(!restored || changed!=PAGE_READWRITE || old!=PAGE_READONLY ||
      !win_detail::image_span(base_,c.at,4,PAGE_READONLY))protection_uncertain_=true;
  };
  // Final source/permission proof can allocate. Every exception/return after
  // VirtualProtect must restore the original protection before any hold.
  std::unique_ptr<void,decltype(restore_page)> protection(reinterpret_cast<void*>(c.at),restore_page);
  // Protection mismatches are held even if the requested operation succeeded.
  if(old==PAGE_READONLY) {
   auto expected_table=win_detail::stock_table;const auto dispatch_before=std::uint32_t(c.before);
   std::memcpy(expected_table.data()+12,&dispatch_before,4);
   std::array<unsigned char,24> actual_table{};
   std::array<unsigned char,16> expected_label{},actual_label{};
   std::memcpy(expected_label.data(),&before.label_id,4);
   std::memcpy(expected_label.data()+4,&before.label_reserved,4);
   std::memcpy(expected_label.data()+8,&before.label,8);
   if(read_memory(base_+kTable,actual_table.data(),actual_table.size()) && actual_table==expected_table &&
      read_memory(base_+label_rva_,actual_label.data(),actual_label.size()) && actual_label==expected_label) {
    const bool publishing=c.before==std::uint32_t(kStockDispatch);
    // No early return inside this protection window: final denial still
    // restores the exact original page access before returning no-CAS.
    // The retained WPF implementation proves Blacktop selection immediately
    // before changing the label, then publishes the dispatch without another
    // menu-context sample between the paired words. The category transition
    // itself can advance that context. The core has already completed two
    // deep checkpoints and exact-route proofs for this dispatch; retain the
    // final authority/source/consent gate here without rejecting that normal
    // transition between the label and dispatch CAS operations.
    if(!publishing || publishing_scope()) {
    const auto prior=InterlockedCompareExchange(reinterpret_cast<volatile LONG*>(c.at),LONG(c.after),LONG(c.before));
    if(std::uint32_t(prior)!=c.before) {
     mutation_conflict_=true;reason_="Blacktop dispatch CAS conflicted; identical foreign data is not owned";
    }
    std::uint32_t after{};wrote=std::uint32_t(prior)==c.before && value(c.at,after) && after==c.after;
    } else reason_="Blacktop dispatch final publishing permission or selection ended; no CAS";
   }
  }
  protection.reset();
  Route after{};
  return wrote && !protection_uncertain_ && read_route(after) && std::uint32_t(after.dispatch)==c.after;
 }
 bool journal_applied(const Binding& b,const Change& c) {
  if(!pending_ || !same(*pending_,c))return false;
  const bool ok=record(b,"ACTION=DATA_CAS_APPLIED\nADDRESS="+std::to_string(c.at)+"\nAFTER="+std::to_string(c.after));
  if(ok)pending_.reset();return ok;
 }
 bool journal_not_applied(const Binding& b,const Change& c) {
  if(!pending_ || !same(*pending_,c))return false;
  const bool ok=record(b,"ACTION=DATA_CAS_NOT_APPLIED\nADDRESS="+std::to_string(c.at)+
    "\nEXPECTED_BEFORE="+std::to_string(c.before)+"\nEFFECTS=none;exact-before-route-readback");
  if(ok)pending_.reset();return ok;
 }
 bool journal_bridge_intent(const Binding& b) {
  if(!preparation_time())return false;
  return record(b,"ACTION=BRIDGE_ALLOCATION_INTENT\nALLOCATION_BYTES=8192\nCODE_BYTES=10\nCANDIDATE_OFFSETS=40000000-through-4FFF0000-step10000\nPROTECTION=RW-then-RX-code-and-R-metadata\nRETENTION=process-lifetime\nUNWIND=CHAININFO-registration-required",true);
 }
 std::uintptr_t allocate_bridge_near(std::uintptr_t base,std::uintptr_t table,std::size_t size) {
  if(bridge_ || base!=base_ || table!=kTable || size!=10) {refuse("Bridge allocation arguments or retained state differ");return 0;}
  if(!checkpoint(binding_,true))return 0; // Preserve the exact checkpoint diagnostic.
  if(!preparation_time())return 0;
  // All candidates lie beyond the game image and within DWORD function-table
  // RVAs relative to the game base. Both dispatch and tail jump get independent
  // signed32 checks. No released allocation can leave a stale loaded jump.
  for(std::uintptr_t offset=0x40000000;offset<0x50000000;offset+=0x10000) {
   const auto at=base_+offset;Code code{};std::int32_t dispatch{};
   if(!bridge_plan(base_,at,code,dispatch))continue;
   if(!preparation_time())return 0;
   auto* p=VirtualAlloc(reinterpret_cast<void*>(at),8192,MEM_RESERVE|MEM_COMMIT,PAGE_READWRITE);
   if(!p)continue;
   bridge_=reinterpret_cast<std::uintptr_t>(p); // Immediately retained even on failure.
   if(bridge_!=at) {refuse("Bridge allocation returned a different retained location");return 0;}
   MEMORY_BASIC_INFORMATION m{};
   if(VirtualQuery(p,&m,sizeof m)!=sizeof m || m.State!=MEM_COMMIT || m.Type!=MEM_PRIVATE ||
      m.Protect!=PAGE_READWRITE || m.AllocationBase!=p || m.BaseAddress!=p || m.RegionSize!=8192) {
    refuse("Bridge allocation private RW shape differs; allocation retained");return 0;
   }
   return bridge_;
  }
  refuse("Bridge allocation exhausted candidates without success; no publication");return 0;
 }
 void retain_bridge(std::uintptr_t at) {
  if(at!=bridge_)protection_uncertain_=true; // No free or unregister operation exists.
 }
 bool bind_preparation_start(std::uint64_t start){return preparation_clock_.bind(start,GetTickCount64());}
 bool passive_preparation_scope() {
  if(!preparation_time())return false;
  if(initialization_attempted_||runtime_||registered_||pending_||!generation_||!publishing_scope()||!profile())return false;
  Route route{};if(!read_route(route)||route!=Route{})return false;
  if(!bridge_)return !allocation_intent_&&preparation_time();
  if(!allocation_intent_||allocation_record_index_>=records_.size())return false;
  const auto intent=[&]{return allocation_intent_->unchanged(records_[allocation_record_index_].value,
   binding_,base_,allocation_intent_->ordinal,[&]{return journal_directory_.unchanged()&&consent_directory_.unchanged();});};
  win_detail::ZeroBridgeSnapshot a{},b{};
  return intent()&&win_detail::zero_bridge_snapshot(bridge_,a)&&publishing_scope()&&profile()&&
   read_route(route)&&route==Route{}&&intent()&&win_detail::zero_bridge_snapshot(bridge_,b)&&a==b&&
   publishing_scope()&&profile()&&read_route(route)&&route==Route{}&&preparation_time();
 }
 PreinitializeProof preallocation_proof() {
  if(!preinitialize_wait_||bridge_||allocation_intent_||initialization_attempted_||runtime_||registered_||pending_||
     !generation_||!passive_preparation_scope())return PreinitializeProof::failed;
  return PreinitializeProof::defer;
 }
 std::uint64_t monotonic_ms()const {return GetTickCount64();}
 PreinitializeProof preinitialize_proof(bool checkpoint_passed) {
  if(!preparation_time())return PreinitializeProof::failed;
  if(!bridge_||initialization_attempted_||runtime_||registered_||pending_||protection_uncertain_||mutation_conflict_||
     !generation_||!allocation_intent_||allocation_record_index_>=records_.size()||
     (!checkpoint_passed&&!preinitialize_wait_))return PreinitializeProof::failed;
  bool deferred=!checkpoint_passed;
  const auto check=[&]{const bool ok=checkpoint(binding_,true);if(!preparation_time()||(!ok&&!preinitialize_wait_))return false;deferred|=!ok;return true;};
  const auto intent=[&]{return allocation_intent_->unchanged(records_[allocation_record_index_].value,
   binding_,base_,allocation_intent_->ordinal,[&]{return journal_directory_.unchanged()&&consent_directory_.unchanged();});};
  win_detail::ZeroBridgeSnapshot before{},after{};
  if(!check()||!intent()||!win_detail::zero_bridge_snapshot(bridge_,before)||
     !check()||!intent()||!win_detail::zero_bridge_snapshot(bridge_,after)||before!=after||
     !check()||!intent())return PreinitializeProof::failed;
  return deferred?PreinitializeProof::defer:PreinitializeProof::exact;
 }
 bool initialize_bridge_rw(std::uintptr_t at,const Code& code) {
  preinitialize_wait_=false;
  if(at!=bridge_ || !at || registered_ || runtime_ || initialization_attempted_)return refuse("Bridge initialize arguments or retained state differ");
  const bool strict=checkpoint(binding_,true);
  const auto proof=preinitialize_proof(strict);
  if(proof!=PreinitializeProof::exact) {preinitialize_wait_=proof==PreinitializeProof::defer;return false;}
  MEMORY_BASIC_INFORMATION m{};
  if(VirtualQuery(reinterpret_cast<void*>(at),&m,sizeof m)!=sizeof m ||
     m.State!=MEM_COMMIT || m.Type!=MEM_PRIVATE || m.Protect!=PAGE_READWRITE ||
     reinterpret_cast<std::uintptr_t>(m.AllocationBase)!=at ||
     reinterpret_cast<std::uintptr_t>(m.BaseAddress)!=at || m.RegionSize!=8192)return refuse("Bridge initialize private RW shape differs");
  Code expected{};std::int32_t dispatch{};
  if(!bridge_plan(base_,at,expected,dispatch) || code!=expected)return refuse("Bridge initialize exact code plan differs");
  // CHAININFO is Microsoft's noncontiguous established-frame representation.
  // Its triple references the exact original switch's runtime-function entry,
  // whose live unwind bytes are pinned in profile(). Zero-prologue clones fail
  // at the E9 instruction in the independent Windows unwind fixture.
  constexpr auto chain=wnba_blacktop_bridge_unwind::kChainInfo;
  RUNTIME_FUNCTION f{DWORD(at-base_),DWORD(at-base_+code.size()),DWORD(at-base_+4096)};
  if(!preparation_time())return false;
  initialization_attempted_=true;preinitialize_wait_=false; // BEFORE any durable intent attempt, including partial/error.
  if(!record(binding_,"ACTION=BRIDGE_INITIALIZE_INTENT\nADDRESS="+std::to_string(at)+
     "\nCODE_BYTES=10\nCHAIN_OFFSET=4096\nRUNTIME_FUNCTION_OFFSET=4128\nEFFECTS=write-private-RW-code-and-chain;readback"))return refuse("Bridge initialize intent was not durable; no initialization writes");
  if(!preparation_time())return false;
  // Durable intent is irreversible even if this last fresh proof refuses.
  win_detail::ZeroBridgeSnapshot final_zero{};
  if(!checkpoint(binding_,true)||!allocation_intent_||
     !allocation_intent_->unchanged(records_[allocation_record_index_].value,binding_,base_,allocation_intent_->ordinal,
       [&]{return journal_directory_.unchanged()&&consent_directory_.unchanged();})||
     !win_detail::zero_bridge_snapshot(at,final_zero))return refuse("Bridge initialize final fresh proof failed after intent; never replay");
  if(!preparation_time())return false;
  std::memcpy(reinterpret_cast<void*>(at),code.data(),code.size());
  std::memcpy(reinterpret_cast<void*>(at+4096),chain.data(),chain.size());
  std::memcpy(reinterpret_cast<void*>(at+4128),&f,sizeof f);
  Code got{};std::array<unsigned char,16> actual{};RUNTIME_FUNCTION table{};
  if(!outfit_match_v2::read_memory(at,got.data(),got.size()) || got!=code ||
     !outfit_match_v2::read_memory(at+4096,actual.data(),actual.size()) || actual!=chain ||
     !outfit_match_v2::value(at+4128,table) || std::memcmp(&table,&f,sizeof f))return refuse("Bridge initialize readback differs; allocation retained");
  runtime_=reinterpret_cast<RUNTIME_FUNCTION*>(at+4128);prepared_code_=code;return true;
 }
 bool seal_bridge_rx(std::uintptr_t at,const Code& code) {
  if(!preparation_time())return false;
  if(at!=bridge_ || !at || !runtime_ || registered_ || code!=prepared_code_)return refuse("Bridge seal arguments or retained state differ");
  if(!checkpoint(binding_,true))return false;
  if(!preparation_time())return false;
  if(!record(binding_,"ACTION=BRIDGE_SEAL_AND_UNWIND_INTENT\nADDRESS="+std::to_string(at)+
     "\nEFFECTS=code-RW-to-RX;metadata-RW-to-R;FlushInstructionCache;RtlAddFunctionTable\nRETENTION=process-lifetime"))return refuse("Bridge seal intent was not durable; no protection changes");
  if(!forward_bridge_preparation(at,code,PAGE_READWRITE,PAGE_READWRITE))return false;
  DWORD old{},meta_old{};
  if(!VirtualProtect(reinterpret_cast<void*>(at),4096,PAGE_EXECUTE_READ,&old)||old!=PAGE_READWRITE)
   return refuse("Bridge seal code protection differs; allocation retained");
  if(!forward_bridge_preparation(at,code,PAGE_EXECUTE_READ,PAGE_READWRITE))return false;
  if(!VirtualProtect(reinterpret_cast<void*>(at+4096),4096,PAGE_READONLY,&meta_old)||meta_old!=PAGE_READWRITE)
   return refuse("Bridge seal metadata protection differs; allocation retained");
  if(!forward_bridge_preparation(at,code,PAGE_EXECUTE_READ,PAGE_READONLY))return false;
  if(!FlushInstructionCache(GetCurrentProcess(),reinterpret_cast<void*>(at),code.size()))
   return refuse("Bridge seal instruction-cache flush failed; allocation retained");
  // RtlAddFunctionTable retains immutable process-lifetime metadata. Every
  // preceding intent/flush/protection interval gets a fresh last-effect gate.
  if(!forward_bridge_preparation(at,code,PAGE_EXECUTE_READ,PAGE_READONLY))return false;
  if(!RtlAddFunctionTable(runtime_,1,base_))return refuse("Bridge unwind registration failed; allocation retained");
  registered_=true;
  return profile() && bridge_is_private_rx(at,code);
 }
 bool bridge_is_private_rx(std::uintptr_t at,const Code& code)const {
  if(!at || at!=bridge_ || !registered_ || !runtime_ || code!=prepared_code_ ||
     !allocation_page(0,PAGE_EXECUTE_READ) || !allocation_page(4096,PAGE_READONLY))return refuse("Bridge RX retained identity or page protection differs");
  constexpr auto chain=wnba_blacktop_bridge_unwind::kChainInfo;
  Code got{};std::array<unsigned char,16> actual{};RUNTIME_FUNCTION table{};
  if(!outfit_match_v2::read_memory(at,got.data(),got.size()) || got!=code ||
     !outfit_match_v2::read_memory(at+4096,actual.data(),actual.size()) || actual!=chain ||
     !outfit_match_v2::value(at+4128,table) || table.BeginAddress!=at-base_ ||
     table.EndAddress!=at-base_+10 || table.UnwindData!=at-base_+4096)return refuse("Bridge RX code or unwind metadata readback differs");
  for(std::uintptr_t pc=at;pc<at+10;++pc) {
   DWORD64 image{};
   if(RtlLookupFunctionEntry(pc,&image,nullptr)!=runtime_ || image!=base_)return refuse("Bridge RX runtime-function lookup differs");
  }
  return true;
 }
 bool journal_bridge_ready(const Binding& b,std::uintptr_t at,const Code& code) {
  if(!preparation_time())return false;
  if(!bridge_is_private_rx(at,code)||!preparation_time())return false;
  return record(b,"ACTION=BRIDGE_READY\nADDRESS="+
   std::to_string(at)+"\nUNWIND=CHAININFO-exact-stock-function\nCODE=private-RX\nMETADATA=private-R\nRETENTION=process-lifetime") ||
   refuse("Bridge ready receipt was not durable; allocation retained");
 }
};
} // namespace wnba_blacktop_category
