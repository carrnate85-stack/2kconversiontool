#pragma once
// Pure first-entry dispatch ownership policy. No Windows API, addresses,
// allocations, code publication, direct game calls, or automatic worker.
#include "wnba_blacktop_category_core.hpp"

namespace wnba_blacktop_roster {
using Binding=wnba_blacktop_category::Binding;
using Process=wnba_blacktop_category::Process;
enum class Endpoint { stock, general_loader, foreign, unknown };
struct Input {
 Binding game;
 Process process{Process::unknown};
 bool permission{},stable{},transition_ready{};
 std::uint32_t current{},alternate{},stage{},active{},kind{};
};
inline bool offline(const Input& in) {
 return in.stable && in.current==in.alternate && !in.active && !in.kind;
}
inline bool launch_menu(const Input& in) {
 return offline(in) && ((in.current==0x6509837B && in.stage<=1) ||
  (in.current==0x079964C0 && in.stage==1));
}
inline bool entry_complete(const Input& in) {
 return in.stable && in.current==0x92DF06DE && in.alternate==in.current && in.stage==3;
}
inline constexpr std::uint64_t kEntryDeadlineMs=60000;
enum class Phase { waiting, armed, held, ended };
enum class Result { waiting, armed, observed, restored, held, ended, limit_reached };

// Host contract:
// monotonic_ms() is the native monotonic clock, not browser input. The deadline
// starts BEFORE the publication attempt; observed ticks never renew it.
// checkpoint(binding,true) repeats exact live owned-offline/source/permission
// and stable Home/Quick Play admission, transition_ready (installed delegate
// differs from the target Blacktop delegate), and native callback profile.
// checkpoint(binding,false) proves retained exact live game/profile/slot only;
// it MUST remain usable for conditional restoration after parent/permit loss.
// read_endpoint(out) classifies only this exact slot; foreign/unknown values
// never become owned. A false return means the read was not complete.
// journal_intent/applied are durable, read back and session scoped. stock->stock
// records an external stock restoration observation, with NO write_endpoint.
// write_endpoint uses exact aligned CAS and readback, never an unconditional
// store or call. This route targets the existing general Learn2K loader via the
// Blacktop delegate callback; it is not the withdrawn Freestyle sibling swap.
class Session {
 Binding binding_;
 Phase phase_{Phase::waiting};
 bool owned_{},uncertain_{};
 std::uint64_t armed_at_ms_{},last_clock_ms_{};
 const char* reason_="Waiting for a stable owned Home or Quick Play boundary";
 Result hold(const char* reason) { phase_=Phase::held;reason_=reason;return Result::held; }
 template<class Host> bool exact(Host& h,Endpoint want) {
  Endpoint a{Endpoint::unknown},b{Endpoint::unknown};
  return h.read_endpoint(a) && h.read_endpoint(b) && a==b && a==want;
 }
 struct Step { bool after{},ok{},uncertain{}; };
 template<class Host> Step change(Host& h,Endpoint from,Endpoint to,bool publishing) {
  if(!h.checkpoint(binding_,publishing))return {};
  if(!exact(h,from))return {false,false,true};
  if(!h.journal_intent(binding_,from,to))return {};
  if(!h.checkpoint(binding_,publishing))return {};
  if(!exact(h,from))return {false,false,true};
  const bool wrote=h.write_endpoint(from,to);
  Endpoint a{Endpoint::unknown},b{Endpoint::unknown};
  if(!h.read_endpoint(a) || !h.read_endpoint(b) || a!=b)return {false,false,true};
  if(a==to)return {true,h.journal_applied(binding_,from,to)&&wrote,false};
  if(a==from)return {};
  return {false,false,true};
 }
 template<class Host> bool restore(Host& h) {
  if(!owned_ || !h.checkpoint(binding_,false))return false;
  Endpoint a{Endpoint::unknown},b{Endpoint::unknown};
  if(!h.read_endpoint(a) || !h.read_endpoint(b) || a!=b) { uncertain_=true;return false; }
  if(a==Endpoint::stock) {
   // Another owner may have restored stock. Record that exact observation
   // durably; never CAS or adopt a foreign replacement to make it look clean.
   if(!h.journal_intent(binding_,Endpoint::stock,Endpoint::stock) ||
      !h.checkpoint(binding_,false) || !exact(h,Endpoint::stock) ||
      !h.journal_applied(binding_,Endpoint::stock,Endpoint::stock))return false;
   owned_=false;return true;
  }
  if(a!=Endpoint::general_loader) { uncertain_=true;return false; }
  const auto result=change(h,Endpoint::general_loader,Endpoint::stock,false);
  uncertain_|=result.uncertain;
  if(result.after)owned_=false;
  return result.after && result.ok;
 }
 template<class Host> Result tick_impl(const Input& in,Host& h) {
  if(!binding_.valid() || in.game!=binding_)return hold("Blacktop roster game identity changed; no retry");
  if(in.process==Process::exited) {
   phase_=Phase::ended;owned_=uncertain_=false;return Result::ended;
  }
  if(phase_==Phase::held)return Result::held;
  if(phase_==Phase::ended)return Result::ended;
  if(in.process!=Process::live)return hold("Blacktop roster game liveness is uncertain; resources retained");
  if(phase_==Phase::armed) {
   const auto now=h.monotonic_ms();
   const bool rollback=now<last_clock_ms_;
   const bool expired=!rollback && now-armed_at_ms_>=kEntryDeadlineMs;
   if(!rollback)last_clock_ms_=now;
   // Live QA069 proved the native route crosses stable intermediate menu
   // 4FDE5DCD before Blacktop installs its delegate. WPF releases only after
   // BOTH Blacktop stage3 or its60s deadline. Initial publication stays strict;
   // retaining this exact owned slot across menus grants no new write access.
   // Explicit active/online owner observations still stop retention immediately.
   if(!in.permission || in.active!=0 || in.kind!=0 || entry_complete(in) || expired || rollback) {
    if(!restore(h))return hold("Owned Blacktop roster callback restoration failed or conflicts; no retry");
    if(!in.permission)return hold("Blacktop roster permission ended; exact owned callback restored");
    if(rollback)return hold("Blacktop roster monotonic clock reversed; exact owned callback restored, no retry");
    phase_=Phase::waiting;
    reason_=expired?"Stock Blacktop callback restored at60s entry deadline; awaiting a fresh Home boundary":
     "Stock Blacktop callback restored; awaiting the next stable Home boundary";
    return Result::restored;
   }
   // Retained identity/profile/exact already-owned callback are independently
   // re-proved on every tick, including unstable intermediate observations.
   if(!h.checkpoint(binding_,false) || !exact(h,Endpoint::general_loader)) {
    uncertain_=true;return hold("Owned Blacktop roster callback changed or profile became uncertain; no overwrite");
   }
   return Result::observed;
  }
  if(!in.permission || !in.transition_ready || !launch_menu(in))return Result::waiting;
  if(!h.publication_budget()) {
   reason_="Waiting: bounded roster journal cannot reserve forward and exact restore receipts";
   return Result::limit_reached;
  }
  const auto started=h.monotonic_ms(); // Includes intent flush and final CAS checks.
  const auto result=change(h,Endpoint::stock,Endpoint::general_loader,true);
  if(result.after)owned_=true;
  uncertain_|=result.uncertain;
  if(!result.ok)return hold("Blacktop roster callback publication failed; no retry, resources retained");
  armed_at_ms_=last_clock_ms_=started;
  phase_=Phase::armed;reason_="Owned Blacktop callback armed for the native general roster loader";
  return Result::armed;
 }
public:
 explicit Session(Binding binding):binding_(std::move(binding)){}
 Phase phase()const{return phase_;}
 const char* reason()const{return reason_;}
 bool owned()const{return owned_;}
 bool uncertain()const{return uncertain_;}
 template<class Host> Result tick(const Input& in,Host& h) {
  try { return tick_impl(in,h); }
  catch(...) { uncertain_=true;return hold("Blacktop roster dispatch exception; no retry, resources retained"); }
 }
};
} // namespace wnba_blacktop_roster
