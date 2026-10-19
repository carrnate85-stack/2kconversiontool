#define OUTFIT_MATCH_V2_OFFLINE_TEST
#include "../native/outfit_match_v2_transport.hpp"
#include <cassert>
#include <chrono>
#include <filesystem>
#include <iostream>
#include <stdexcept>
#include <type_traits>
using namespace outfit_match_v2;
static_assert(!std::is_copy_constructible_v<ExpansionPermit>);
static_assert(!std::is_copy_assignable_v<ExpansionPermit>);
static_assert(!std::is_default_constructible_v<ExpansionPermit>);
static_assert(!std::is_constructible_v<ExpansionPermit,const void*,ExpansionPlan>);
namespace {
ExpansionPlan first(unsigned count=3,unsigned length=8) {
 ExpansionPlan p{};p.base=0x10000000;p.original=0x20000;p.clone=0x30000;p.owner=0x40000;p.heap=0x50000;
 p.actor=0x60000;p.source=0x70000;p.row=0x80000;p.manager=0x90000;p.loaded=0xa0000;p.state=0xb0000;p.count=count;
 for(unsigned i=0;i<count;++i){auto name=std::wstring(length,L'a');name.back()=wchar_t(L'0'+i);p.items.push_back(name);}
 p.calls=expansion_calls(p);return p;
}
ExpansionPlan second(ExpansionPlan p) {p.stage=ExpansionStage::set_bind;p.exposed=0xc0000;p.owned=0xd0000;p.calls=expansion_calls(p);return p;}
void cursor_tests() {
 int owner{},foreign{};
 ExpansionArrayBounds safe{0x500000,0x600000,0x700000,0x800000,3,2,0x100000,0x100f30,0x200000,0x300000,0x400000,0x38c0,0x2970,{0x900000,0xA00000}};
 assert(independent_expansion_arrays(safe));
 for(bool target_owned:{false,true}) {
  for(auto source:{safe.source_exposed,safe.source_owned,safe.actor,safe.source_description,safe.clone,safe.owner,safe.source_owner}) {
   for(auto offset:{-8,0,8}) {
    auto bad=safe;const auto at=std::uintptr_t(std::int64_t(source)+offset);
    (target_owned?bad.owned:bad.exposed)=at;
    assert(!independent_expansion_arrays(bad));
   }
  }
 }
 for(bool exposed:{false,true})for(auto source:safe.source_strings)for(auto offset:{-8,0,8,504}) {
  auto bad=safe;(exposed?bad.exposed:bad.owned)=std::uintptr_t(std::int64_t(source)+offset);
  assert(!independent_expansion_arrays(bad));
 }
 auto string_adjacent=safe;string_adjacent.exposed=safe.source_strings[0]+512;assert(independent_expansion_arrays(string_adjacent));
 auto bad_strings=safe;bad_strings.source_strings.pop_back();assert(!independent_expansion_arrays(bad_strings));
 bad_strings=safe;bad_strings.source_strings[0]=0x7fff00000000ull-511;assert(!independent_expansion_arrays(bad_strings));
 for(auto offset:{-16,-8,0,8,16}) {auto bad=safe;bad.owned=std::uintptr_t(std::int64_t(bad.exposed)+offset);assert(!independent_expansion_arrays(bad));}
 auto adjacent=safe;adjacent.owned=adjacent.exposed+adjacent.count*8;assert(independent_expansion_arrays(adjacent));
 adjacent=safe;adjacent.exposed=adjacent.actor-adjacent.count*8;assert(independent_expansion_arrays(adjacent));
 adjacent=safe;adjacent.exposed=adjacent.actor+adjacent.actor_bytes;assert(independent_expansion_arrays(adjacent));
 adjacent=safe;adjacent.exposed=adjacent.owner+0x28;assert(independent_expansion_arrays(adjacent));
 auto high=safe;high.exposed=0x7fff00000000ull-high.count*8;assert(independent_expansion_arrays(high));
 for(auto at:{std::uintptr_t(0),std::uintptr_t(0xffff),std::uintptr_t(0x7fff00000000ull-8),~std::uintptr_t(0)-7}) {
  auto bad=safe;bad.exposed=at;assert(!independent_expansion_arrays(bad));
 }
 auto invalid=safe;invalid.count=0;assert(!independent_expansion_arrays(invalid));invalid=safe;invalid.count=8;assert(!independent_expansion_arrays(invalid));
 invalid=safe;invalid.source_count=0;assert(!independent_expansion_arrays(invalid));invalid=safe;invalid.actor_bytes=0;assert(!independent_expansion_arrays(invalid));
 invalid=safe;invalid.description_bytes=65537;assert(!independent_expansion_arrays(invalid));invalid=safe;invalid.source_owned=~std::uintptr_t(0)-7;assert(!independent_expansion_arrays(invalid));
 for(unsigned count:{3u,7u}) {
  for(auto stage:{ExpansionStage::release_init,ExpansionStage::set_bind}) {
   auto p=stage==ExpansionStage::release_init?first(count):second(first(count));
   assert(valid_expansion_plan(p));
   for(unsigned failure=0;failure<p.calls.size();++failure) {
    for(bool fail_guard:{false,true}) {
     auto permit=ExpansionPermit::fixture(&owner,p);unsigned guards{},effects{};
     for(unsigned i=0;i<p.calls.size();++i) {
      const auto ok=permit.invoke(&owner,p.calls[i],[&](){++guards;return !(fail_guard&&i==failure);},[&](){++effects;return !(!fail_guard&&i==failure);});
      if(i==failure){assert(!ok&&permit.failed());break;}assert(ok);
     }
     assert(effects==failure+(fail_guard?0u:1u));
     assert(!permit.invoke(&owner,p.calls[failure],[]{return true;},[&](){++effects;return true;}));
     assert(effects==failure+(fail_guard?0u:1u));
    }
    auto throwing=ExpansionPermit::fixture(&owner,p);unsigned calls{};
    for(unsigned i=0;i<=failure;++i) {
     const bool ok=throwing.invoke(&owner,p.calls[i],[]{return true;},[&]()->bool{++calls;if(i==failure)throw std::runtime_error("native result uncertain");return true;});
     assert(ok==(i!=failure));
    }
    assert(throwing.failed()&&throwing.attempted()&&calls==failure+1);
   }
   for(unsigned field=0;field<6;++field) {
    auto permit=ExpansionPermit::fixture(&owner,p);auto wrong=p.calls[0];unsigned effects{};
    switch(field){case 0:wrong.function+=8;break;case 1:wrong.target+=8;break;case 2:wrong.argument+=8;break;case 3:++wrong.count;break;case 4:++wrong.index;break;default:wrong.name=L"foreign_name";}
    assert(!permit.invoke(&owner,wrong,[]{return true;},[&](){++effects;return true;}));assert(!effects&&permit.failed());
   }
   auto foreign_token=ExpansionPermit::fixture(&owner,p);unsigned effects{};
   assert(!foreign_token.invoke(&foreign,p.calls[0],[]{return true;},[&](){++effects;return true;}));assert(!effects);
   auto guard_throw=ExpansionPermit::fixture(&owner,p);unsigned guard_effects{};
   assert(!guard_throw.invoke(&owner,p.calls[0],[]()->bool{throw std::runtime_error("owner uncertain");},[&](){++guard_effects;return true;}));
   assert(!guard_effects&&guard_throw.failed()&&!guard_throw.attempted());
   auto guard_reentry=ExpansionPermit::fixture(&owner,p);unsigned reentry_effects{};
   assert(!guard_reentry.invoke(&owner,p.calls[0],[&](){
    assert(!guard_reentry.invoke(&owner,p.calls[0],[]{return true;},[&](){++reentry_effects;return true;}));
    return true;
   },[&](){++reentry_effects;return true;}));
   assert(!reentry_effects&&guard_reentry.failed()&&!guard_reentry.attempted());
   auto effect_reentry=ExpansionPermit::fixture(&owner,p);
   assert(!effect_reentry.invoke(&owner,p.calls[0],[]{return true;},[&](){
    ++reentry_effects;
    assert(!effect_reentry.invoke(&owner,p.calls[0],[]{return true;},[&](){++reentry_effects;return true;}));
    return true;
   }));
   assert(reentry_effects==1&&effect_reentry.failed()&&effect_reentry.attempted());
   auto reordered=ExpansionPermit::fixture(&owner,p);
   assert(!reordered.invoke(&owner,p.calls.back(),[]{return true;},[]{return true;}));
   auto moved_from=ExpansionPermit::fixture(&owner,p);auto moved=std::move(moved_from);
   assert(!moved_from.invoke(&owner,p.calls[0],[]{return true;},[]{return true;}));
   for(const auto& call:p.calls)assert(moved.invoke(&owner,call,[]{return true;},[]{return true;}));
   assert(moved.complete());assert(!moved.invoke(&owner,p.calls[0],[]{return true;},[]{return true;}));
  }
 }
 ExpansionState state=ExpansionState::prepared;
 {ExpansionFailureGuard failure{state};}assert(expansion_clone_proof_allowed(state));
 {ExpansionFailureGuard failure{state};state=ExpansionState::mutating;}assert(state==ExpansionState::uncertain&&!expansion_clone_proof_allowed(state));
 state=ExpansionState::mutating;
 try {ExpansionFailureGuard failure{state};throw 1;}catch(...){}assert(!expansion_clone_proof_allowed(state));
 state=ExpansionState::expanded;{ExpansionFailureGuard failure{state};}assert(expansion_clone_proof_allowed(state));
 auto bad=first();bad.calls[0].argument=1;assert(!valid_expansion_plan(bad));
 bad=second(first());bad.owned=bad.exposed;assert(!valid_expansion_plan(bad));
 std::cout<<"PASS production single-use cursor, every action guard/effect/throw fault, exact params/order/transport and partial-clone retention\n";
}
std::vector<unsigned char> bodies(const std::vector<ExpansionPlan>& players) {
 constexpr std::size_t stride=16+7*512;std::vector<unsigned char>b(24+stride*players.size());
 auto u32=[&](std::size_t at,unsigned v){std::memcpy(b.data()+at,&v,4);};
 std::uint64_t birth=456;u32(0,0x33504f42);u32(4,123);u32(8,unsigned(players.size()));std::memcpy(b.data()+16,&birth,8);
 for(unsigned n=0;n<players.size();++n) {
  const auto& p=players[n];const auto offset=24+n*stride;std::memcpy(b.data()+offset,&p.original,8);u32(offset+8,p.count);
  for(unsigned i=0;i<p.count;++i)for(unsigned j=0;j<p.items[i].size();++j){unsigned short w=static_cast<unsigned short>(p.items[i][j]);std::memcpy(b.data()+offset+16+i*512+j*2,&w,2);}
 }
 return b;
}
std::vector<unsigned char> body(const ExpansionPlan& p){return bodies({p});}
void write(const std::wstring& path,const std::vector<unsigned char>& bytes) {
 Handle f(CreateFileW(path.c_str(),GENERIC_WRITE,0,nullptr,CREATE_NEW,FILE_ATTRIBUTE_NORMAL,nullptr));DWORD done{};
 assert(f&&WriteFile(f.value,bytes.data(),DWORD(bytes.size()),&done,nullptr)&&done==bytes.size()&&FlushFileBuffers(f.value));
}
std::string journal(const std::wstring& prefix) {
 Handle f(CreateFileW((prefix+L".v2-intents").c_str(),GENERIC_READ,FILE_SHARE_READ|FILE_SHARE_WRITE,nullptr,OPEN_EXISTING,0,nullptr));
 std::vector<unsigned char> bytes;assert(f&&read_file(f.value,65536,bytes));return {bytes.begin(),bytes.end()};
}
void durable_batch_benchmark(const std::wstring& root,const std::string& body_a,const std::string& body_b) {
 // Compare identical aggregate bytes/readback, split only at record boundaries.
 auto lines=[](const std::string& text){std::vector<std::string> out;std::size_t at{};while(at<text.size()){auto end=text.find('\n',at);assert(end!=std::string::npos);out.push_back(text.substr(at,end-at+1));at=end+1;}return out;};
 const auto a=lines(body_a),b=lines(body_b);assert(a.size()==4&&b.size()==6);
 std::vector<std::string> old_one={a[0],a[1],a[2]+a[3],b[0]+b[1],b[2],b[3],b[4]+b[5]};
 std::vector<std::string> grouped_one={body_a,body_b};
 auto twice=[](const auto& input){auto out=input;out.insert(out.end(),input.begin(),input.end());return out;};
 const auto individual=twice(old_one),grouped=twice(grouped_one);assert(individual.size()==14&&grouped.size()==4);
 auto joined=[](const auto& chunks){std::string out;for(const auto& c:chunks)out+=c;return out;};
 assert(joined(individual)==joined(grouped));
 auto run=[&](const std::vector<std::string>& chunks,unsigned round,const wchar_t* lane){
  const auto path=root+L"\\io-"+lane+L"-"+std::to_wstring(round)+L".bin";
  Handle f(CreateFileW(path.c_str(),GENERIC_READ|GENERIC_WRITE,FILE_SHARE_READ,nullptr,CREATE_NEW,FILE_FLAG_OPEN_REPARSE_POINT|FILE_FLAG_WRITE_THROUGH,nullptr));
  StableId id{};assert(f&&file_id(f.value,false,id));std::uint64_t offset{};
  const auto start=std::chrono::steady_clock::now();
  for(const auto& chunk:chunks) {
   StableId before{},after{};assert(file_id(f.value,false,before)&&before==id);
   LARGE_INTEGER pos{};pos.QuadPart=LONGLONG(offset);DWORD wrote{},got{};std::string read(chunk.size(),'\0');
   assert(SetFilePointerEx(f.value,pos,nullptr,FILE_BEGIN)&&WriteFile(f.value,chunk.data(),DWORD(chunk.size()),&wrote,nullptr)&&wrote==chunk.size());
   assert(FlushFileBuffers(f.value)&&SetFilePointerEx(f.value,pos,nullptr,FILE_BEGIN)&&ReadFile(f.value,read.data(),DWORD(read.size()),&got,nullptr)&&got==read.size());
   assert(read==chunk&&file_id(f.value,false,after)&&after==id);offset+=chunk.size();
  }
  return std::chrono::duration<double,std::milli>(std::chrono::steady_clock::now()-start).count();
 };
 std::vector<double> old_ms,new_ms;
 for(unsigned round=0;round<10;++round) {
  if(round%2){new_ms.push_back(run(grouped,round,L"bundle"));old_ms.push_back(run(individual,round,L"individual"));}
  else {old_ms.push_back(run(individual,round,L"individual"));new_ms.push_back(run(grouped,round,L"bundle"));}
 }
 std::sort(old_ms.begin(),old_ms.end());std::sort(new_ms.begin(),new_ms.end());
 std::cout<<"BENCHMARK synthetic-only identicalBytes="<<joined(individual).size()<<" iterations=10 individualFlushes=14 bundleFlushes=4 medianIndividualMs="<<(old_ms[4]+old_ms[5])/2<<" medianBundleMs="<<(new_ms[4]+new_ms[5])/2<<" (not game performance)\n";
}
void validation_count_tests(const std::wstring& root,const TrustedBinding& binding) {
 auto a=first(),other=first();other.original+=8;other.clone+=8;other.calls=expansion_calls(other);
 const std::vector<ExpansionPlan> players={a,other};
 for(bool bundled:{false,true}) {
  const unsigned number=bundled?102:101;const auto prefix=root+L"\\Logs\\game-session\\nba2k27_saved_outfits_123_match"+std::to_wstring(number);
  Transport t;assert(t.open(binding,number));write(prefix+L".plan",bodies(players));write(prefix+L".trigger",{'S','A','V','E','D','_','O','U','T','F','I','T','S','_','V','1'});assert(t.consume());
  const auto plan_checks=t.pinned_plan_calls(),ordinary_checks=t.ordinary_calls(),flushes=t.flush_calls();unsigned effects{};
  for(const auto& p:players) {
   const auto b=second(p);
   if(bundled) {
    auto pa=t.expansion_bundle(p);assert(pa);
    for(const auto& call:p.calls)assert(pa->invoke(&t,call,[&](){return t.pinned_plan();},[&](){++effects;return true;}));
    auto pb=t.expansion_bundle(b,&*pa);assert(pb);
    for(const auto& call:b.calls)assert(pb->invoke(&t,call,[&](){return t.pinned_plan();},[&](){++effects;return true;}));
   }else {
    Resources resource{};resource.clone=p.clone;resource.clone_proved=true;const Choice choice{p.original,p.items};
    assert(t.event(Phase::expand_intent,resource,choice));
    for(unsigned i=0;i<p.calls.size()+b.calls.size();++i){assert(t.pinned_plan());assert(t.event(Phase::expand_intent,resource,choice));++effects;}
   }
  }
  assert(effects==12);
  const auto plans=t.pinned_plan_calls()-plan_checks,ordinary=t.ordinary_calls()-ordinary_checks,flush=t.flush_calls()-flushes;
  assert(plans==(bundled?16u:26u)&&ordinary==(bundled?24u:54u)&&flush==(bundled?4u:14u));
  std::cout<<"VALIDATION synthetic-only bundle="<<bundled<<" sameFreshEffectGuards=12 pinnedPlanChecks="<<plans<<" ownedFileDirectoryChecks="<<ordinary<<" flushes="<<flush<<"\n";
 }
}
void transport_tests(const std::wstring& root) {
 for(const auto* child:{L"App",L"Data",L"Library",L"Logs",L"Logs\\game-session"})assert(CreateDirectoryW((root+L"\\"+child).c_str(),nullptr));
 TrustedBinding b{};b.game_pid=123;b.game_birth=456;b.owner_pid=789;b.owner_birth=901;b.session=std::string(48,'a');b.portable_root=root;b.owner_executable=root+L"\\App\\fixture.exe";
 const auto prefix_base=root+L"\\Logs\\game-session\\nba2k27_saved_outfits_123";
 unsigned number{};
 for(unsigned count:{3u,7u})for(unsigned length:{8u,255u}) {
  ++number;const auto prefix=prefix_base+(number==1?L"":L"_match"+std::to_wstring(number));
  const auto a=first(count,length),sb=second(a);Transport t;assert(t.open(b,number));write(prefix+L".plan",body(a));
  write(prefix+L".trigger",{'S','A','V','E','D','_','O','U','T','F','I','T','S','_','V','1'});assert(t.consume());
  const auto initial=t.flush_calls();auto pa=t.expansion_bundle(a);assert(pa&&t.flush_calls()==initial+1);
  assert(!t.expansion_bundle(a));assert(!t.expansion_bundle(sb,&*pa));
  unsigned calls{};
  for(const auto& call:a.calls)assert(pa->invoke(&t,call,[]{return true;},[&](){assert(journal(prefix).find("END_EXPANSION_BUNDLE=A TICK_MS=")!=std::string::npos);++calls;return true;}));
  for(unsigned field=0;field<12;++field) {
   auto changed=sb;
   switch(field){case 0:changed.original+=8;break;case 1:changed.clone+=8;break;case 2:changed.owner+=8;break;case 3:changed.heap+=8;break;case 4:changed.actor+=8;break;case 5:changed.source+=8;break;case 6:changed.row+=8;break;case 7:changed.manager+=8;break;case 8:changed.loaded+=8;break;case 9:changed.state+=8;break;case 10:changed.items[0]=L"foreign_item";break;default:changed.base+=0x10000;}
   changed.calls=expansion_calls(changed);
   assert(!t.expansion_bundle(changed,&*pa));
  }
  auto pb=t.expansion_bundle(sb,&*pa);assert(pb&&t.flush_calls()==initial+2);assert(!t.expansion_bundle(sb,&*pa));
  for(const auto& call:sb.calls)assert(pb->invoke(&t,call,[]{return true;},[&](){const auto text=journal(prefix);assert(text.find("END_EXPANSION_BUNDLE=B TICK_MS=")!=std::string::npos);assert(text.find("PLAN_SHA256=")!=std::string::npos);++calls;return true;}));
  assert(calls==count+3&&pa->complete()&&pb->complete());
  assert(t.flush_calls()==initial+2);
  const auto text=journal(prefix);
  assert(text.find("NAME_LENGTH="+std::to_string(length)+" NAME=")!=std::string::npos);
  if(count==3 && length==8) {
   const auto start=text.find("EXPANSION_BUNDLE=1 STAGE=A"),mid=text.find("EXPANSION_BUNDLE=1 STAGE=B");
   assert(start!=std::string::npos&&mid>start);
   durable_batch_benchmark(root,text.substr(start,mid-start),text.substr(mid));
  }
 }
 // Each actual disk failure is tested before A or after Release/Init before B.
 for(unsigned stage=0;stage<2;++stage)for(unsigned fault=1;fault<=3;++fault) {
  ++number;const auto prefix=prefix_base+L"_match"+std::to_wstring(number);const auto a=first(),sb=second(a);
  Transport t;assert(t.open(b,number));write(prefix+L".plan",body(a));write(prefix+L".trigger",{'S','A','V','E','D','_','O','U','T','F','I','T','S','_','V','1'});assert(t.consume());
  unsigned effects{};ExpansionState state=ExpansionState::prepared;
  {
   ExpansionFailureGuard failure{state};
   std::optional<ExpansionPermit> pa;
   if(stage) {
    auto minted=t.expansion_bundle(a);assert(minted);pa.emplace(std::move(*minted));
    for(const auto& call:a.calls)assert(pa->invoke(&t,call,[]{return true;},[&](){state=ExpansionState::mutating;++effects;return true;}));
   }
   t.inject_append_fault(fault);auto failed=t.expansion_bundle(stage?sb:a,stage?&*pa:nullptr);assert(!failed&&t.failed());
   assert(!t.expansion_bundle(stage?sb:a,stage?&*pa:nullptr));
  }
  assert(effects==(stage?2u:0u));
  if(stage)assert(state==ExpansionState::uncertain&&!expansion_clone_proof_allowed(state));
 }
 // Crash/restart cannot turn a durable intent into a live permit or reopen it.
 ++number;const auto prefix=prefix_base+L"_match"+std::to_wstring(number);const auto a=first();
 {
  Transport crashed;assert(crashed.open(b,number));write(prefix+L".plan",body(a));write(prefix+L".trigger",{'S','A','V','E','D','_','O','U','T','F','I','T','S','_','V','1'});assert(crashed.consume());auto unused=crashed.expansion_bundle(a);assert(unused&&!unused->attempted());
 }
 Transport restarted;assert(!restarted.open(b,number));
 assert(journal(prefix).find("EXPANSION_BUNDLE=1 STAGE=A")!=std::string::npos);
 validation_count_tests(root,b);
 std::cout<<"PASS real owned Transport bundle flush/readback before effects, max7x255 names, A/B faults and crash/reopen refusal;7/11 expansion flushes ->2\n";
}
}
int wmain(int argc,wchar_t** argv) {assert(argc==2);cursor_tests();transport_tests(argv[1]);}
