#pragma once
// OFFLINE POLICY ONLY. No Windows APIs, bindings, worker, executable memory
// allocation, or game-memory capability is supplied by this header.
// Mapping: qa062-blacktop-priority-alias-disk-audit-20261018.json and paired
// switch disassembly. 0x230BE98 is instruction data, NOT the jump table.
#include "wnba_blacktop_journal_budget.hpp"
#include <array>
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <limits>
#include <span>
#include <string>
#include <utility>
#include <vector>

namespace wnba_blacktop_category {
inline constexpr std::uintptr_t kTable=0x7425F4C, kEntry=kTable+12;
inline constexpr std::uintptr_t kStockCase=0x230BF54, kContinue=0x230BEDB;
// Initializer location only; runtime sorting moves this record. Session takes
// the separately resolved, validated record RVA instead of writing this slot.
inline constexpr std::uintptr_t kLabel=0xACFC4D0;
inline constexpr std::uint32_t kLabelId=0xBA3C982D;
inline constexpr std::uint64_t kEuro=0x6FC058B0442652CD, kWomen=0x0F39A44C3426C962;
inline constexpr std::int32_t kStockDispatch=std::int32_t(std::int64_t(kStockCase)-std::int64_t(kTable));
struct Binding {
    std::uint32_t pid{};std::uint64_t birth{};std::string nonce;
    bool operator==(const Binding&) const = default;
    bool valid() const {
        if(!pid || !birth || nonce.size()!=48)return false;
        for(char c:nonce)if(!((c>='0'&&c<='9')||(c>='a'&&c<='f')||(c>='A'&&c<='F')))return false;
        return true;
    }
};
// V2 permission covers the paired native roster callback + category route.
// The old BC1 category-only diagnostic is deliberately not this capability.
inline bool category_approval_matches(std::span<const std::uint8_t> bytes,const Binding& binding) {
    constexpr std::array<std::uint8_t,8> magic={'C','2','7','W','N','B','C','2'};
    if(bytes.size()!=72 || !binding.valid())return false;
    for(unsigned n=0;n<8;++n)if(bytes[n]!=magic[n])return false;
    std::uint32_t pid{},reserved{};std::uint64_t birth{};
    for(unsigned n=0;n<4;++n) {pid|=std::uint32_t(bytes[8+n])<<(8*n);reserved|=std::uint32_t(bytes[12+n])<<(8*n);}
    for(unsigned n=0;n<8;++n)birth|=std::uint64_t(bytes[16+n])<<(8*n);
    if(pid!=binding.pid || reserved || birth!=binding.birth)return false;
    for(unsigned n=0;n<48;++n)if(bytes[24+n]!=std::uint8_t(binding.nonce[n]))return false;
    return true;
}
enum class Process { live, exited, unknown };
struct Input {
    Binding game;
    Process process{Process::unknown};
    bool owned_offline{}, source_trial_verified{}, explicit_category_consent{};
    bool stable{}, deep_mixed{};
    std::uint32_t current{},alternate{},stage{},active{},kind{},observed_team_count{};
    std::uint64_t roster_generation{};
    std::vector<std::uint16_t> clubs;
};
inline bool ready_selection(const Input& in) {
    if(!in.stable || !in.deep_mixed || !in.roster_generation ||
        in.current!=0x92DF06DE || in.alternate!=in.current || in.stage!=3 ||
        in.active || in.kind || in.clubs.size()!=15)return false;
    std::array<bool,18> seen{};
    for(auto id:in.clubs) {
        if(id<300 || id>317 || (id>=312 && id<=314) || seen[id-300])return false;
        seen[id-300]=true;
    }
    return true;
}
struct Route {
    std::int32_t dispatch{kStockDispatch};
    std::uint32_t label_id{kLabelId},label_reserved{};
    std::uint64_t label{kEuro};
    bool operator==(const Route&) const = default;
};
enum class Word { label, dispatch };
struct Change { Word word;std::uintptr_t at;std::uint64_t before,after; };
using Code=std::array<std::uint8_t,10>;
inline bool displacement(std::uintptr_t target,std::uintptr_t origin,std::int32_t& out) {
    if(target>0x7FFF00000000ull || origin>0x7FFF00000000ull)return false;
    const auto n=std::int64_t(target)-std::int64_t(origin);
    if(n<std::numeric_limits<std::int32_t>::min() || n>std::numeric_limits<std::int32_t>::max())return false;
    out=std::int32_t(n);return true;
}
inline bool bridge_plan(std::uintptr_t base,std::uintptr_t bridge,Code& code,std::int32_t& dispatch) {
    if(base<0x10000 || base>0x7FFF00000000ull-0x36532000 ||
        bridge<0x10000 || bridge>0x7FFF00000000ull-code.size())return false;
    std::int32_t jump{};
    if(!displacement(bridge,base+kTable,dispatch) ||
        !displacement(base+kContinue,bridge+code.size(),jump))return false;
    code={0xBE,0,0,0,0x20,0xE9,0,0,0,0};
    const auto raw=std::uint32_t(jump);
    for(unsigned i=0;i<4;++i)code[6+i]=std::uint8_t(raw>>(i*8));
    return true;
}
enum class Phase { waiting, preparing, prepared, active, held, ended };
enum class PreinitializeProof { exact, defer, failed };
enum class Result { waiting, published, observed, restored, held, ended, limit_reached };

// Host contract for a FUTURE independently reviewed executor:
// - checkpoint(binding,true) repeats exact source-trial/consent/owned-offline
//   identity, deep stable selection, complete native code/table/label profile.
// - a failed publishing checkpoint may classify selection_departed() ONLY
//   after exact profile/route/permission/source proof and a fresh stable offline
//   context outside selection. All other failures remain permanent holds.
//   reason() supplies a stable diagnostic for that failed checkpoint.
// - checkpoint(binding,false) proves retained live game identity and original
//   native profile for conditional restoration, even after selection/owner exit.
// - monotonic_ms/bind_preparation_start share one nonrenewable 10s origin.
//   The host repeats the bound inside preparation proofs and immediately
//   before every preparation intent/effect, not just after returning to core.
// - Typed pre-effect deferral never normalizes Context. Owned RW proof includes
//   zero8192 twice and original intent identity/bytes/ordinal. Any initialize
//   intent attempt is irreversible: no further defer, disk adoption or replay.
// - journal_* is durable before each mutation; write_change is restricted to
//   this exact image-backed label key(8 bytes)/dispatch entry(4 bytes), checks
//   before bytes, verifies readback, and restores page protection on ALL paths.
// - bridge allocation starts private RW, is initialized once, sealed private RX
//   and instruction-cache flushed/read back before journal_bridge_ready.
// - every allocation, including uncertain failures, remains retained until
//   exact game exit. There is deliberately NO free/unregister API here.
class Session {
    Binding binding_;std::uintptr_t base_{},label_rva_{},bridge_{};Code code_{};
    std::int32_t bridge_dispatch_{};std::uint64_t generation_{};
    Phase phase_{Phase::waiting};bool label_owned_{},dispatch_owned_{},uncertain_{};
    const char* reason_="Awaiting explicit owned trial category admission";
    std::array<char,512> bridge_reason_{};
    bool preparation_started_{},allocation_attempted_{};
    std::uint64_t preparation_start_{},preparation_last_{},preparation_generation_{};
    unsigned preparation_attempts_{};
    template<class Host> bool preparation_clock(Host& h) {
        const auto now=h.monotonic_ms();
        if(!preparation_started_) {preparation_start_=preparation_last_=now;preparation_started_=true;return true;}
        if(now<preparation_last_)return false;
        preparation_last_=now;return now-preparation_start_<10000;
    }
    template<class Host> bool start_preparation(const Input& in,Host& h) {
        if(!in.roster_generation)return false;
        preparation_generation_=in.roster_generation;phase_=Phase::preparing;preparation_attempts_=1;
        return preparation_clock(h) && h.bind_preparation_start(preparation_start_);
    }
    template<class Host> Result preparation_wait(Host& h) {
        if(!preparation_clock(h))return hold("Bridge preparation deadline or monotonic clock changed; no retry");
        reason_="Waiting for strict Blacktop selection; uninitialized preparation retained without replay";
        return Result::waiting;
    }
    Result hold(const char* reason){phase_=Phase::held;reason_=reason;return Result::held;}
    Result bridge_hold(const char* stage,const char* detail) {
        std::snprintf(bridge_reason_.data(),bridge_reason_.size(),"Bridge preparation failed [%s]: %s",stage,detail?detail:"Host detail unavailable");
        return hold(nullptr); // reason() resolves this object's fixed buffer, never a borrowed host pointer.
    }
    Route expected() const {
        Route r;if(label_owned_)r.label=kWomen;if(dispatch_owned_)r.dispatch=bridge_dispatch_;return r;
    }
    template<class Host> bool exact(Host& h,const Route& want) {
        Route a{},b{};return h.read_route(a)&&h.read_route(b)&&a==b&&a==want;
    }
    struct Step {bool after{},ok{},uncertain{};};
    template<class Host> Step step(Host& h,const Change& change,const Route& before,const Route& after,bool publishing) {
        if(!h.checkpoint(binding_,publishing))return {};
        if(!exact(h,before))return {false,false,true};
        if(!h.journal_change(binding_,change))return {};
        if(!h.checkpoint(binding_,publishing))return {};
        if(!exact(h,before))return {false,false,true};
        const bool written=h.write_change(change);
        Route a{},b{};
        if(!h.read_route(a)||!h.read_route(b)||a!=b)return {false,false,true};
        if(a==after)return {true,h.journal_applied(binding_,change)&&written,false};
        // A durable intent whose guarded CAS made no change must receive its
        // own durable closure before an already-owned prefix can be restored.
        // Otherwise the host's pending intent blocks the rollback journal and
        // strands the WNBA label after a known no-effect dispatch failure.
        if(a==before)return {false,false,!h.journal_not_applied(binding_,change)};
        return {false,false,true};
    }
    template<class Host> bool restore(Host& h) {
        bool clean=true;
        if(!exact(h,expected())){uncertain_=true;return false;}
        if(dispatch_owned_) {
            const auto before=expected();auto after=before;after.dispatch=kStockDispatch;
            const auto s=step(h,{Word::dispatch,base_+kEntry,std::uint32_t(bridge_dispatch_),std::uint32_t(kStockDispatch)},before,after,false);
            if(!s.after){uncertain_|=s.uncertain;return false;}
            dispatch_owned_=false;clean&=s.ok;
        }
        if(label_owned_) {
            const auto before=expected();auto after=before;after.label=kEuro;
            const auto s=step(h,{Word::label,base_+label_rva_+8,kWomen,kEuro},before,after,false);
            if(!s.after){uncertain_|=s.uncertain;return false;}
            label_owned_=false;clean&=s.ok;
        }
        generation_=0;return clean && exact(h,Route{});
    }
    template<class Host> Result failed_publication(Host& h,const Step& s) {
        uncertain_|=s.uncertain;
        if(!uncertain_ && (label_owned_||dispatch_owned_))restore(h);
        return hold("Category publication failed; no retry or foreign restoration permitted");
    }
    template<class Host> Result tick_impl(const Input& in,Host& h) {
        if(!binding_.valid() || in.game!=binding_)return hold("Game identity or category consent scope changed");
        if(label_rva_<0xACFA610 || label_rva_>=0xACFCE80 || (label_rva_-0xACFA610)%16)
            return hold("Category label was not resolved inside the pinned native label table");
        if(in.process==Process::exited){phase_=Phase::ended;label_owned_=dispatch_owned_=uncertain_=false;return Result::ended;}
        if(phase_==Phase::held)return Result::held;
        if(phase_==Phase::ended)return Result::ended;
        if(in.process!=Process::live)return hold("Game liveness is uncertain; bridge and owned state retained");
        const bool permission=in.owned_offline && in.source_trial_verified && in.explicit_category_consent;
        const bool selection=ready_selection(in);
        if(phase_==Phase::active) {
            if(!permission || !selection || in.roster_generation!=generation_) {
                const bool stopped=!permission || (selection && in.roster_generation!=generation_);
                if(!restore(h))return hold("Owned category restore is incomplete or conflicts; no retry");
                if(stopped)return hold("Consent/source/roster ownership changed; exact owned route restored");
                phase_=Phase::prepared;reason_="Selection exited; stock dispatch and label restored, RX bridge retained";
                return Result::restored;
            }
            if(!h.checkpoint(binding_,true)) {
                if(h.selection_departed()) {
                    if(!h.bridge_is_private_rx(bridge_,code_) || !exact(h,expected()))
                        return hold("Active category ownership changed during selection exit; no overwrite");
                    if(!restore(h))return hold("Owned category selection-exit restore is incomplete or conflicts; no retry");
                    phase_=Phase::prepared;reason_="Fresh stable offline selection exit; exact owned route restored, RX bridge retained";
                    return Result::restored;
                }
                return hold(h.reason());
            }
            if(!h.bridge_is_private_rx(bridge_,code_))return hold("Active category RX bridge changed; no overwrite");
            if(!exact(h,expected()))return hold("Active category exact owned route changed; no overwrite");
            return Result::observed;
        }
        if(!in.owned_offline)return hold("Owned offline game admission lost");
        if(phase_==Phase::preparing) {
            if(!preparation_clock(h))return hold("Bridge preparation deadline or monotonic clock changed; no retry");
            if(!permission)return hold("Uninitialized bridge permission/source/consent lost; no retry");
            if(in.deep_mixed && in.roster_generation && in.roster_generation!=preparation_generation_)
                return hold("Uninitialized bridge original roster generation changed; no adoption");
            if(!selection) {
                if(in.stable && in.current==0x92DF06DE && in.alternate==in.current &&
                   in.stage==3 && in.active==0 && in.kind==0)
                    return hold("Uninitialized bridge strict picker roster proof is unavailable; no retry");
                if(!h.passive_preparation_scope())return bridge_hold("passive proof",h.reason());
                return preparation_wait(h);
            }
            if(in.roster_generation!=preparation_generation_)return hold("Uninitialized bridge roster generation is unproven; no initialization");
            if(preparation_attempts_>=12)return hold("Bridge preparation attempt limit reached; no retry");
            ++preparation_attempts_;
        }
        if(!permission || !selection)return Result::waiting;
        if(!h.checkpoint(binding_,true)) {
            if(phase_!=Phase::preparing && !bridge_) {
                if(!start_preparation(in,h))return hold("Bridge preparation generation or clock is unproven");
            }
            if(phase_==Phase::preparing) {
                const auto proof=bridge_?h.preinitialize_proof(false):h.preallocation_proof();
                if(proof==PreinitializeProof::defer)return preparation_wait(h);
            }
            return bridge_hold("baseline checkpoint",h.reason());
        }
        if(phase_==Phase::preparing && !preparation_clock(h))return hold("Bridge preparation checkpoint exceeded deadline; no retry");
        if(!exact(h,Route{}))return hold("Category baseline is not exact stock; nothing adopted");
        if(phase_==Phase::preparing && !preparation_clock(h))return hold("Bridge preparation stock proof exceeded deadline; no retry");
        if(!h.publication_budget(phase_!=Phase::preparing && bridge_!=0)) {
            reason_="Waiting: bounded category journal cannot reserve forward and exact restore receipts";
            if(phase_==Phase::preparing && !preparation_clock(h))return hold("Bridge preparation journal budget wait exceeded deadline; no retry");
            return Result::limit_reached;
        }
        if(!bridge_) {
            if(phase_!=Phase::preparing && !start_preparation(in,h))return hold("Bridge preparation generation or clock is unproven");
            if(allocation_attempted_)return hold("Bridge allocation intent was already attempted; no replay");
            allocation_attempted_=true;
            if(!h.journal_bridge_intent(binding_))return hold("Bridge allocation intent was not durable");
            if(!preparation_clock(h))return hold("Bridge allocation intent exceeded preparation deadline; no allocation");
            bridge_=h.allocate_bridge_near(base_,kTable,code_.size());
            if(bridge_)h.retain_bridge(bridge_);
            if(!bridge_)return bridge_hold("allocate",h.reason());
            if(!preparation_clock(h))return hold("Bridge allocation exceeded preparation deadline; allocation retained");
            if(!bridge_plan(base_,bridge_,code_,bridge_dispatch_))return bridge_hold("plan","Core bridge displacement rejected");
        }
        if(phase_==Phase::preparing) {
            const auto proof=h.preinitialize_proof(true);
            if(!preparation_clock(h))return hold("Uninitialized bridge proof exceeded preparation deadline; no retry");
            if(proof==PreinitializeProof::defer)return preparation_wait(h);
            if(proof!=PreinitializeProof::exact)return bridge_hold("preinitialize proof",h.reason());
            if(!h.initialize_bridge_rw(bridge_,code_)) {
                std::array<char,512> detail{};
                std::snprintf(detail.data(),detail.size(),"%s",h.reason()?h.reason():"Host detail unavailable");
                const auto deferred=h.preinitialize_proof(false);
                if(!preparation_clock(h))return hold("Bridge initialize proof exceeded preparation deadline; no retry");
                if(deferred==PreinitializeProof::defer)return preparation_wait(h);
                return bridge_hold("initialize",detail.data());
            }
            if(!preparation_clock(h))return hold("Bridge initialization exceeded preparation deadline; no retry");
            if(!h.seal_bridge_rx(bridge_,code_))return bridge_hold("seal",h.reason());
            if(!preparation_clock(h))return hold("Bridge seal exceeded preparation deadline; no retry");
            if(!h.bridge_is_private_rx(bridge_,code_))return bridge_hold("RX",h.reason());
            if(!preparation_clock(h))return hold("Bridge RX proof exceeded preparation deadline; no retry");
            if(!h.journal_bridge_ready(binding_,bridge_,code_))return bridge_hold("ready",h.reason());
            if(!preparation_clock(h))return hold("Bridge ready receipt exceeded preparation deadline; no retry");
            phase_=Phase::prepared;
        }
        if(!h.bridge_is_private_rx(bridge_,code_))return hold("Retained private RX bridge changed");
        const auto stock=expected();auto labeled=stock;labeled.label=kWomen;
        const auto label=step(h,{Word::label,base_+label_rva_+8,kEuro,kWomen},stock,labeled,true);
        if(label.after)label_owned_=true;
        if(!label.ok)return failed_publication(h,label);
        auto active=labeled;active.dispatch=bridge_dispatch_;
        const auto dispatch=step(h,{Word::dispatch,base_+kEntry,std::uint32_t(kStockDispatch),std::uint32_t(bridge_dispatch_)},labeled,active,true);
        if(dispatch.after)dispatch_owned_=true;
        if(!dispatch.ok)return failed_publication(h,dispatch);
        generation_=in.roster_generation;phase_=Phase::active;
        reason_="Exact owned Blacktop category route published; gameplay unverified";
        return Result::published;
    }
public:
    Session(Binding binding,std::uintptr_t base,std::uintptr_t label_rva):
        binding_(std::move(binding)),base_(base),label_rva_(label_rva){}
    Phase phase()const{return phase_;}
    const char* reason()const{return reason_?reason_:bridge_reason_.data();}
    std::uintptr_t bridge()const{return bridge_;}
    bool owned_route()const{return label_owned_||dispatch_owned_;}
    bool uncertain()const{return uncertain_;}
    unsigned preparation_attempts()const{return preparation_attempts_;}
    template<class Host> Result tick(const Input& in,Host& h) {
        try{return tick_impl(in,h);}catch(...){uncertain_=true;return hold("Exception during category operation; no retry, allocation retained");}
    }
};
} // namespace wnba_blacktop_category
