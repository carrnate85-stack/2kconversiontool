#include "wnba_blacktop_roster_core.hpp"
#include <cassert>
#include <cstdio>
#include <stdexcept>
#include <string>
#include <vector>
using namespace wnba_blacktop_roster;
static const Binding game{123,456,std::string(48,'a')};
static Input home() { return {game,Process::live,true,true,true,0x6509837B,0x6509837B,1,0,0}; }
static Input blacktop(unsigned stage) { auto in=home();in.current=in.alternate=0x92DF06DE;in.stage=stage;return in; }
struct Host {
 std::uint64_t clock{1000},intent_elapsed{};bool clock_failure{};
 bool budget=true;
 bool publication_budget()const{return budget;}
 std::uint64_t monotonic_ms()const {if(clock_failure)throw std::runtime_error("clock unavailable");return clock;}
 Endpoint endpoint{Endpoint::stock};
 bool publish_allowed{true},restore_allowed{true},intent_ok{true},applied_ok{true};
 bool write_ok{true},effect{true},fail_readback{},throw_write{},cas_conflict{};
 unsigned checks{},reads{},writes{},intents{},applied{};
 unsigned fail_checkpoint_at{},foreign_checkpoint_at{};
 std::vector<std::string> events;
 bool checkpoint(const Binding& b,bool publishing) {
  assert(b==game);++checks;events.push_back(publishing?"publish-check":"retained-check");
  if(checks==foreign_checkpoint_at)endpoint=Endpoint::foreign;
  return checks!=fail_checkpoint_at && (publishing?publish_allowed:restore_allowed);
 }
 bool read_endpoint(Endpoint& out) {
  ++reads;events.push_back("read");
  if(fail_readback && writes)return false;
  out=endpoint;return true;
 }
 bool journal_intent(const Binding& b,Endpoint,Endpoint) {
  assert(b==game);++intents;events.push_back("intent");clock+=intent_elapsed;return intent_ok;
 }
 bool write_endpoint(Endpoint from,Endpoint to) {
  ++writes;events.push_back("write");
  assert(intents>writes-1); // Every write attempt has a prior durable intent.
  if(cas_conflict) { endpoint=to;fail_readback=true;return false; }
  if(endpoint!=from)return false;
  if(effect)endpoint=to;
  if(throw_write)throw std::runtime_error("write exception");
  return write_ok;
 }
 bool journal_applied(const Binding& b,Endpoint,Endpoint) {
  assert(b==game);++applied;events.push_back("applied");return applied_ok;
 }
};
static void admission_and_boundaries() {
 {
  auto in=home();in.transition_ready=false;Host h;Session s(game);
  assert(s.tick(in,h)==Result::waiting && h.checks==0 && h.reads==0 && h.writes==0);
 }
 for(auto input: {home(),blacktop(0),blacktop(1),blacktop(2),blacktop(3)}) {
  input.permission=false;Host h;Session s(game);
  assert(s.tick(input,h)==Result::waiting && h.checks==0 && h.reads==0 && h.writes==0);
 }
 for(unsigned n=0;n<9;++n) {
  auto in=home();
  switch(n) {
   case 0:in.stable=false;break;
   case 1:in.alternate^=1;break;
   case 2:in.active=1;break;
   case 3:in.kind=1;break;
   case 4:in.stage=2;break;
   case 5:in.current=in.alternate=0xF54D5D12;break; // Never arm a Freestyle sibling route.
   case 6:in.current=in.alternate=0x92DF06DE;in.stage=0;break;
   case 7:in.current=in.alternate=0x079964C0;in.stage=0;break;
   default:in.current=in.alternate=0x079964C0;in.stage=2;break;
  }
  Host h;Session s(game);assert(s.tick(in,h)==Result::waiting && h.writes==0);
 }
 for(auto in: {home(),[] {auto in=home();in.stage=0;return in;}(),[] {auto in=home();in.current=in.alternate=0x079964C0;return in;}()}) {
  Host h;Session s(game);assert(s.tick(in,h)==Result::armed);
  assert(s.owned() && h.endpoint==Endpoint::general_loader && h.writes==1 && h.applied==1);
  assert(h.events==std::vector<std::string>({"publish-check","read","read","intent","publish-check","read","read","write","read","read","applied"}));
 }
}
static void lifecycle_and_reentry() {
 Host h;Session s(game);assert(s.tick(home(),h)==Result::armed);
 assert(s.tick(home(),h)==Result::observed);
 for(unsigned stage=0;stage<3;++stage)assert(s.tick(blacktop(stage),h)==Result::observed);
 auto installed=blacktop(2);installed.transition_ready=false;
 assert(s.tick(installed,h)==Result::observed && s.owned());
 assert(h.writes==1);
 assert(s.tick(blacktop(3),h)==Result::restored);
 assert(!s.owned() && h.endpoint==Endpoint::stock && h.writes==2);
 assert(s.tick(blacktop(3),h)==Result::waiting);
 assert(s.tick(home(),h)==Result::armed && h.writes==3);
 auto in=home();in.current=in.alternate=0xF54D5D12;
 assert(s.tick(in,h)==Result::observed && h.endpoint==Endpoint::general_loader);
 assert(s.tick(blacktop(3),h)==Result::restored);
 assert(s.tick(home(),h)==Result::armed);
 in=home();in.permission=false;h.publish_allowed=false;
 assert(s.tick(in,h)==Result::held && !s.owned() && h.endpoint==Endpoint::stock);
 const auto writes=h.writes;assert(s.tick(home(),h)==Result::held && h.writes==writes);
}
static void exit_shapes_and_liveness() {
 for(unsigned n=0;n<6;++n) {
  Host h;Session s(game);assert(s.tick(home(),h)==Result::armed);
  auto in=blacktop(2);
  switch(n) {
   case 0:in.stable=false;break;
   case 1:in.alternate^=1;break;
   case 2:in.active=1;break;
   case 3:in.kind=1;break;
   case 4:in.stage=3;break;
   default:in.current=in.alternate=0xB4D37B22;in.stage=1;break;
  }
  if(n<=1 || n==5) {
   assert(s.tick(in,h)==Result::observed && s.owned() && h.writes==1);
   assert(s.tick(blacktop(0),h)==Result::observed && h.writes==1);
   assert(s.tick(blacktop(2),h)==Result::observed && h.writes==1);
   assert(s.tick(blacktop(3),h)==Result::restored && !s.owned() && h.endpoint==Endpoint::stock);
  } else assert(s.tick(in,h)==Result::restored && !s.owned() && h.endpoint==Endpoint::stock);
 }
 Host h;Session s(game);assert(s.tick(home(),h)==Result::armed);
 auto in=home();in.process=Process::unknown;
 assert(s.tick(in,h)==Result::held && s.owned() && h.writes==1);
 in.process=Process::exited;
 assert(s.tick(in,h)==Result::ended && !s.owned() && h.writes==1);
 auto mismatch=home();mismatch.game.birth++;
 Host other;Session bound(game);assert(bound.tick(mismatch,other)==Result::held && other.writes==0);
 Host lost;Session owner(game);assert(owner.tick(home(),lost)==Result::armed);
 auto gap=home();gap.stable=false;gap.permission=false;
 assert(owner.tick(gap,lost)==Result::held && !owner.owned() && lost.endpoint==Endpoint::stock && lost.writes==2);
}
static void bounded_owned_transition_retention() {
 Host h;Session s(game);assert(s.tick(home(),h)==Result::armed);
 auto menu=home();menu.current=menu.alternate=0x4FDE5DCD;menu.stage=0;
 h.clock+=1000;assert(s.tick(menu,h)==Result::observed && h.writes==1);
 menu.current=menu.alternate=0x11111111;menu.stage=2;
 h.clock+=1000;assert(s.tick(menu,h)==Result::observed && h.writes==1);
 menu.stable=false;menu.alternate^=1;
 h.clock=60999;assert(s.tick(menu,h)==Result::observed && s.owned());
 h.clock=61000;assert(s.tick(menu,h)==Result::restored && !s.owned() && h.writes==2);
 assert(s.tick(menu,h)==Result::waiting && h.writes==2);
 assert(s.tick(home(),h)==Result::armed && h.writes==3);
 // Stable callbacks may rearm normally only after complete exact restoration.
 assert(s.tick(blacktop(3),h)==Result::restored);
 // Even an unstable sample may explicitly reveal active/online state.
 for(unsigned kind=0;kind<2;++kind) {
  Host online;Session owned(game);assert(owned.tick(home(),online)==Result::armed);
  auto input=menu;if(kind)input.kind=1;else input.active=1;
  assert(owned.tick(input,online)==Result::restored && !owned.owned() && online.writes==2);
 }
 // Never refresh the deadline on observation; durable publication time counts.
 Host slow;slow.intent_elapsed=10000;Session delayed(game);
 assert(delayed.tick(home(),slow)==Result::armed);slow.intent_elapsed=0;
 slow.clock=61000;assert(delayed.tick(menu,slow)==Result::restored);
 Host backward;Session reversed(game);assert(reversed.tick(home(),backward)==Result::armed);
 backward.clock=2000;assert(reversed.tick(menu,backward)==Result::observed);
 backward.clock=1999;assert(reversed.tick(menu,backward)==Result::held && !reversed.owned() && backward.writes==2);
 assert(reversed.tick(home(),backward)==Result::held && backward.writes==2);
 Host lost;Session uncertain(game);assert(uncertain.tick(home(),lost)==Result::armed);
 lost.clock=999;lost.restore_allowed=false;
 assert(uncertain.tick(menu,lost)==Result::held && uncertain.owned() && lost.writes==1);
 Host no_clock;no_clock.clock_failure=true;Session unopened(game);
 assert(unopened.tick(home(),no_clock)==Result::held && no_clock.writes==0);
 Host mid_clock;Session opened(game);assert(opened.tick(home(),mid_clock)==Result::armed);
 mid_clock.clock_failure=true;assert(opened.tick(menu,mid_clock)==Result::held && opened.owned() && mid_clock.writes==1);
}
static void failure_never_retries_or_adopts() {
 {
  Host conflict;conflict.cas_conflict=true;Session owner(game);
  assert(owner.tick(home(),conflict)==Result::held && owner.uncertain() && !owner.owned());
  assert(conflict.endpoint==Endpoint::general_loader && conflict.applied==0);
  assert(owner.tick(blacktop(3),conflict)==Result::held && conflict.writes==1);
 }
 for(auto endpoint: {Endpoint::general_loader,Endpoint::foreign,Endpoint::unknown}) {
  Host h;h.endpoint=endpoint;Session s(game);
  assert(s.tick(home(),h)==Result::held && !s.owned() && h.writes==0);
 }
 for(unsigned n=0;n<8;++n) {
  Host h;Session s(game);
  switch(n) {
   case 0:h.publish_allowed=false;break;
   case 1:h.intent_ok=false;break;
   case 2:h.fail_checkpoint_at=2;break; // Boundary lost after durable intent.
   case 3:h.foreign_checkpoint_at=2;break; // Foreign CAS input after durable intent.
   case 4:h.write_ok=false;break; // CAS took effect but result uncertain/failed.
   case 5:h.applied_ok=false;break; // Applied receipt failed after exact mutation.
   case 6:h.fail_readback=true;break;
   default:h.throw_write=true;break;
  }
  assert(s.tick(home(),h)==Result::held);
  if(n<4)assert(h.writes==0);else assert(h.writes==1);
  if(n==4 || n==5)assert(s.owned());
  if(n==6 || n==7)assert(s.uncertain());
  const auto writes=h.writes,checks=h.checks;
  assert(s.tick(home(),h)==Result::held && h.writes==writes && h.checks==checks);
 }
}
static void restoration_conflicts_and_observations() {
 for(auto endpoint: {Endpoint::foreign,Endpoint::unknown}) {
  Host h;Session s(game);assert(s.tick(home(),h)==Result::armed);
  h.endpoint=endpoint;assert(s.tick(blacktop(3),h)==Result::held && h.writes==1 && s.owned());
 }
 for(unsigned n=0;n<3;++n) {
  Host h;Session s(game);assert(s.tick(home(),h)==Result::armed);
  h.endpoint=Endpoint::stock;
  if(n==1)h.intent_ok=false;
  if(n==2)h.applied_ok=false;
  const auto result=s.tick(blacktop(3),h);
  assert(result==(n==0?Result::restored:Result::held));
  assert(h.writes==1); // Observing stock never writes it again.
  assert(s.owned()==(n!=0));
 }
 Host h;Session s(game);assert(s.tick(home(),h)==Result::armed);
 h.restore_allowed=false;
 assert(s.tick(blacktop(3),h)==Result::held && h.writes==1 && s.owned());
 for(unsigned n=0;n<3;++n) {
  Host restore;Session owner(game);assert(owner.tick(home(),restore)==Result::armed);
  if(n==0)restore.intent_ok=false;
  if(n==1)restore.applied_ok=false;
  if(n==2)restore.write_ok=false;
  assert(owner.tick(blacktop(3),restore)==Result::held);
  assert(restore.writes==(n==0?1u:2u));
  assert(restore.endpoint==(n==0?Endpoint::general_loader:Endpoint::stock));
  const auto writes=restore.writes;
  assert(owner.tick(home(),restore)==Result::held && restore.writes==writes);
 }
}
static void budget_does_not_block_owned_observation_or_restore() {
 Host h;h.budget=false;Session s(game);
 assert(s.tick(home(),h)==Result::limit_reached && h.writes==0 && h.intents==0);
 h.budget=true;assert(s.tick(home(),h)==Result::armed);
 h.budget=false;assert(s.tick(blacktop(1),h)==Result::observed);
 assert(s.tick(blacktop(3),h)==Result::restored && h.endpoint==Endpoint::stock);
 assert(s.tick(home(),h)==Result::limit_reached && h.writes==2 && h.intents==2);
}
int main() {
 budget_does_not_block_owned_observation_or_restore();admission_and_boundaries();lifecycle_and_reentry();exit_shapes_and_liveness();bounded_owned_transition_retention();
 failure_never_retries_or_adopts();restoration_conflicts_and_observations();
 std::puts("Blacktop first-entry roster callback pure policy tests passed");
}
