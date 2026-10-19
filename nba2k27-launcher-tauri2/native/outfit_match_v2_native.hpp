#pragma once
// Explicitly unwired Version2-only in-process helper. No remote-thread,
// executable-page or vtable patching. This header never starts a worker itself.
#include "outfit_match_v2_transport.hpp"
#include <memory>
namespace outfit_match_v2 {
struct Description {
    std::uintptr_t at{},array{},owner{},owned{};
    unsigned count{};
    std::array<unsigned char,kDescriptionBytes> bytes{};
    std::vector<std::uintptr_t> strings;
    std::vector<std::wstring> names;
    bool operator==(const Description&) const = default;
};
inline std::uintptr_t field(const Description& s,std::size_t offset) {
    std::uintptr_t v{};std::memcpy(&v,s.bytes.data()+offset,8);return v;
}
inline bool wardrobe(std::uintptr_t base,std::uintptr_t at,Description& out) {
    Description s{};s.at=at;
    if(!private_span(at,kDescriptionBytes) || !read_memory(at,s.bytes.data(),s.bytes.size()) ||
        field(s,0)!=base+kPrivate || field(s,0xB8)!=at+0x378 || (s.bytes[0x7D]&1)) return false;
    s.array=field(s,0xC8);std::memcpy(&s.count,s.bytes.data()+0xD0,4);s.owner=field(s,0x368);
    if(s.count<1 || s.count>7 || !private_span(s.owner,0x28) || q(s.owner)!=base+kVariableOutfit ||
        d(s.owner+8)!=s.count || q(s.owner+0x10)!=s.array) return false;
    s.owned=q(s.owner+0x20);
    if(s.array==s.owned || !private_span(s.array,s.count*8) || !private_span(s.owned,s.count*8)) return false;
    for(unsigned n=0;n<s.count;++n) {
        const auto p=q(s.array+n*8);std::array<wchar_t,256> name{};
        if(q(s.owned+n*8)!=p || std::find(s.strings.begin(),s.strings.end(),p)!=s.strings.end() ||
            !private_span(p,512) || !read_memory(p,name.data(),512)) return false;
        const auto end=std::find(name.begin(),name.end(),wchar_t(0));
        if(end==name.begin() || end==name.end()) return false;
        std::wstring item(name.begin(),end);
        if(item!=L"NO SHIRT") for(auto c:item) if(!((c>=L'a'&&c<=L'z')||(c>=L'A'&&c<=L'Z')||(c>=L'0'&&c<=L'9')||c==L'_')) return false;
        s.strings.push_back(p);s.names.push_back(std::move(item));
    }
    std::array<unsigned char,kDescriptionBytes> after{};
    if(s.names[0]==L"VTXANI_JERSEY" || !read_memory(at,after.data(),after.size()) || after!=s.bytes) return false;
    out=std::move(s);return true;
}
inline bool independent(const Description& source,const Description& copy,std::uintptr_t actor) {
    if(source.at==copy.at || (copy.at>=actor && copy.at<actor+kActorBytes) ||
        copy.array==source.array || copy.array==source.owned || copy.owned==source.array || copy.owned==source.owned ||
        copy.owner==source.owner) return false;
    constexpr unsigned owned[]={0x368,0x370,0x2940,0x2948,0x2950,0x2958};
    for(auto x:owned) for(auto y:owned) if(field(source,x) && field(source,x)==field(copy,y)) return false;
    for(auto x:owned) for(auto y:owned) if(x!=y && field(copy,x) && field(copy,x)==field(copy,y)) return false;
    for(auto x:owned) {const auto p=field(copy,x);
        if((p>=actor && p<actor+kActorBytes) || (p>=source.at && p<source.at+kDescriptionBytes) ||
            (p>=copy.at && p<copy.at+kDescriptionBytes)) return false;}
    for(auto a:source.strings) for(auto b:copy.strings) if(a==b) return false;
    // The destructor can walk this additional intrusive list. Until all its
    // possible node types are audited, only the independently empty clone list
    // is supported. Never destroy a live actor's embedded list.
    const auto head=copy.at+0x350;
    return field(copy,0x358)==head && field(copy,0x360)==head &&
        private_span(copy.at,kDescriptionBytes,true) && private_span(copy.owner,0x28,true);
}
inline bool expanded(const Description& before,const Description& after,const Choice& choice) {
    // WPF expanded_copy_matches: only an ordinary two-item copy is expanded.
    if(before.count!=2 || before.names.size()!=2 || before.at!=after.at || before.owner!=after.owner || after.count!=choice.items.size() || after.names!=choice.items) return false;
    auto normalized=after.bytes;
    std::copy(before.bytes.begin()+0xC8,before.bytes.begin()+0xD4,normalized.begin()+0xC8);
    return normalized==before.bytes;
}
struct Context {
    unsigned index{},alternate{},mode{},other{},stage{},kind{},game_mode{};
    unsigned char active{};
    std::uintptr_t owner{},root_tag{};
    bool operator==(const Context&) const = default;
};
inline bool context(std::uintptr_t base,Context& c) {
    Context v{};v.index=d(base+kCurrent);v.alternate=d(base+kAlternate);
    v.owner=q(base+kOwner);v.stage=d(base+kRoot+0xEA8);v.root_tag=q(base+kRoot);
    unsigned char kind{};
    if(v.index>=128 || v.alternate>=128 || !private_span(v.owner,0xA9C) ||
        !value(v.owner,kind) || !value(base+kActiveGame,v.active)) return false;
    v.kind=kind;v.game_mode=d(v.owner+0xA04);
    v.mode=d(base+kModes+v.index*64);v.other=d(base+kModes+v.alternate*64);
    c=v;return true;
}
enum class Boundary { selection, match, menu, other };
inline Boundary boundary(std::uintptr_t base,const Context& c) {
    if(c.mode!=c.other) return Boundary::other;
    if(c.mode==0x92DF06DE && c.stage==3 && c.active==0 && c.kind==0 && c.root_tag==base+kRootTag) return Boundary::selection;
    if(c.stage==1 && c.active==0 && c.kind==0 && (c.mode==0x6509837B || c.mode==0x079964C0)) return Boundary::menu;
    if(c.game_mode==6 && c.stage==1 && (c.mode==0xB4D37B22 || c.mode==0x3EEF98B9 || c.mode==0x7C602DF8 || c.mode==0x50BAD898)) return Boundary::match;
    return Boundary::other;
}
inline bool queue(std::uintptr_t base,bool empty) {
    unsigned char initialized{};const auto head=base+kQueueHead;
    return q(base+kQueue)==base+kQueueTag && q(head)==base+kQueueHeadTag &&
        value(base+kQueue+0x5A,initialized) && initialized==1 &&
        (!empty || (q(head+8)==head && q(head+0x10)==head));
}
inline bool lineup(std::uintptr_t base,std::vector<std::uintptr_t>& out) {
    const auto count=d(base+kLineup+0xB8),required=d(base+kLineup+4);
    if(required<2 || required>10 || required%2 || count!=required || d(base+kLineup+8)!=required) return false;
    std::vector<std::uintptr_t> rows;
    for(unsigned n=0;n<count;++n) {
        const auto roster=q(base+kLineup+0xC0+n*8);
        if(roster<0x10000 || roster>=0x7FFF00000000ull || roster%8 || std::find(rows.begin(),rows.end(),roster)!=rows.end()) return false;
        rows.push_back(roster);
    }
    if(d(base+kLineup+0xB8)!=count || d(base+kLineup+4)!=required || d(base+kLineup+8)!=required) return false;
    for(unsigned n=0;n<count;++n) if(q(base+kLineup+0xC0+n*8)!=rows[n]) return false;
    out=std::move(rows);return true;
}
inline bool active_manager(std::uintptr_t base,std::uintptr_t manager) {
    const auto head=base+kActiveManagers;
    if(q(head)!=base+kActiveHeadTag) return false;
    auto node=q(head+0x10),previous=head;unsigned matches{};
    for(unsigned i=0;i<256;++i) {
        if(node==head) return matches==1 && q(head+8)==previous;
        if(!private_span(node,0x18) || q(node+8)!=previous) return false;
        matches+=node==manager;previous=node;node=q(node+0x10);
    }
    return false;
}
inline bool idle(std::uintptr_t base,std::uintptr_t manager,std::uintptr_t actor,unsigned label) {
    unsigned char flag=1;
    if(!private_span(manager,0x148) || q(manager)!=base+kManager || q(manager+0x78)!=base+kSecondary ||
        q(manager+0x110)!=actor || q(manager+0x118) || d(manager+0x90)!=1 ||
        !value(manager+0x130,flag) || flag || !active_manager(base,manager)) return false;
    const auto head=manager+0xF8;
    if(q(head)!=base+kManagedHead) return false;
    auto node=q(head+0x10),previous=head;unsigned matches{};
    for(unsigned i=0;i<16;++i) {
        if(node==head) return matches==1 && q(head+8)==previous;
        if(!private_span(node,0x40) || q(node)!=base+kManaged || q(node+8)!=previous || q(node+0x38)) return false;
        if(d(node+0x18)==label) {if(q(node+0x20)!=actor) return false;++matches;}
        previous=node;node=q(node+0x10);
    }
    return false;
}
struct Player {
    // table: the raw manager pointer stored at row+0x10 and loaded+0x518. Live
    // V2 stores the MANAGER_INSTANCE interface base (kSecondary) there; primary
    // is always the full object (scaler_v2 applies the same -0x78 adjustment).
    std::uintptr_t row{},table{},primary{},actor{},loaded{},original{},state{},context{};
    unsigned label{},main_thread{},tag{},line{};
    Context mode{};Description source{};
    std::vector<std::uintptr_t> lineup;
};
inline bool find_player(std::uintptr_t base,const Choice& choice,Player& result) {
    Context before{};std::vector<std::uintptr_t> selected;
    if(!context(base,before) || boundary(base,before)!=Boundary::match || !queue(base,true) || !lineup(base,selected) ||
        std::find(selected.begin(),selected.end(),choice.original)==selected.end()) return false;
    unsigned count{};Player found{};
    for(unsigned n=0;n<30;++n) {
        const auto row=base+kTable+n*0x50;
        if(q(row)!=base+kWrapper) continue;
        const auto loaded=q(row+0x28),table=q(row+0x10);
        if(!loaded || !table || !private_span(loaded,0x528) || q(loaded+0x500)!=choice.original) continue;
        if(++count!=1 || !private_span(table,8) || q(loaded+0x518)!=table) return false;
        const auto primary=q(table)==base+kSecondary?table-0x78:table;
        if(!private_span(primary,0x148) || q(primary)!=base+kManager || q(primary+0x78)!=base+kSecondary) return false;
        Player p{};p.row=row;p.table=table;p.loaded=loaded;p.primary=primary;p.original=choice.original;p.lineup=selected;
        p.state=q(row+0x18);p.actor=q(primary+0x110);p.context=q(primary+0x98);p.mode=before;
        p.tag=d(primary+0x80);p.line=d(primary+0x84);p.main_thread=d(base+kMainThread);
        unsigned char ready{};
        if(!p.state || q(loaded+0x520)!=p.state || !value(row+0x20,ready) || ready!=1 ||
            !p.context || !private_span(p.context,0x20) || !q(p.context) || !q(p.context+0x18) ||
            !p.main_thread || p.main_thread==~0u || p.main_thread==GetCurrentThreadId() ||
            !private_span(p.actor,kActorBytes) || q(p.actor)!=base+kPlayer || !wardrobe(base,p.actor+0xF30,p.source)) return false;
        // WPF parity (audit D3): only an ordinary two-item live description is a
        // carry source, re-proven in-DLL even though the launcher checked it.
        if(p.source.count!=2 || p.source.names.size()!=2) return false;
        p.label=d(p.source.at+0x88);
        if(!p.label || p.label==~0u || !idle(base,primary,p.actor,p.label)) return false;
        found=std::move(p);
    }
    Context after{};std::vector<std::uintptr_t> final_lineup;
    if(count!=1 || !context(base,after) || after!=before || !lineup(base,final_lineup) || final_lineup!=selected) return false;
    result=std::move(found);return true;
}
class NativeOperations final : public Operations {
    Authority& authority_;Transport& transport_;Player player_{};Choice choice_{};
    Description copy_{};std::uintptr_t clone_{},packet_{},heap_{};
    std::array<std::uintptr_t,8> packet_bytes_{};
    bool expanded_{false};
    ExpansionState expansion_state_{ExpansionState::ordinary};
    std::optional<ExpansionPermit> expansion_a_,expansion_b_;
    ExpansionPlan expansion_plan(ExpansionStage stage,const Choice& c)const {
        ExpansionPlan p{};p.stage=stage;p.base=authority_.base();p.original=player_.original;
        p.clone=clone_;p.owner=copy_.owner;p.heap=heap_;p.actor=player_.actor;p.source=player_.source.at;
        p.row=player_.row;p.manager=player_.primary;p.loaded=player_.loaded;p.state=player_.state;
        p.count=unsigned(c.items.size());p.items=c.items;
        if(stage==ExpansionStage::set_bind){p.exposed=q(copy_.owner+0x10);p.owned=q(copy_.owner+0x20);}
        p.calls=expansion_calls(p);return p;
    }
    bool binding(bool original_actor=true) {
        const auto b=authority_.base();Context now{};std::vector<std::uintptr_t> selected;
        if(!authorized() || !context(b,now) || now!=player_.mode || boundary(b,now)!=Boundary::match ||
            q(player_.row)!=b+kWrapper || q(player_.row+0x10)!=player_.table || q(player_.row+0x28)!=player_.loaded ||
            q(player_.row+0x18)!=player_.state || q(player_.loaded+0x518)!=player_.table ||
            q(player_.loaded+0x520)!=player_.state || q(player_.loaded+0x500)!=player_.original ||
            q(player_.primary)!=b+kManager || q(player_.primary+0x78)!=b+kSecondary ||
            q(player_.primary+0x98)!=player_.context || d(player_.primary+0x80)!=player_.tag ||
            d(player_.primary+0x84)!=player_.line || d(b+kMainThread)!=player_.main_thread ||
            !lineup(b,selected) || selected!=player_.lineup) return false;
        unsigned char ready{};
        if(!value(player_.row+0x20,ready) || ready!=1) return false;
        if(!original_actor) return true;
        Description source{};
        return q(player_.primary+0x110)==player_.actor && q(player_.actor)==b+kPlayer &&
            wardrobe(b,player_.source.at,source) && source==player_.source &&
            idle(b,player_.primary,player_.actor,player_.label);
    }
    bool clone_owned() {
        Description now{};
        return expansion_clone_proof_allowed(expansion_state_) &&
            binding() && clone_ && wardrobe(authority_.base(),clone_,now) && now==copy_ &&
            independent(player_.source,now,player_.actor);
    }
    bool effect(Phase phase) {
        Resources r{};r.clone=clone_;r.packet=packet_;r.clone_proved=clone_!=0;r.packet_proved=packet_!=0;
        return binding() && transport_.event(phase,r,choice_);
    }
public:
    NativeOperations(Authority& a,Transport& t) : authority_(a),transport_(t) {}
    bool authorized() override {
        return authority_.safe() && transport_.belongs(authority_.binding()) && transport_.pinned_plan() && profile(authority_.base()) &&
            d(authority_.base()+kMainThread)!=0 && d(authority_.base()+kMainThread)!=GetCurrentThreadId();
    }
    bool all_bindings(const std::vector<Choice>& choices) override {
        if(!authorized()) return false;
        std::vector<Player> players;
        for(const auto& c:choices) {
            Player p{};if(!find_player(authority_.base(),c,p)) return false;
            for(const auto& other:players) if(p.primary==other.primary || p.actor==other.actor || p.loaded==other.loaded || p.state==other.state) return false;
            players.push_back(std::move(p));
        }
        for(const auto& p:players) {
            Player again{};Choice c{};c.original=p.original;
            if(!find_player(authority_.base(),c,again) || p.row!=again.row || p.primary!=again.primary || p.actor!=again.actor ||
                p.loaded!=again.loaded || p.state!=again.state || p.source!=again.source) return false;
        }
        return true;
    }
    bool bind(const Choice& c) override {
        choice_=c;clone_=packet_=heap_=0;expanded_=false;copy_={};
        expansion_a_.reset();expansion_b_.reset();expansion_state_=ExpansionState::ordinary;
        return authorized() && find_player(authority_.base(),c,player_) && binding();
    }
    bool durable(Phase phase,const Resources& r,const Choice& c) override {
        if(phase!=Phase::expand_intent)return authority_.safe() && transport_.event(phase,r,c);
        if(expansion_state_!=ExpansionState::ordinary || expansion_a_ || expansion_b_ ||
           r.clone!=clone_ || !r.clone_proved || r.packet || r.transfer_attempted ||
           c.original!=choice_.original || c.items!=choice_.items || !clone_owned())return false;
        auto permit=transport_.expansion_bundle(expansion_plan(ExpansionStage::release_init,c));
        if(!permit)return false;
        expansion_a_.emplace(std::move(*permit));expansion_state_=ExpansionState::prepared;return true;
    }
    std::uintptr_t clone() override {
        if(!binding() || !queue(authority_.base(),true)) return 0;
        heap_=q(authority_.base()+kHeap);
        if(!heap_ || !q(heap_)) return 0;
        using Clone=void*(*)(void*,void*);
        clone_=reinterpret_cast<std::uintptr_t>(reinterpret_cast<Clone>(authority_.base()+kClone)(
            reinterpret_cast<void*>(player_.source.at),reinterpret_cast<void*>(heap_)));
        return clone_;
    }
    bool prove_clone(std::uintptr_t at) override {
        return at==clone_ && binding() && wardrobe(authority_.base(),at,copy_) && copy_.count==2 &&
            independent(player_.source,copy_,player_.actor) && copy_.names==player_.source.names;
    }
    bool expand(std::uintptr_t at,const Choice& c) override {
        if(at!=clone_ || expansion_state_!=ExpansionState::prepared || !expansion_a_ ||
           c.original!=choice_.original || c.items!=choice_.items || !clone_owned())return false;
        // After the first attempted Release, the ordinary-clone destructor proof
        // is gone. Any partial/unknown expansion is retained, never retried.
        ExpansionFailureGuard failure{expansion_state_};
        const auto before=copy_;const auto b=authority_.base();const auto count=c.items.size();
        const auto first=expansion_a_->plan();
        if(first!=expansion_plan(ExpansionStage::release_init,c))return false;
        auto clone_owner=[&](){return private_span(at,kDescriptionBytes,true) && q(at)==b+kPrivate && q(at+0x368)==before.owner;};
        auto owner_empty=[&](){return binding() && clone_owner() && private_span(before.owner,0x28,true) &&
            q(before.owner)==b+kVariableOutfit && !d(before.owner+8) &&
            !q(before.owner+0x10) && !q(before.owner+0x20);};
        using Release=void*(*)(void*,unsigned);
        if(!expansion_a_->invoke(&transport_,first.calls[0],
            [&](){return clone_owned();},[&](){
                expansion_state_=ExpansionState::mutating;
                reinterpret_cast<Release>(b+kReleaseList)(reinterpret_cast<void*>(before.owner),0);
                return owner_empty();
            }))return false;
        using Init=void(*)(void*,int,void*);
        if(!expansion_a_->invoke(&transport_,first.calls[1],
            [&](){return owner_empty() && q(b+kHeap)==heap_;},[&](){
                reinterpret_cast<Init>(b+kInitList)(reinterpret_cast<void*>(before.owner),int(count),reinterpret_cast<void*>(heap_));
                return binding();
            }) || !expansion_a_->complete())return false;
        const auto exposed=q(before.owner+0x10),owned=q(before.owner+0x20);
        auto arrays=[&](){return binding() && clone_owner() && private_span(before.owner,0x28,true) && q(before.owner)==b+kVariableOutfit &&
            d(before.owner+8)==count && q(before.owner+0x10)==exposed && q(before.owner+0x20)==owned &&
            independent_expansion_arrays({exposed,owned,player_.source.array,player_.source.owned,unsigned(count),player_.source.count,
                player_.actor,player_.source.at,at,before.owner,player_.source.owner,kActorBytes,kDescriptionBytes,player_.source.strings}) &&
            private_span(exposed,count*8,true) && private_span(owned,count*8,true);};
        if(!arrays())return false;
        for(unsigned i=0;i<count;++i)if(q(owned+i*8))return false;
        const auto second=expansion_plan(ExpansionStage::set_bind,c);
        if(second.exposed!=exposed || second.owned!=owned)return false;
        auto permit=transport_.expansion_bundle(second,&*expansion_a_);
        if(!permit)return false;
        expansion_b_.emplace(std::move(*permit));
        using Set=void(*)(void*,const wchar_t*,int);
        for(unsigned i=0;i<count;++i) {
            const auto& call=second.calls[i];
            if(!expansion_b_->invoke(&transport_,call,arrays,[&](){
                reinterpret_cast<Set>(b+kSetItem)(reinterpret_cast<void*>(before.owner),call.name.c_str(),int(i));
                return true;
            }))return false;
        }
        Description after{};using Bind=void(*)(void*,void*,unsigned);
        if(!expansion_b_->invoke(&transport_,second.calls.back(),arrays,[&](){
            reinterpret_cast<Bind>(b+kBind)(reinterpret_cast<void*>(at),reinterpret_cast<void*>(exposed),unsigned(count));
            return wardrobe(b,at,after) && independent(player_.source,after,player_.actor) &&
                outfit_match_v2::expanded(before,after,c);
        }) || !expansion_b_->complete())return false;
        copy_=std::move(after);expanded_=true;expansion_state_=ExpansionState::expanded;
        if(!clone_owned()){expansion_state_=ExpansionState::uncertain;return false;}
        return true;
    }
    std::uintptr_t allocate_packet() override {
        if(!expanded_ || !clone_owned() || q(authority_.base()+kHeap)!=heap_) return 0;
        using Allocate=void*(*)(std::size_t,void*,unsigned,unsigned,unsigned,unsigned,bool,bool);
        packet_=reinterpret_cast<std::uintptr_t>(reinterpret_cast<Allocate>(authority_.base()+kAllocate)(
            0x40,reinterpret_cast<void*>(heap_),0,0xAD7FD911,0x67C,8,true,false));
        return packet_;
    }
    bool prepare_packet(std::uintptr_t at,std::uintptr_t copy) override {
        if(at!=packet_ || copy!=clone_ || !clone_owned() || !private_span(at,0x40,true) || at%8 ||
            (at>=clone_ && at<clone_+kDescriptionBytes) || (at>=player_.actor && at<player_.actor+kActorBytes) ||
            at==player_.primary || at==player_.source.owner || at==copy_.owner) return false;
        packet_bytes_={authority_.base()+kRequestTag,at,at,player_.primary,player_.context+0x18,0,0,
            std::uintptr_t(player_.tag)|(std::uintptr_t(player_.line)<<32)};
        if(!effect(Phase::packet_intent)) return false;
        std::memcpy(reinterpret_cast<void*>(at),packet_bytes_.data(),0x40);
        std::array<std::uintptr_t,8> now{};
        return read_memory(at,now.data(),0x40) && now==packet_bytes_;
    }
    Transfer transfer(Resources& resources) override {
        const auto b=authority_.base();
        if(resources.clone!=clone_ || resources.packet!=packet_ || !clone_owned() || !expanded_ || !queue(b,true)) return Transfer::not_entered;
        auto* lock=reinterpret_cast<CRITICAL_SECTION*>(b+kQueueLock);
        if(!TryEnterCriticalSection(lock)) return Transfer::not_entered;
        struct Leave { CRITICAL_SECTION* p;~Leave(){LeaveCriticalSection(p);} } leave{lock};
        std::array<std::uintptr_t,8> now{};
        if(!clone_owned() || !queue(b,true) || !private_span(packet_,0x40,true) ||
            !read_memory(packet_,now.data(),0x40) || now!=packet_bytes_) return Transfer::not_entered;
        // The already durable transfer intent authorizes these two writes. The
        // copy is independent, and the packet has not entered any queue yet.
        resources.transfer_attempted=true; // BEFORE any transfer-stage effect.
        *reinterpret_cast<unsigned char*>(clone_+0x7C)|=2;
        *reinterpret_cast<std::uintptr_t*>(packet_+0x28)=clone_;
        using Insert=void(*)(void*,void*);
        reinterpret_cast<Insert>(b+kInsert)(reinterpret_cast<void*>(b+kQueueHead),reinterpret_cast<void*>(packet_));
        const auto head=b+kQueueHead;
        return q(head+8)==packet_ && q(head+0x10)==packet_ && q(packet_+8)==head && q(packet_+0x10)==head &&
            q(packet_+0x28)==clone_ ? Transfer::witnessed : Transfer::uncertain;
    }
    bool observe(const Choice& c) override {
        const auto b=authority_.base();
        const auto end=GetTickCount64()+60000;bool drained=false;
        while(GetTickCount64()<end) {
            if(!binding(false) || !queue(b,false)) return false;
            auto* lock=reinterpret_cast<CRITICAL_SECTION*>(b+kQueueLock);
            if(TryEnterCriticalSection(lock)) {drained=drained||queue(b,true);LeaveCriticalSection(lock);}
            const auto actor=q(player_.primary+0x110);
            Description after{};
            if(drained && actor && actor!=player_.actor && q(actor)==b+kPlayer &&
                wardrobe(b,actor+0xF30,after) && after.names==c.items && idle(b,player_.primary,actor,player_.label) && binding(false)) return true;
            const auto delay=receipt_poll_delay(GetTickCount64(),end);
            if(!delay)break;
            Sleep(DWORD(delay));
        }
        return false;
    }
    bool dispose_packet(std::uintptr_t at) override {
        std::array<std::uintptr_t,8> now{};
        if(at!=packet_ || !binding() || !private_span(at,0x40,true) || !read_memory(at,now.data(),0x40) ||
            now!=packet_bytes_ || now[5]!=0 || now[1]!=at || now[2]!=at) return false;
        using Free=void(*)(void*);reinterpret_cast<Free>(authority_.base()+kFree)(reinterpret_cast<void*>(at));packet_=0;return true;
    }
    bool dispose_clone(std::uintptr_t at) override {
        if(at!=clone_ || !clone_owned()) return false;
        using Destroy=void*(*)(void*,unsigned);
        reinterpret_cast<Destroy>(authority_.base()+kDestroyClone)(reinterpret_cast<void*>(at),1);clone_=0;return true;
    }
};
// Blocking worker-thread API; the caller owns the thread and cancellation event.
// Never invoke in DllMain, the game thread, or a UI thread. Parent must keep the
// provider session callbacks alive until this call has returned.
inline Outcome run_one(Authority& authority,Transport& transport,ConsumedPlan plan) {
    NativeOperations operations(authority,transport);
    if(!transport.report("RUNNING","Version2 bounded outfit rebuild started",0))
        return {Completion::held,0,false,"Native report ownership failed"};
    auto outcome=execute(operations,std::move(plan));
    const auto* state=outcome.state==Completion::rebuilt?"REBUILT":outcome.state==Completion::held?"INCONCLUSIVE":"FAILED";
    if(!transport.report(state,outcome.reason,outcome.rebuilt)) {
        outcome.state=Completion::held;outcome.reason="Native result could not be durably reported";
    }
    return outcome;
}
// Explicitly created provider worker thread only. Every new match requires a
// fresh transport and four stable selection observations, >=500ms apart. There
// is no automatic choice generation, retry, or old-profile fallback.
inline Outcome run_session(const TrustedBinding& binding) {
    Authority authority;
    if(!authority.open(binding)) return {Completion::failed,0,false,authority.reason()};
    unsigned total{};
    for(unsigned match=1;match<=10000;++match) {
        Transport transport;
        if(!transport.open(binding,match)) return {Completion::held,total,false,"Owned match transport could not be created"};
        // WPF parity (audit D4): three identical 250 ms samples at match load.
        StableBoundary<Context,3,250> loaded;
        std::optional<ConsumedPlan> plan;
        while(authority.safe()) {
            Context now{};
            const bool match_context=context(authority.base(),now) && boundary(authority.base(),now)==Boundary::match;
            if(loaded.observe(match_context,now,GetTickCount64())) {
                auto candidate=transport.consume();
                if(candidate) {plan.emplace(std::move(*candidate));break;}
                if(transport.failed()) return {Completion::held,total,false,"Invalid or changed single-use outfit plan"};
            }
            Sleep(100);
        }
        if(!plan) return {Completion::failed,total,false,"Worker cancelled before an outfit request"};
        auto outcome=run_one(authority,transport,std::move(*plan));
        total+=outcome.rebuilt;
        if(outcome.state!=Completion::rebuilt) return outcome;
        StableBoundary<Context,4,500> selection;bool rearmed=false;
        while(authority.safe() && !rearmed) {
            Context now{};
            const bool selected=context(authority.base(),now) && boundary(authority.base(),now)==Boundary::selection;
            rearmed=selection.observe(selected,now,GetTickCount64());
            Sleep(100);
        }
        if(!rearmed) return {Completion::rebuilt,total,false,"Worker ended after completed native replacement"};
    }
    return {Completion::held,total,false,"Maximum native match count reached"};
}
} // namespace outfit_match_v2
