#include "wnba_blacktop_coordinator_core.hpp"
#include <algorithm>
#include <cassert>
#include <cstdio>
#include <string>
#include <stdexcept>
#include <vector>
namespace c=wnba_blacktop_category;
namespace r=wnba_blacktop_roster;
using wnba_blacktop_coordinator::Coordinator;
static const c::Binding binding{123,456,std::string(48,'a')};
static constexpr std::uintptr_t base=0x140000000,label=0xACFC3D0,bridge=base+0x40000000;
static c::Input home() {
 c::Input in{};in.game=binding;in.process=c::Process::live;
 in.owned_offline=in.source_trial_verified=in.explicit_category_consent=in.stable=true;
 in.current=in.alternate=0x6509837B;in.stage=1;return in;
}
static c::Input blacktop(unsigned stage,bool mixed=false) {
 auto in=home();in.current=in.alternate=0x92DF06DE;in.stage=stage;
 in.observed_team_count=mixed?403:370;
 if(mixed) {
  in.deep_mixed=true;in.roster_generation=111;
  for(std::uint16_t id=300;id<=317;++id)if(id<312 || id>314)in.clubs.push_back(id);
 }
 return in;
}
struct Journal {
 std::uint64_t used{};unsigned calls{},fail_at{};
 bool take() {
  if(!wnba_blacktop_journal::available(used,1))return false;
  ++used;return ++calls!=fail_at;
 }
};
struct RosterHost {
 Journal* journal{};
 std::uint64_t clock{1000};
 bool publication_budget()const{return !journal || wnba_blacktop_journal::roster(journal->used);}
 std::uint64_t monotonic_ms()const{return clock;}
 std::vector<std::string>& trace;
 r::Endpoint endpoint{r::Endpoint::stock};
 bool transition{true},menu{},permission{},retained{true},sticky{},conflict{},throw_transition{};
 unsigned intents{},writes{},applied{},queries{},fail_intent{},fail_applied{};
 bool pending{};
 explicit RosterHost(std::vector<std::string>& events):trace(events){}
 bool transition_ready() {++queries;if(throw_transition)throw std::runtime_error("observation failed");return transition;}
 bool checkpoint(const c::Binding& b,bool publishing) {
  return b==binding && retained && !sticky && (!publishing || (permission && menu && transition));
 }
 bool read_endpoint(r::Endpoint& out) {if(sticky)return false;out=endpoint;return true;}
 bool journal_intent(const c::Binding& b,r::Endpoint,r::Endpoint) {
  assert(b==binding);if(journal && !journal->take())return false;++intents;if(pending || intents==fail_intent)return false;pending=true;return true;
 }
 bool write_endpoint(r::Endpoint from,r::Endpoint to) {
  assert(pending);++writes;
  if(endpoint!=from)return false;
  endpoint=to;
  if(conflict) {sticky=true;trace.push_back("foreign-callback");return false;}
  trace.push_back(to==r::Endpoint::general_loader?"callback-general":"callback-stock");return true;
 }
 bool journal_applied(const c::Binding& b,r::Endpoint,r::Endpoint) {
  assert(b==binding && pending);if(journal && !journal->take())return false;++applied;if(applied==fail_applied)return false;pending=false;return true;
 }
};
struct CategoryHost {
 Journal* journal{};
 std::vector<std::string>& trace;c::Route route{};c::Code code{};
 bool departed{},permission{},selection{},retained{true},allocated{},sealed{},sticky{},pending{};
 unsigned intents{},writes{},applied{},allocations{},fail_intent{},fail_applied{},conflict_write{};
 explicit CategoryHost(std::vector<std::string>& events):trace(events){}
 std::uint64_t now=100;bool preparation_bound{};std::uint64_t preparation_start{};unsigned defer_proofs{},initializations{};
 std::uint64_t monotonic_ms()const{return now;}
 bool bind_preparation_start(std::uint64_t start){if(preparation_bound)return start==preparation_start;preparation_bound=true;preparation_start=start;return true;}
 bool passive_preparation_scope()const{return retained&&!sticky&&permission;}
 c::PreinitializeProof preallocation_proof()const{return c::PreinitializeProof::failed;}
 c::PreinitializeProof preinitialize_proof(bool passed){if(defer_proofs){--defer_proofs;return c::PreinitializeProof::defer;}return passed&&retained&&!sticky?c::PreinitializeProof::exact:c::PreinitializeProof::failed;}
 bool checkpoint(const c::Binding& b,bool publishing) {
  return b==binding && retained && !sticky && (!publishing || (permission && selection));
 }
 bool selection_departed()const{return departed;}
 const char* reason()const{return "Injected uncertain active category checkpoint";}
 bool publication_budget(bool bridge_ready)const{return !journal || wnba_blacktop_journal::category(journal->used,bridge_ready);}
 bool read_route(c::Route& out) {if(sticky)return false;out=route;return true;}
 bool journal_bridge_intent(const c::Binding& b) {assert(b==binding);if(journal && !journal->take())return false;trace.push_back("bridge-intent");return true;}
 std::uintptr_t allocate_bridge_near(std::uintptr_t at,std::uintptr_t table,std::size_t bytes) {
  assert(at==base && table==c::kTable && bytes==10);allocated=true;++allocations;return bridge;
 }
 void retain_bridge(std::uintptr_t at) {assert(at==bridge && allocated);}
 bool initialize_bridge_rw(std::uintptr_t at,const c::Code& value) {++initializations;assert(at==bridge && allocated);if(journal && !journal->take())return false;code=value;return true;}
 bool seal_bridge_rx(std::uintptr_t at,const c::Code& value) {assert(at==bridge && value==code);if(journal && !journal->take())return false;sealed=true;return true;}
 bool bridge_is_private_rx(std::uintptr_t at,const c::Code& value) {return at==bridge && allocated && sealed && value==code;}
 bool journal_bridge_ready(const c::Binding& b,std::uintptr_t at,const c::Code& value) {
  assert(b==binding && at==bridge && value==code);return !journal || journal->take();
 }
 bool journal_change(const c::Binding& b,const c::Change&) {
  assert(b==binding);if(journal && !journal->take())return false;++intents;if(pending || intents==fail_intent)return false;pending=true;return true;
 }
 bool write_change(const c::Change& change) {
  assert(pending);++writes;
  if(change.word==c::Word::label) {
   assert(change.at==base+label+8 && route.label==change.before);route.label=change.after;
  } else {
   assert(change.at==base+c::kEntry && std::uint32_t(route.dispatch)==change.before);route.dispatch=std::int32_t(change.after);
  }
  if(writes==conflict_write) {sticky=true;trace.push_back("foreign-category");return false;}
  trace.push_back(change.word==c::Word::label?
   (change.after==c::kWomen?"label-women":"label-euro"):
   (std::int32_t(change.after)==c::kStockDispatch?"dispatch-stock":"dispatch-bridge"));return true;
 }
 bool journal_applied(const c::Binding& b,const c::Change&) {
  assert(b==binding && pending);if(journal && !journal->take())return false;++applied;if(applied==fail_applied)return false;pending=false;return true;
 }
};
struct Fixture {
 std::vector<std::string> trace;RosterHost rh{trace};CategoryHost ch{trace};Coordinator coordinator;
 explicit Fixture(std::uintptr_t resolved=label):coordinator(binding,base,resolved){}
 auto tick(const c::Input& in) {
  const bool permitted=in.owned_offline && in.source_trial_verified && in.explicit_category_consent;
  r::Input ri{};ri.stable=in.stable;ri.current=in.current;ri.alternate=in.alternate;
  ri.stage=in.stage;ri.active=in.active;ri.kind=in.kind;
  rh.menu=r::launch_menu(ri);rh.permission=ch.permission=permitted;ch.selection=c::ready_selection(in);
  return coordinator.tick(in,rh,ch);
 }
 void active() {assert(tick(home()).roster_result==r::Result::armed);assert(tick(blacktop(3,true)).category_result==c::Result::published);}
};
static std::size_t at(const std::vector<std::string>& trace,const char* event) {
 const auto found=std::find(trace.begin(),trace.end(),event);assert(found!=trace.end());return std::size_t(found-trace.begin());
}
static void first_entry_ordering_and_reentry() {
 Fixture f;assert(f.tick(home()).roster_result==r::Result::armed);
 auto gap=home();gap.stable=false;assert(f.tick(gap).roster_result==r::Result::observed);
 gap.stable=true;gap.alternate=0x92DF06DE;assert(f.tick(gap).roster_result==r::Result::observed);
 assert(f.tick(blacktop(0)).roster_result==r::Result::observed);
 assert(f.tick(blacktop(2)).roster_result==r::Result::observed);
 const auto stock=f.tick(blacktop(3));assert(stock.roster_result==r::Result::restored && stock.category_result==c::Result::waiting);
 assert(f.rh.endpoint==r::Endpoint::stock && f.ch.writes==0 && !f.ch.allocated);
 assert(f.tick(blacktop(3,true)).category_result==c::Result::published);
 assert(at(f.trace,"callback-stock")<at(f.trace,"bridge-intent"));
 assert(at(f.trace,"callback-stock")<at(f.trace,"label-women"));
 assert(at(f.trace,"label-women")<at(f.trace,"dispatch-bridge"));
 assert(f.tick(home()).category_result==c::Result::restored);
 assert(at(f.trace,"dispatch-stock")<at(f.trace,"label-euro"));
 assert(f.rh.endpoint==r::Endpoint::general_loader && f.ch.route==c::Route{});
 const auto reentry=f.tick(blacktop(3,true));
 assert(reentry.roster_result==r::Result::restored && reentry.category_result==c::Result::published);
 assert(f.ch.allocations==1 && f.ch.allocated && f.ch.sealed);
}
static void retained_zero_wait_keeps_callback_order_and_cleans_on_expiry() {
 Fixture f;f.ch.defer_proofs=1;
 assert(f.tick(home()).roster_result==r::Result::armed);
 const auto pending=f.tick(blacktop(3,true));
 assert(pending.roster_result==r::Result::restored&&pending.category_result==c::Result::waiting&&!pending.held);
 assert(f.ch.allocations==1&&f.ch.initializations==0&&f.ch.writes==0&&f.rh.endpoint==r::Endpoint::stock);
 assert(f.tick(blacktop(3,true)).category_result==c::Result::published);
 assert(f.ch.allocations==1&&f.ch.initializations==1&&at(f.trace,"callback-stock")<at(f.trace,"label-women"));
 Fixture deadline;deadline.ch.defer_proofs=1;
 assert(deadline.tick(home()).roster_result==r::Result::armed);
 assert(deadline.tick(blacktop(3,true)).category_result==c::Result::waiting);
 const auto rearm=deadline.tick(home());assert(!rearm.held&&deadline.rh.endpoint==r::Endpoint::general_loader);
 deadline.ch.now+=10000;
 const auto expired=deadline.tick(home());assert(expired.held&&deadline.rh.endpoint==r::Endpoint::stock);
 assert(deadline.ch.allocations==1&&deadline.ch.initializations==0&&deadline.ch.writes==0);
}
static void intermediate_menu_keeps_loader_until_actual_picker() {
 Fixture f;assert(f.tick(home()).roster_result==r::Result::armed);
 auto intermediate=home();intermediate.current=intermediate.alternate=0x4FDE5DCD;intermediate.stage=0;
 f.rh.clock+=1000;
 assert(f.tick(intermediate).roster_result==r::Result::observed && f.rh.writes==1 && f.ch.writes==0);
 intermediate.current=intermediate.alternate=0x11111111;intermediate.stable=false;
 f.rh.clock+=1000;
 assert(f.tick(intermediate).roster_result==r::Result::observed && f.rh.writes==1);
 const auto entered=f.tick(blacktop(3,true));
 assert(entered.roster_result==r::Result::restored && entered.category_result==c::Result::published);
 assert(at(f.trace,"callback-stock")<at(f.trace,"label-women"));
 Fixture deadline;assert(deadline.tick(home()).roster_result==r::Result::armed);
 deadline.rh.clock=61000;
 assert(deadline.tick(intermediate).roster_result==r::Result::restored && deadline.ch.writes==0);
 Fixture backward;assert(backward.tick(home()).roster_result==r::Result::armed);
 backward.rh.clock=999;
 assert(backward.tick(intermediate).held && backward.rh.endpoint==r::Endpoint::stock && backward.ch.writes==0);
}
static void both_off_and_already_mixed() {
 Fixture off;auto in=home();in.source_trial_verified=in.explicit_category_consent=false;
 assert(!off.tick(in).held && off.trace.empty() && off.rh.writes==0 && off.ch.writes==0);
 in=blacktop(3,true);in.source_trial_verified=in.explicit_category_consent=false;
 assert(!off.tick(in).held && off.trace.empty() && off.rh.writes==0 && off.ch.writes==0);
 assert(!off.tick(blacktop(0)).held); // New authorized input may arm only at Home.
 Fixture mixed;mixed.rh.transition=false;
 assert(mixed.tick(home()).roster_result==r::Result::waiting && mixed.rh.writes==0);
 assert(mixed.tick(blacktop(3,true)).category_result==c::Result::published);
 assert(mixed.rh.writes==0 && mixed.ch.writes==2);
}
static void owner_loss_and_sticky_holds() {
 for(bool category_active: {false,true}) {
  Fixture f;if(category_active)f.active();else assert(f.tick(home()).roster_result==r::Result::armed);
  auto lost=category_active?blacktop(3,true):home();lost.owned_offline=false;lost.stable=false;
  assert(f.tick(lost).held && f.rh.endpoint==r::Endpoint::stock && f.ch.route==c::Route{});
  const auto writes=f.rh.writes+f.ch.writes,queries=f.rh.queries;
  assert(f.tick(home()).held && f.rh.writes+f.ch.writes==writes && f.rh.queries==queries);
 }
 Fixture unknown;unknown.active();auto lost=blacktop(3,true);lost.process=c::Process::unknown;
 const auto writes=unknown.ch.writes;
 assert(unknown.tick(lost).held && unknown.ch.writes==writes && unknown.ch.allocated && unknown.ch.sealed);
}
static void coupled_fault_cleanup() {
 // Roster publication fails on Home exit while an earlier category is active:
 // the shared coordinator must still restore that exact owned category route.
 Fixture f;f.active();f.rh.fail_intent=f.rh.intents+1;
 assert(f.tick(home()).held && f.ch.route==c::Route{} && !f.coordinator.category_owned());
 assert(f.rh.endpoint==r::Endpoint::stock && f.ch.allocated && f.ch.sealed);
 // Category refusal after initial roster arming conditionally restores loader.
 Fixture invalid_label(0);assert(invalid_label.tick(home()).held);
 assert(invalid_label.rh.endpoint==r::Endpoint::stock && invalid_label.rh.writes==2);
 // Callback restoration must succeed before any category write is attempted.
 Fixture restore;assert(restore.tick(home()).roster_result==r::Result::armed);
 restore.rh.fail_intent=2;
 assert(restore.tick(blacktop(3,true)).held && restore.ch.writes==0 && !restore.ch.allocated);
 assert(restore.coordinator.roster_owned());
 // Partial category receipt preserves responsibility/resources, blocks retries.
 Fixture partial;assert(partial.tick(home()).roster_result==r::Result::armed);partial.ch.fail_applied=1;
 assert(partial.tick(blacktop(3,true)).held && partial.rh.endpoint==r::Endpoint::stock);
 assert(partial.coordinator.category_owned() && partial.ch.allocated && partial.ch.sealed);
 const auto writes=partial.ch.writes;assert(partial.tick(home()).held && partial.ch.writes==writes);
 // Identical foreign CAS targets cannot become controller-owned values.
 Fixture foreign_callback;foreign_callback.rh.conflict=true;
 assert(foreign_callback.tick(home()).held && !foreign_callback.coordinator.roster_owned());
 assert(foreign_callback.rh.applied==0 && foreign_callback.ch.writes==0);
 Fixture foreign_category;assert(foreign_category.tick(home()).roster_result==r::Result::armed);
 foreign_category.ch.conflict_write=1;
 assert(foreign_category.tick(blacktop(3,true)).held && !foreign_category.coordinator.category_owned());
 assert(foreign_category.ch.applied==0 && foreign_category.ch.allocated && foreign_category.ch.sealed);
 Fixture foreign_callback_reentry;foreign_callback_reentry.active();foreign_callback_reentry.rh.conflict=true;
 assert(foreign_callback_reentry.tick(home()).held && foreign_callback_reentry.ch.route==c::Route{});
 assert(!foreign_callback_reentry.coordinator.roster_owned() && !foreign_callback_reentry.coordinator.category_owned());
 // Extra observation failures stop permissions and conditionally clean only
 // exactly owned routes; neither host is released or admitted again.
 for(bool category_active: {false,true}) {
  Fixture observed;if(category_active)observed.active();else assert(observed.tick(home()).roster_result==r::Result::armed);
  observed.rh.throw_transition=true;
  assert(observed.tick(home()).held && observed.rh.endpoint==r::Endpoint::stock && observed.ch.route==c::Route{});
  const auto queries=observed.rh.queries;
  assert(observed.tick(home()).held && observed.rh.queries==queries);
 }
}
static void selection_checkpoint_departure_reentry() {
 Fixture f;f.active();f.ch.departed=true;
 // Old input still selection; executor has fresh stable offline exit proof.
 f.ch.selection=false;
 auto input=blacktop(3,true);
 auto result=f.coordinator.tick(input,f.rh,f.ch);
 assert(!result.held && result.category_result==c::Result::restored && f.ch.route==c::Route{});
 f.ch.departed=false;
 assert(f.tick(home()).roster_result==r::Result::armed);
 result=f.tick(blacktop(3,true));
 assert(!result.held && result.category_result==c::Result::published && f.ch.allocations==1);
}
static void menu_cleanup_precedes_fresh_loader_arm() {
 for(bool quick: {false,true}) {
  Fixture f;f.active();const auto before=f.trace.size();auto menu=home();
  if(quick)menu.current=menu.alternate=0x079964C0;
  const auto result=f.tick(menu);
  assert(!result.held && result.category_result==c::Result::restored && result.roster_result==r::Result::armed);
  std::vector<std::string> tail(f.trace.begin()+before,f.trace.end());
  assert(at(tail,"dispatch-stock")<at(tail,"label-euro") && at(tail,"label-euro")<at(tail,"callback-general"));
 }
 for(unsigned fault=0;fault<4;++fault) {
  Fixture f;f.active();const auto callback_writes=f.rh.writes;
  if(fault==0)f.ch.fail_intent=f.ch.intents+1;
  if(fault==1)f.ch.fail_applied=f.ch.applied+1;
  if(fault==2)f.ch.route.dispatch=123;
  if(fault==3)f.ch.retained=false;
  assert(f.tick(home()).held && f.rh.writes==callback_writes && f.rh.endpoint==r::Endpoint::stock);
  assert(f.ch.allocated && f.ch.sealed);
  const auto writes=f.ch.writes;
  assert(f.tick(home()).held && f.ch.writes==writes && f.rh.writes==callback_writes);
 }
 // An unstable Home observation cannot authorize new loader publication.
 Fixture unstable;unstable.active();const auto writes=unstable.rh.writes;
 auto gap=home();gap.stable=false;
 assert(!unstable.tick(gap).held && unstable.rh.writes==writes && unstable.ch.route==c::Route{});
}
static c::Input match_mode(unsigned n=0) {
 auto in=home();constexpr unsigned modes[]={0xB4D37B22,0x3EEF98B9,0x7C602DF8,0x50BAD898};
 in.current=in.alternate=modes[n%4];return in;
}
static void typed_limit_notice_waits_for_clean_ownership() {
 using namespace wnba_blacktop_coordinator;
 Tick roster{};roster.roster_result=r::Result::limit_reached;
 Tick category{};category.category_result=c::Result::limit_reached;
 assert(limit_source(roster)==LimitSource::roster && limit_source(category)==LimitSource::category);
 auto both=roster;both.category_result=c::Result::limit_reached;assert(limit_source(both)==LimitSource::both);
 LimitNotice notice;
 assert(!notice.observe(Tick{},false,false));
 assert(!notice.observe(roster,true,false));
 assert(!notice.observe(category,false,true));
 // Later ticks need not repeat the typed result: pending ownership is sticky.
 assert(!notice.observe(Tick{},true,true));
 assert(notice.observe(Tick{},false,false));
 assert(std::string(notice.detail()).find("roster and category")!=std::string::npos);
 for(unsigned n=0;n<100;++n)assert(!notice.observe(both,false,false));
 LimitNotice held;auto failure=both;failure.held=true;
 assert(!held.observe(failure,false,false));
 assert(!held.observe(Tick{},false,false));
 // Real combined session: no publication, journal use, or writes after refusal.
 Journal exhausted{wnba_blacktop_journal::kCapacity};Fixture f;
 f.rh.journal=f.ch.journal=&exhausted;LimitNotice actual;unsigned notices{};
 for(unsigned n=0;n<100;++n) {
  const auto tick=f.tick(n%2?blacktop(3,true):home());
  assert(!tick.held && !f.coordinator.held());
  notices+=actual.observe(tick,f.coordinator.roster_owned(),f.coordinator.category_owned());
 }
 assert(notices==1 && !f.rh.writes && !f.ch.writes && !f.ch.allocations && !exhausted.calls);
}
static void shared_journal_capacity_reserves_clean_exit() {
 constexpr auto cap=wnba_blacktop_journal::kCapacity;
 static_assert(!wnba_blacktop_journal::available(~std::uint64_t{},1));
 for(unsigned remaining=0;remaining<=13;++remaining) {
  Journal journal{cap-remaining};Fixture roster;
  roster.rh.journal=roster.ch.journal=&journal;
  const auto armed=roster.tick(home());
  assert(!armed.held && (armed.roster_result==r::Result::armed)==(remaining>=4));
  if(remaining>=4)assert(!roster.tick(blacktop(3)).held && roster.rh.endpoint==r::Endpoint::stock);
  else assert(roster.rh.writes==0 && armed.roster_result==r::Result::limit_reached);
  assert(journal.used<=cap);
  Journal first_journal{cap-remaining};Fixture first;
  first.rh.journal=first.ch.journal=&first_journal;
  const auto published=first.tick(blacktop(3,true));
  assert(!published.held && (published.category_result==c::Result::published)==(remaining>=12));
  if(remaining>=12)assert(!first.tick(match_mode()).held && first.ch.route==c::Route{});
  else assert(!first.ch.allocated && first.ch.writes==0 && published.category_result==c::Result::limit_reached);
  assert(first_journal.used<=cap);
  Fixture retained;retained.active();assert(!retained.tick(match_mode()).held);
  Journal next_journal{cap-remaining};retained.rh.journal=retained.ch.journal=&next_journal;
  const auto next=retained.tick(blacktop(3,true));
  assert(!next.held && (next.category_result==c::Result::published)==(remaining>=8));
  if(remaining>=8)assert(!retained.tick(match_mode()).held && retained.ch.route==c::Route{});
  else assert(retained.ch.route==c::Route{} && next.category_result==c::Result::limit_reached);
  assert(next_journal.used<=cap && retained.ch.allocations==1);
 }
 // Exhaust the real shared256-record accounting over many complete cycles.
 // A pre-effect budget refusal must leave stock routes, never strand cleanup.
 Journal journal;Fixture f;f.rh.journal=f.ch.journal=&journal;
 unsigned published{};
 for(unsigned cycle=0;cycle<100;++cycle) {
  assert(!f.tick(home()).held);
  const auto entered=f.tick(blacktop(3,true));assert(!entered.held);
  if(entered.category_result==c::Result::published)++published;
  assert(!f.tick(match_mode(cycle)).held && f.ch.route==c::Route{} && f.rh.endpoint==r::Endpoint::stock);
 }
 assert(published>10 && published<30 && journal.used<=cap && f.ch.allocations==1);
 std::printf("Shared256-record journal: %u full category entries, clean stock at exhaustion (%llu records)\n",published,static_cast<unsigned long long>(journal.used));
 // Each failed forward record consumes capacity but must not trigger a retry.
 for(unsigned fault=1;fault<=8;++fault) {
  Journal failed{cap-12,0,fault};Fixture bad;bad.rh.journal=bad.ch.journal=&failed;
  const auto result=bad.tick(blacktop(3,true));assert(result.held && failed.used<=cap);
  const auto writes=bad.ch.writes;
  assert(bad.tick(home()).held && bad.ch.writes==writes && failed.used<=cap);
 }
}
static void repeated_match_and_reentry_model() {
 Fixture f;
 for(unsigned cycle=0;cycle<512;++cycle) {
  const auto entered=f.tick(home());assert(!entered.held);
  auto intermediate=home();intermediate.current=intermediate.alternate=0x4FDE5DCD;
  f.rh.clock+=100;assert(!f.tick(intermediate).held);
  auto picker=blacktop(3,true);picker.roster_generation=cycle+100;
  assert(f.tick(picker).category_result==c::Result::published);
  const auto writes=f.ch.writes;
  for(unsigned n=0;n<16;++n)assert(f.tick(picker).category_result==c::Result::observed && f.ch.writes==writes);
  assert(f.tick(match_mode(cycle)).category_result==c::Result::restored);
  for(unsigned n=0;n<8;++n)assert(!f.tick(match_mode(cycle)).held && f.ch.route==c::Route{});
  assert(f.ch.allocations==1 && f.rh.endpoint==r::Endpoint::stock);
 }
}
int main() {
 retained_zero_wait_keeps_callback_order_and_cleans_on_expiry();
 typed_limit_notice_waits_for_clean_ownership();shared_journal_capacity_reserves_clean_exit();repeated_match_and_reentry_model();menu_cleanup_precedes_fresh_loader_arm();selection_checkpoint_departure_reentry();first_entry_ordering_and_reentry();intermediate_menu_keeps_loader_until_actual_picker();both_off_and_already_mixed();owner_loss_and_sticky_holds();coupled_fault_cleanup();
 std::puts("Shared production Blacktop roster/category coordinator lifecycle tests passed");
}
