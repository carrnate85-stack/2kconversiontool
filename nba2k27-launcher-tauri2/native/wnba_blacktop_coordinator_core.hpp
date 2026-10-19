#pragma once
// Shared production ordering only. No Windows APIs, process access, function
// calls into the game, allocation/free capability, workers, or token reader.
#include "wnba_blacktop_category_core.hpp"
#include "wnba_blacktop_roster_core.hpp"

namespace wnba_blacktop_coordinator {
namespace category=wnba_blacktop_category;
namespace roster=wnba_blacktop_roster;
struct Tick {
 roster::Result roster_result{roster::Result::waiting};
 category::Result category_result{category::Result::waiting};
 bool category_ticked{},held{};
 const char* reason{};
 const char* roster_reason{};
 const char* category_reason{};
};
// Informational only: no session policy, journal use, cleanup gate or stop.
enum class LimitSource { none, roster, category, both };
inline LimitSource limit_source(const Tick& tick) {
 const bool roster=tick.roster_result==roster::Result::limit_reached;
 const bool category=tick.category_result==category::Result::limit_reached;
 return roster?(category?LimitSource::both:LimitSource::roster):
  (category?LimitSource::category:LimitSource::none);
}
class LimitNotice {
 bool roster_{},category_{},reported_{};
public:
 bool observe(const Tick& tick,bool roster_owned,bool category_owned) {
  // HELD has priority; leave cleanup/retention to the existing coordinator.
  if(tick.held)return false;
  const auto source=limit_source(tick);
  roster_|=source==LimitSource::roster || source==LimitSource::both;
  category_|=source==LimitSource::category || source==LimitSource::both;
  if(reported_ || (!roster_ && !category_) || roster_owned || category_owned)return false;
  reported_=true;return true;
 }
 const char* detail()const {
  return roster_?(category_?"Session change limit reached (roster and category); both routes are unowned; restart game for further changes":
   "Session change limit reached (roster); both routes are unowned; restart game for further changes"):
   "Session change limit reached (category); both routes are unowned; restart game for further changes";
 }
};
class Coordinator {
 roster::Session roster_;
 category::Session category_;
 bool held_{};
 const char* reason_="Waiting for owned Blacktop initialization";
 static bool stopped(roster::Result r) {return r==roster::Result::held || r==roster::Result::ended;}
 static bool stopped(category::Result r) {return r==category::Result::held || r==category::Result::ended;}
 static roster::Input roster_input(const category::Input& in,bool transition_ready) {
  roster::Input result{};result.game=in.game;result.process=in.process;
  result.permission=in.owned_offline && in.source_trial_verified && in.explicit_category_consent;
  result.stable=in.stable;result.transition_ready=transition_ready;
  result.current=in.current;result.alternate=in.alternate;result.stage=in.stage;
  result.active=in.active;result.kind=in.kind;return result;
 }
 template<class RosterHost,class CategoryHost>
 Tick tick_impl(const category::Input& in,RosterHost& rh,CategoryHost& ch) {
  if(held_)return {roster::Result::held,category::Result::held,false,true,reason_};
  auto loader=roster_input(in,false);
  Tick result{};
  // WPF's Home/Quick Play path explicitly cleans the old category route
  // BEFORE arming a new native roster callback. A failed cleanup must never
  // start a new loader publication. This is strict positive menu admission,
  // not an inference from an unstable or mismatched context.
  if(category_.owned_route() && roster::launch_menu(loader)) {
   result.category_result=category_.tick(in,ch);result.category_ticked=true;
   result.category_reason=category_.reason();
   if(stopped(result.category_result)) {
    loader.permission=false;
    result.roster_result=roster_.tick(loader,rh);
    result.roster_reason=roster_.reason();
    held_=true;reason_=category_.reason();result.held=true;result.reason=reason_;return result;
   }
  }
  loader.transition_ready=rh.transition_ready();
  result.roster_result=roster_.tick(loader,rh);
  result.roster_reason=roster_.reason();
  if(stopped(result.roster_result)) {
   // Source/callback failure cannot leave a separately owned category route
   // running. Its controller conditionally restores only its exact own bytes.
   auto denied=in;denied.owned_offline=false;
   result.category_result=category_.tick(denied,ch);result.category_ticked=true;
   result.category_reason=category_.reason();
   held_=true;reason_=roster_.reason();result.held=true;result.reason=reason_;return result;
  }
  // Stage3 restores the roster callback above BEFORE any category publication.
  if(!result.category_ticked) {
   result.category_result=category_.tick(in,ch);result.category_ticked=true;
   result.category_reason=category_.reason();
  }
  if(stopped(result.category_result)) {
   loader.permission=false;
   roster_.tick(loader,rh);
   held_=true;reason_=category_.reason();result.held=true;
  }
  result.reason=reason_;return result;
 }
public:
 Coordinator(category::Binding binding,std::uintptr_t base,std::uintptr_t label_rva):
  roster_(binding),category_(std::move(binding),base,label_rva){}
 bool held()const{return held_;}
 const char* reason()const{return reason_;}
 bool roster_owned()const{return roster_.owned();}
 bool category_owned()const{return category_.owned_route();}
 template<class RosterHost,class CategoryHost>
 Tick tick(const category::Input& in,RosterHost& rh,CategoryHost& ch) {
  try {return tick_impl(in,rh,ch);}
  catch(...) {
   // The incoming game scope was already sampled by the caller. A failed
   // additional observation can only stop permissions, never authorize a new
   // publication. Each controller independently repeats retained identity and
   // exact ownership before any conditional cleanup; no exception may escape
   // this fallback or cause an application retry.
   try {auto denied=in;denied.owned_offline=false;category_.tick(denied,ch);}catch(...){}
   try {auto denied=roster_input(in,false);denied.permission=false;roster_.tick(denied,rh);}catch(...){}
   held_=true;reason_="Blacktop coordination observation failed; resources retained, no retry";
   return {roster::Result::held,category::Result::held,false,true,reason_};
  }
 }
};
} // namespace wnba_blacktop_coordinator
