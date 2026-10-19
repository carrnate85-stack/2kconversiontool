#pragma once
// No Windows APIs or game addresses. The native adapter implements these
// operations; this state machine owns the exactly-once transfer boundary.
#include <cstdint>
#include <string>
#include <utility>
#include <vector>
namespace outfit_match_v2 {
struct Choice {
    std::uintptr_t original{};
    std::vector<std::wstring> items;
};
enum class Phase { clone_intent, clone_owned, expand_intent, packet_intent,
    packet_owned, transfer_intent, transferred, dispose_packet, dispose_clone,
    complete, stopped };
enum class Transfer { not_entered, witnessed, uncertain };
enum class Completion { rebuilt, failed, held };
// Receipt polling changes only how soon the worker re-observes stock-owned
// replacement. The queue/actor/items/idle/ownership proofs remain mandatory.
inline constexpr std::uint64_t kReceiptPollMs=20;
inline std::uint64_t receipt_poll_delay(std::uint64_t now,std::uint64_t deadline) {
    if(now>=deadline)return 0;
    const auto remaining=deadline-now;
    return remaining<kReceiptPollMs?remaining:kReceiptPollMs;
}

struct Outcome {
    Completion state{Completion::failed};
    unsigned rebuilt{};
    bool gameplay_verified{false};
    std::string reason;
};
struct Resources {
    std::uintptr_t clone{}, packet{};
    bool clone_proved{}, packet_proved{}, transfer_attempted{};
};
enum class ExpansionState { ordinary, prepared, mutating, expanded, uncertain };
inline bool expansion_clone_proof_allowed(ExpansionState state) {
    return state==ExpansionState::ordinary || state==ExpansionState::prepared || state==ExpansionState::expanded;
}
struct ExpansionFailureGuard {
    ExpansionState& state;
    ~ExpansionFailureGuard(){if(state==ExpansionState::mutating)state=ExpansionState::uncertain;}
};
struct ExpansionArrayBounds {
    std::uintptr_t exposed{},owned{},source_exposed{},source_owned{};
    unsigned count{},source_count{};
    std::uintptr_t actor{},source_description{},clone{},owner{},source_owner{};
    std::size_t actor_bytes{},description_bytes{};
    std::vector<std::uintptr_t> source_strings;
};
inline bool independent_expansion_arrays(const ExpansionArrayBounds& b) {
    constexpr std::uintptr_t limit=0x7FFF00000000ull;
    struct Span {std::uintptr_t at{};std::size_t bytes{};};
    auto bounded=[&](Span span){return span.at>=0x10000 && span.bytes && span.bytes<=65536 &&
        span.at<limit && span.bytes<=limit-span.at;};
    auto separate=[&](Span a,Span c){return bounded(a) && bounded(c) &&
        (a.at+a.bytes<=c.at || c.at+c.bytes<=a.at);};
    if(!b.count || b.count>7 || !b.source_count || b.source_count>7 ||
       b.source_strings.size()!=b.source_count)return false;
    const Span exposed{b.exposed,b.count*8u},owned{b.owned,b.count*8u};
    if(!separate(exposed,owned))return false;
    const Span excluded[]={{b.source_exposed,b.source_count*8u},{b.source_owned,b.source_count*8u},
        {b.actor,b.actor_bytes},{b.source_description,b.description_bytes},{b.clone,b.description_bytes},
        {b.owner,0x28},{b.source_owner,0x28}};
    for(const auto& range:excluded)if(!separate(exposed,range) || !separate(owned,range))return false;
    for(auto at:b.source_strings)if(!separate(exposed,{at,512}) || !separate(owned,{at,512}))return false;
    return true;
}
enum class ExpansionStage { release_init, set_bind };
enum class ExpansionAction { release_list, initialize_list, set_item, bind_list };
struct ExpansionCall {
    ExpansionAction action{};
    std::uintptr_t function{},target{},argument{};
    unsigned count{},index{};
    std::wstring name;
    bool operator==(const ExpansionCall&) const = default;
};
struct ExpansionPlan {
    ExpansionStage stage{};
    std::uintptr_t base{},original{},clone{},owner{},heap{},actor{},source{},row{},manager{},loaded{},state{},exposed{},owned{};
    unsigned count{};
    std::vector<std::wstring> items;
    std::vector<ExpansionCall> calls;
    bool operator==(const ExpansionPlan&) const = default;
};
// Only a Transport that completed exact durable bundle readback can mint this.
// No serialized input or recovery record can construct an execution capability.
class ExpansionPermit {
    friend class Transport;
    const void* transport_{};
    ExpansionPlan plan_;
    std::size_t cursor_{};
    bool failed_{},inflight_{},attempted_{};
    ExpansionPermit(const void* transport,ExpansionPlan plan):transport_(transport),plan_(std::move(plan)){}
public:
    ExpansionPermit(const ExpansionPermit&)=delete;
    ExpansionPermit& operator=(const ExpansionPermit&)=delete;
    ExpansionPermit& operator=(ExpansionPermit&&)=delete;
    ExpansionPermit(ExpansionPermit&& other) noexcept:
        transport_(std::exchange(other.transport_,nullptr)),plan_(std::move(other.plan_)),
        cursor_(other.cursor_),failed_(other.failed_),inflight_(other.inflight_),attempted_(other.attempted_) {
        other.failed_=true;
    }
    const ExpansionPlan& plan()const{return plan_;}
    bool complete()const{return transport_ && !failed_ && !inflight_ && cursor_==plan_.calls.size();}
    bool attempted()const{return attempted_;}
    bool failed()const{return failed_;}
    template<class Guard,class Effect>
    bool invoke(const void* transport,const ExpansionCall& call,Guard&& guard,Effect&& effect) {
        if(!transport_ || transport!=transport_ || failed_ || inflight_ || cursor_>=plan_.calls.size() ||
           call!=plan_.calls[cursor_]) {failed_=true;return false;}
        // Guard callbacks are inside the same single-use critical section.
        // Reentry poisons the outer attempt before any native effect.
        inflight_=true;
        try {
            if(!guard() || failed_){inflight_=false;failed_=true;return false;}
            // Attempted marks the effect boundary, never just the guard.
            attempted_=true;
            const bool result=effect();
            inflight_=false;
            if(!result || failed_){failed_=true;return false;}
            ++cursor_;return true;
        }catch(...){inflight_=false;failed_=true;return false;}
    }
#ifdef OUTFIT_MATCH_V2_OFFLINE_TEST
    static ExpansionPermit fixture(const void* transport,ExpansionPlan plan) {
        return ExpansionPermit(transport,std::move(plan));
    }
#endif
};
class Operations {
public:
    virtual ~Operations() = default;
    virtual bool authorized() = 0;
    virtual bool all_bindings(const std::vector<Choice>&) = 0;
    virtual bool bind(const Choice&) = 0;
    virtual bool durable(Phase, const Resources&, const Choice&) = 0;
    virtual std::uintptr_t clone() = 0;
    virtual bool prove_clone(std::uintptr_t) = 0;
    virtual bool expand(std::uintptr_t, const Choice&) = 0;
    virtual std::uintptr_t allocate_packet() = 0;
    virtual bool prepare_packet(std::uintptr_t, std::uintptr_t) = 0;
    // Must set transfer_attempted BEFORE calling the native insert function.
    // It may set it only while holding the native queue lock, after full guards.
    virtual Transfer transfer(Resources&) = 0;
    virtual bool observe(const Choice&) = 0;
    virtual bool dispose_packet(std::uintptr_t) = 0;
    virtual bool dispose_clone(std::uintptr_t) = 0;
};
inline bool valid_choices(const std::vector<Choice>& choices) {
    if (choices.empty() || choices.size() > 10) return false;
    for (std::size_t n = 0; n < choices.size(); ++n) {
        const auto& c = choices[n];
        // Compatible BOP3 transport deliberately retains its 3-piece minimum.
        if (c.original < 0x10000 || c.original >= 0x7FFF00000000ull || c.original % 8 || c.items.size() < 3 || c.items.size() > 7)
            return false;
        for (std::size_t x = 0; x < n; ++x) if (choices[x].original == c.original) return false;
        for (const auto& item : c.items) {
            if (item.empty() || item.size() >= 256) return false;
            if (item == L"NO SHIRT") continue;
            for (wchar_t ch : item)
                if (!((ch >= L'a' && ch <= L'z') || (ch >= L'A' && ch <= L'Z') ||
                    (ch >= L'0' && ch <= L'9') || ch == L'_')) return false;
        }
    }
    return true;
}
// A consumed durable plan is supplied by the pinned native transport. A raw
// browser request, a log report, or the old worker cannot manufacture this type.
class ConsumedPlan {
    friend class Transport;
    std::vector<Choice> choices_;
    bool claimed_{false};
    explicit ConsumedPlan(std::vector<Choice> c) : choices_(std::move(c)) {}
public:
    ConsumedPlan(const ConsumedPlan&) = delete;
    ConsumedPlan& operator=(const ConsumedPlan&) = delete;
    ConsumedPlan(ConsumedPlan&& o) noexcept : choices_(std::move(o.choices_)),claimed_(std::exchange(o.claimed_,true)) {}
    const std::vector<Choice>& choices() const { return choices_; }
    bool claim() { return !std::exchange(claimed_,true); }
#ifdef OUTFIT_MATCH_V2_OFFLINE_TEST
    static ConsumedPlan fixture(std::vector<Choice> c) { return ConsumedPlan(std::move(c)); }
#endif
};
template<class State,unsigned Required,std::uint64_t Interval> class StableBoundary {
    State state_{};
    unsigned count_{};
    std::uint64_t last_{};
public:
    bool observe(bool accepted,const State& state,std::uint64_t now) {
        if(!accepted) {count_=0;return false;}
        if(!count_ || state!=state_ || now<last_) {state_=state;count_=1;last_=now;}
        else if(count_<Required && now-last_>=Interval) {++count_;last_=now;}
        return count_>=Required;
    }
};
inline Outcome execute(Operations& op, ConsumedPlan plan) {
    Outcome out{};
    if (!plan.claim() || !valid_choices(plan.choices()) || !op.authorized() || !op.all_bindings(plan.choices())) {
        out.reason = "Plan or exact loaded-player ownership is unsupported"; return out;
    }
    for (const auto& choice : plan.choices()) {
        Resources r{};
        auto stop = [&](const char* reason) {
            out.reason = reason;
            if (r.transfer_attempted) {
                out.state = Completion::held; return;
            }
            bool clean = true;
            // Lost authority, unreadable memory or aliases never justify a
            // speculative destructor. Bounded allocations remain until exit.
            if (r.packet) {
                clean = r.packet_proved && op.authorized() &&
                    op.durable(Phase::dispose_packet, r, choice) && op.dispose_packet(r.packet);
            }
            if (r.clone) {
                const bool freed = r.clone_proved && op.authorized() &&
                    op.durable(Phase::dispose_clone, r, choice) && op.dispose_clone(r.clone);
                clean = clean && freed;
            }
            out.state = clean ? Completion::failed : Completion::held;
        };
        try {
            if (!op.authorized() || !op.bind(choice) || !op.durable(Phase::clone_intent,r,choice)) {
                stop("Binding or durable clone intent was rejected"); return out;
            }
            r.clone = op.clone();
            if (!r.clone) { stop("Native clone allocation failed"); return out; }
            r.clone_proved = op.prove_clone(r.clone);
            if (!r.clone_proved || !op.durable(Phase::clone_owned,r,choice)) {
                stop("Private clone ownership is unproven"); return out;
            }
            if (!op.authorized() || !op.durable(Phase::expand_intent,r,choice) || !op.expand(r.clone,choice)) {
                stop("Private clone expansion did not verify"); return out;
            }
            if (!op.authorized() || !op.durable(Phase::packet_intent,r,choice)) {
                stop("Durable packet allocation intent was rejected"); return out;
            }
            r.packet = op.allocate_packet();
            if (!r.packet) { stop("Native request allocation failed"); return out; }
            r.packet_proved = op.prepare_packet(r.packet,r.clone);
            if (!r.packet_proved || !op.durable(Phase::packet_owned,r,choice) || !op.authorized() ||
                !op.durable(Phase::transfer_intent,r,choice)) {
                stop("Request ownership or durable transfer intent was rejected"); return out;
            }
            const auto transferred = op.transfer(r);
            if (transferred == Transfer::not_entered && !r.transfer_attempted) {
                stop("Queue changed before transfer; request not inserted"); return out;
            }
            // Treat contradictory or partial adapter output conservatively.
            r.transfer_attempted = true;
            if (transferred != Transfer::witnessed || !op.durable(Phase::transferred,r,choice)) {
                stop("Stock queue transfer uncertain; no retry or manual cleanup"); return out;
            }
            // Packet and clone are now stock-owned. Do not free or read them.
            if (!op.observe(choice) || !op.durable(Phase::complete,r,choice)) {
                stop("Native replacement was not confirmed; no forced rollback"); return out;
            }
            ++out.rebuilt;
        } catch (...) {
            // The adapter is noexcept in production. Never run a destructor
            // after an unexpected native failure or uncertain allocation result.
            out.state = Completion::held;
            out.reason = "Unexpected worker failure; no speculative native cleanup";
            return out;
        }
    }
    out.state = Completion::rebuilt;
    out.reason = "Native replacement observed; visual/gameplay acceptance remains required";
    return out;
}
} // namespace outfit_match_v2
