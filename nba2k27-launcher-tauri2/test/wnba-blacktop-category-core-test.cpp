#include "../native/wnba_blacktop_category_core.hpp"
#include <algorithm>
#include <cassert>
#include <iostream>
#include <stdexcept>
#include <string>
using namespace wnba_blacktop_category;
constexpr std::uintptr_t BASE=0x140000000;
Binding scope(){return {123,456,std::string(48,'a')};}
Input ready(){
    Input in;in.game=scope();in.process=Process::live;
    in.owned_offline=in.source_trial_verified=in.explicit_category_consent=in.stable=in.deep_mixed=true;
    in.current=in.alternate=0x92DF06DE;in.stage=3;in.roster_generation=7;
    for(std::uint16_t id=300;id<312;++id)in.clubs.push_back(id);
    for(auto id:{315,316,317})in.clubs.push_back(std::uint16_t(id));return in;
}
struct Host {
    Route route{};std::uintptr_t label_rva=kLabel;std::uintptr_t address=BASE+0x40000000;
    bool departed{},pub=true,rest=true,rx{},retained{},allocation_intent{},ready_intent{};
    bool seal_ok=true,write_ok=true,throw_write{},foreign_after_label{};
    unsigned allocations{},journals{},fail_journal{},writes{},write_fail_at{};
    unsigned bridge_fault{},initializations{},seals{},rx_checks{},ready_calls{};
    std::string host_detail="Injected active checkpoint uncertainty";
    std::uint64_t now=100,clock_delay{};bool clock_throw{},clock_rollback{},passive_ok=true,witness_ok=true;
    bool init_attempted{},context_wait{},baseline_wait_once{},initialize_wait_once{};
    unsigned initialize_intent_calls{};
    bool bound{},bind_ok=true;std::uint64_t bound_start{};unsigned binds{};
    bool bind_preparation_start(std::uint64_t start){++binds;if(!bind_ok)return false;if(bound)return start==bound_start;bound=true;bound_start=start;return true;}
    unsigned defer_proofs{},proofs{},false_proofs{},preallocation_proofs{},passive_proofs{},clock_reads{};
    unsigned delay_stage{};
    std::uint64_t monotonic_ms(){++clock_reads;if(clock_throw)throw std::runtime_error("clock");return clock_rollback?0:now;}
    bool passive_preparation_scope(){++passive_proofs;return passive_ok&&witness_ok&&!init_attempted;}
    PreinitializeProof preallocation_proof(){++preallocation_proofs;return context_wait&&witness_ok?PreinitializeProof::defer:PreinitializeProof::failed;}
    PreinitializeProof preinitialize_proof(bool passed){++proofs;if(delay_stage==3)now+=10000;
        if(!passed)++false_proofs;
        if(init_attempted||!witness_ok)return PreinitializeProof::failed;
        if(!passed)return context_wait?PreinitializeProof::defer:PreinitializeProof::failed;
        if(defer_proofs){--defer_proofs;return PreinitializeProof::defer;}
        return PreinitializeProof::exact;}
    Code code{};std::vector<std::string> events;
    bool checkpoint(const Binding& b,bool publishing){assert(b==scope());context_wait=false;
        if(publishing&&baseline_wait_once){baseline_wait_once=false;context_wait=true;return false;}
        return publishing?pub:rest;}
    bool selection_departed()const{return departed;}
    const char* reason()const{return host_detail.c_str();}
    bool budget=true;
    bool publication_budget(bool)const{return budget;}
    bool read_route(Route& out){out=route;return true;}
    bool journal_bridge_intent(const Binding& b){assert(b==scope());allocation_intent=true;events.push_back("bridge-intent");if(delay_stage==1)now+=10000;return true;}
    std::uintptr_t allocate_bridge_near(std::uintptr_t base,std::uintptr_t table,std::size_t size){
        assert(allocation_intent&&base==BASE&&table==kTable&&size==10);++allocations;events.push_back("allocate");if(delay_stage==2)now+=10000;return bridge_fault==1?0:address;
    }
    void retain_bridge(std::uintptr_t p){assert(p==address);retained=true;events.push_back("retain");}
    bool initialize_bridge_rw(std::uintptr_t p,const Code& c){++initializations;assert(retained&&p==address&&!rx);code=c;events.push_back("initialize");
        if(initialize_wait_once){initialize_wait_once=false;context_wait=true;return false;}
        init_attempted=true;++initialize_intent_calls;if(delay_stage==4)now+=10000;return bridge_fault!=3;}
    bool seal_bridge_rx(std::uintptr_t p,const Code& c){++seals;assert(p==address&&c==code);rx=seal_ok&&bridge_fault!=4;events.push_back("seal");if(delay_stage==5)now+=10000;return rx;}
    bool bridge_is_private_rx(std::uintptr_t p,const Code& c){++rx_checks;if(delay_stage==6)now+=10000;return bridge_fault!=5&&p==address&&retained&&rx&&c==code;}
    bool journal_bridge_ready(const Binding& b,std::uintptr_t p,const Code& c){++ready_calls;assert(b==scope()&&bridge_is_private_rx(p,c));ready_intent=true;events.push_back("bridge-ready");if(delay_stage==7)now+=10000;return bridge_fault!=6;}
    bool journal_change(const Binding& b,const Change& c){
        assert(b==scope()&&ready_intent);++journals;events.push_back(c.word==Word::label?"label-intent":"dispatch-intent");return journals!=fail_journal;
    }
    bool write_change(const Change& c){
        assert(ready_intent&&retained&&rx);++writes;
        if(throw_write)throw std::runtime_error("uncertain write");
        if(c.word==Word::label){
            assert(c.at==BASE+label_rva+8&&route.label==c.before);
            route.label=c.after;events.push_back(c.after==kWomen?"label-publish":"label-restore");
            if(foreign_after_label&&c.after==kWomen)route.dispatch=12345;
        }else{
            assert(c.at==BASE+kEntry&&std::uint32_t(route.dispatch)==c.before);
            route.dispatch=std::int32_t(c.after);events.push_back(c.after==std::uint32_t(kStockDispatch)?"dispatch-restore":"dispatch-publish");
        }
        return write_ok&&writes!=write_fail_at;
    }
    bool journal_applied(const Binding& b,const Change&){assert(b==scope());events.push_back("applied");return true;}
};
int main(){
    {
        Host h;h.defer_proofs=1;Session s(scope(),BASE,h.label_rva);auto in=ready();
        assert(s.tick(in,h)==Result::waiting&&s.phase()==Phase::preparing&&s.preparation_attempts()==1);
        assert(h.allocations==1&&h.initializations==0&&h.writes==0&&h.retained);
        assert(s.tick(in,h)==Result::published&&h.allocations==1&&h.initialize_intent_calls==1);
        assert(h.binds==1&&h.bound_start==100);
        assert(std::count(h.events.begin(),h.events.end(),"bridge-intent")==1);
    }
    {
        Host h;h.initialize_wait_once=true;Session s(scope(),BASE,h.label_rva);auto in=ready();
        assert(s.tick(in,h)==Result::waiting&&h.initializations==1&&!h.init_attempted&&h.initialize_intent_calls==0);
        assert(s.tick(in,h)==Result::published&&h.initializations==2&&h.initialize_intent_calls==1&&h.allocations==1);
    }
    {
        Host h;h.baseline_wait_once=true;Session s(scope(),BASE,h.label_rva);auto in=ready();
        assert(s.tick(in,h)==Result::waiting&&s.phase()==Phase::preparing&&s.bridge()==0);
        assert(h.preallocation_proofs==1&&!h.allocation_intent&&h.allocations==0);
        assert(s.tick(in,h)==Result::published&&h.allocations==1&&h.initialize_intent_calls==1);
    }
    {
        Host h;h.defer_proofs=1;Session s(scope(),BASE,h.label_rva);auto in=ready();
        assert(s.tick(in,h)==Result::waiting);h.baseline_wait_once=true;
        assert(s.tick(in,h)==Result::waiting&&h.false_proofs==1&&h.allocations==1&&h.initializations==0);
        assert(s.tick(in,h)==Result::published&&h.allocations==1&&h.initialize_intent_calls==1);
    }
    {
        Host h;h.defer_proofs=100;Session s(scope(),BASE,h.label_rva);auto in=ready();
        for(unsigned n=0;n<12;++n)assert(s.tick(in,h)==Result::waiting&&s.preparation_attempts()==n+1);
        assert(s.tick(in,h)==Result::held&&h.allocations==1&&h.initializations==0);
        const auto events=h.events;h.defer_proofs=0;assert(s.tick(in,h)==Result::held&&h.events==events);
    }
    for(unsigned elapsed:{9999u,10000u,10001u}) {
        Host h;h.defer_proofs=1;Session s(scope(),BASE,h.label_rva);auto in=ready();
        assert(s.tick(in,h)==Result::waiting);h.now+=elapsed;in.stable=false;in.deep_mixed=false;in.roster_generation=0;
        assert(s.tick(in,h)==(elapsed<10000?Result::waiting:Result::held));assert(h.initializations==0);
    }
    {
        Host h;Session s(scope(),BASE,h.label_rva);auto in=ready();
        for(unsigned n=0;n<12;++n){h.baseline_wait_once=true;assert(s.tick(in,h)==Result::waiting&&s.preparation_attempts()==n+1);}
        assert(s.tick(in,h)==Result::held&&h.allocations==0&&!h.allocation_intent&&h.binds==1);
    }
    {
        Host h;h.bind_ok=false;Session s(scope(),BASE,h.label_rva);
        assert(s.tick(ready(),h)==Result::held&&!h.allocation_intent&&h.allocations==0);
    }
    for(unsigned failure=0;failure<8;++failure) {
        Host h;h.defer_proofs=1;Session s(scope(),BASE,h.label_rva);auto in=ready();assert(s.tick(in,h)==Result::waiting);
        if(failure==0)in.owned_offline=false;if(failure==1)in.source_trial_verified=false;
        if(failure==2)in.explicit_category_consent=false;if(failure==3)++in.roster_generation;
        if(failure==4)h.witness_ok=false;if(failure==5)h.clock_rollback=true;
        if(failure==6)h.clock_throw=true;if(failure==7){in.stable=false;h.passive_ok=false;}
        assert(s.tick(in,h)==Result::held&&h.initializations==0&&h.allocations==1);
        const auto events=h.events;h.clock_throw=false;assert(s.tick(ready(),h)==Result::held&&h.events==events);
    }
    for(unsigned stage=1;stage<=7;++stage) {
        Host h;h.delay_stage=stage;Session s(scope(),BASE,h.label_rva);
        assert(s.tick(ready(),h)==Result::held&&h.writes==0);
        if(stage==1)assert(h.allocation_intent&&h.allocations==0);
        if(stage>=2)assert(h.allocations==1&&h.retained);
        const auto events=h.events;h.delay_stage=0;assert(s.tick(ready(),h)==Result::held&&h.events==events);
    }
    {
        Host h;h.defer_proofs=1;Session s(scope(),BASE,h.label_rva);auto in=ready();assert(s.tick(in,h)==Result::waiting);
        in.deep_mixed=false;in.roster_generation=0;
        assert(s.tick(in,h)==Result::held&&h.passive_proofs==0&&h.initializations==0);
    }
    {
        Host h;Session s(scope(),BASE,h.label_rva);auto in=ready();in.roster_generation=0;
        assert(s.tick(in,h)!=Result::published&&h.allocations==0&&h.initialize_intent_calls==0);
    }
    for(unsigned fault=1;fault<=6;++fault) {
        Host h;h.bridge_fault=fault;
        h.host_detail="Blacktop category deep roster capture or repeated departure scope is uncertain; D=0,0,0 F=16,0 R=402,4294967295 N=403,0 NBA=30,0 W=15,0 DIFF=0 CTX=64 S=1,1,0 O=1,1";
        if(fault==2)h.address=BASE+0x90000000;
        Session s(scope(),BASE,h.label_rva);
        assert(s.tick(ready(),h)==Result::held && h.writes==0 && h.journals==0);
        const char* stages[]={"","allocate","plan","initialize","seal","RX","ready"};
        assert(std::string(s.reason()).find(std::string("[")+stages[fault]+"]")!=std::string::npos);
        if(fault!=2)assert(std::string(s.reason()).find(h.host_detail)!=std::string::npos);
        else assert(std::string(s.reason()).find("Core bridge displacement rejected")!=std::string::npos);
        assert(std::string(s.reason()).size()<240);
        assert(h.allocations==1 && h.retained==(fault!=1));
        assert(h.initializations==(fault>=3?1u:0u) && h.seals==(fault>=4?1u:0u));
        assert(h.rx_checks==(fault==6?2u:fault==5?1u:0u) && h.ready_calls==(fault==6?1u:0u));
        std::vector<std::string> expected={"bridge-intent","allocate"};
        if(fault!=1)expected.push_back("retain");if(fault>=3)expected.push_back("initialize");
        if(fault>=4)expected.push_back("seal");if(fault==6)expected.push_back("bridge-ready");
        assert(h.events==expected);
        const auto events=h.events;const auto reason=std::string(s.reason());h.host_detail="Changed after failure";
        assert(s.tick(ready(),h)==Result::held && h.events==events && std::string(s.reason())==reason);
    }
    {
        Host h;h.bridge_fault=1;h.host_detail=std::string(4096,'X');Session s(scope(),BASE,h.label_rva);
        assert(s.tick(ready(),h)==Result::held && std::string(s.reason()).size()==511);
    }
    {
        std::array<std::uint8_t,72> permit{};
        constexpr std::array<std::uint8_t,8> magic={'C','2','7','W','N','B','C','2'};
        std::copy(magic.begin(),magic.end(),permit.begin());
        const auto binding=scope();
        for(unsigned n=0;n<4;++n)permit[8+n]=std::uint8_t(binding.pid>>(8*n));
        for(unsigned n=0;n<8;++n)permit[16+n]=std::uint8_t(binding.birth>>(8*n));
        std::copy(binding.nonce.begin(),binding.nonce.end(),permit.begin()+24);
        assert(category_approval_matches(permit,binding));
        for(unsigned n=0;n<72;++n) {auto changed=permit;changed[n]^=1;assert(!category_approval_matches(changed,binding));}
        auto legacy=permit;legacy[7]='1';assert(!category_approval_matches(legacy,binding));
        assert(!category_approval_matches(std::span(permit).first(71),binding));
    }

    {
        Host h;Session s(scope(),BASE,h.label_rva);const auto in=ready();
        assert(s.tick(in,h)==Result::published);
        // The UI sample was selection; the fresh executor checkpoint has now
        // positively observed stable offline departure. No new publication.
        h.pub=false;h.departed=true;
        assert(s.tick(in,h)==Result::restored&&s.phase()==Phase::prepared&&h.route==Route{}&&h.writes==4);
        h.pub=true;h.departed=false;
        assert(s.tick(in,h)==Result::published&&h.allocations==1&&h.writes==6);
    }
    for(unsigned failure=0;failure<3;++failure) {
        Host h;Session s(scope(),BASE,h.label_rva);const auto in=ready();assert(s.tick(in,h)==Result::published);
        h.pub=false;h.departed=failure!=0;
        if(failure==1)h.route.dispatch=123;
        if(failure==2)h.rx=false;
        assert(s.tick(in,h)==Result::held&&h.writes==2&&h.retained);
        h.pub=true;h.departed=false;assert(s.tick(in,h)==Result::held&&h.writes==2);
    }
    {
        Host h;h.budget=false;Session s(scope(),BASE,h.label_rva);
        assert(s.tick(ready(),h)==Result::limit_reached && h.writes==0 && h.allocations==0);
        h.budget=true;assert(s.tick(ready(),h)==Result::published);
        h.budget=false;assert(s.tick(ready(),h)==Result::observed);
        auto exit=ready();exit.current=exit.alternate=0x7C602DF8;exit.stage=1;
        assert(s.tick(exit,h)==Result::restored && h.route==Route{});
        assert(s.tick(ready(),h)==Result::limit_reached && h.allocations==1 && h.writes==4);
    }
    static_assert(kEntry==0x7425F58&&kStockDispatch==std::int32_t(0xFAEE5F58u));
    {
        auto in=ready();assert(ready_selection(in));
        for(unsigned failure=0;failure<8;++failure){auto bad=in;
            if(failure==0)bad.stage=2;if(failure==1)bad.active=1;if(failure==2)bad.kind=1;
            if(failure==3)bad.alternate=0;if(failure==4)bad.deep_mixed=false;
            if(failure==5)bad.clubs.pop_back();if(failure==6)bad.clubs[1]=bad.clubs[0];
            if(failure==7)bad.roster_generation=0;assert(!ready_selection(bad));
            Host h;Session s(scope(),BASE,h.label_rva);assert(s.tick(bad,h)==Result::waiting&&h.writes==0&&h.allocations==0);
        }
        Host h;Session s(scope(),BASE,h.label_rva);in.explicit_category_consent=false;
        assert(s.tick(in,h)==Result::waiting&&h.allocations==0);
        in=ready();in.source_trial_verified=false;
        assert(s.tick(in,h)==Result::waiting&&h.allocations==0&&h.writes==0);
        in=ready();in.game.nonce[0]='b';assert(s.tick(in,h)==Result::held&&h.writes==0);
    }
    {
        Host h;Session s(scope(),BASE,h.label_rva);auto in=ready();
        assert(s.tick(in,h)==Result::published&&s.phase()==Phase::active&&h.writes==2);
        assert(h.route.label==kWomen&&h.retained&&h.rx&&h.code[0]==0xBE&&h.code[4]==0x20&&h.code[5]==0xE9);
        const auto label=std::find(h.events.begin(),h.events.end(),"label-publish");
        const auto dispatch=std::find(h.events.begin(),h.events.end(),"dispatch-publish");assert(label<dispatch);
        assert(s.tick(in,h)==Result::observed&&h.writes==2);
        in.current=in.alternate=0x6509837B;in.stage=0;
        assert(s.tick(in,h)==Result::restored&&h.route==Route{}&&h.writes==4&&h.retained);
        const auto dr=std::find(h.events.begin(),h.events.end(),"dispatch-restore");
        const auto lr=std::find(h.events.begin(),h.events.end(),"label-restore");assert(dr<lr);
        in=ready();in.roster_generation=8;
        assert(s.tick(in,h)==Result::published&&h.allocations==1&&h.retained);
    }
    {
        Host h;Session s(scope(),BASE,h.label_rva);h.route.label=kWomen;
        assert(s.tick(ready(),h)==Result::held&&h.writes==0&&h.allocations==0);
    }
    {
        Host h;Session s(scope(),BASE,h.label_rva);h.fail_journal=2;
        assert(s.tick(ready(),h)==Result::held&&h.route==Route{}&&h.retained&&h.writes==2);
        const auto writes=h.writes;h.fail_journal=0;
        assert(s.tick(ready(),h)==Result::held&&h.writes==writes);
    }
    {
        Host h;Session s(scope(),BASE,h.label_rva);h.write_fail_at=2;
        assert(s.tick(ready(),h)==Result::held&&h.route==Route{}&&h.writes==4&&h.retained);
    }
    {
        Host h;Session s(scope(),BASE,h.label_rva);h.foreign_after_label=true;
        assert(s.tick(ready(),h)==Result::held&&s.uncertain()&&h.route.dispatch==12345&&h.writes==1);
        assert(h.retained);assert(s.tick(ready(),h)==Result::held&&h.writes==1);
    }
    {
        Host h;Session s(scope(),BASE,h.label_rva);auto in=ready();assert(s.tick(in,h)==Result::published);
        h.route.dispatch=12345;in.stable=false;
        assert(s.tick(in,h)==Result::held&&h.writes==2&&h.route.dispatch==12345&&h.retained);
    }
    {
        Host h;Session s(scope(),BASE,h.label_rva);auto in=ready();assert(s.tick(in,h)==Result::published);
        in.owned_offline=false;
        assert(s.tick(in,h)==Result::held&&h.route==Route{}&&h.retained);
    }
    {
        Host h;Session s(scope(),BASE,h.label_rva);auto in=ready();assert(s.tick(in,h)==Result::published);
        h.rest=false;in.stable=false;
        assert(s.tick(in,h)==Result::held&&h.writes==2&&s.owned_route()&&h.retained);
        h.rest=true;assert(s.tick(in,h)==Result::held&&h.writes==2);
        in.process=Process::exited;assert(s.tick(in,h)==Result::ended&&h.writes==2);
    }
    {
        Host h;Session s(scope(),BASE,h.label_rva);auto in=ready();assert(s.tick(in,h)==Result::published);
        in.process=Process::unknown;
        assert(s.tick(in,h)==Result::held&&h.writes==2&&s.owned_route()&&h.retained);
        in.process=Process::live;assert(s.tick(in,h)==Result::held&&h.writes==2);
    }
    {
        Host h;Session s(scope(),BASE,h.label_rva);h.seal_ok=false;
        assert(s.tick(ready(),h)==Result::held&&h.writes==0&&h.retained);
    }
    {
        Host h;Session s(scope(),BASE,h.label_rva);h.address=BASE+0x90000000;
        assert(s.tick(ready(),h)==Result::held&&h.writes==0&&h.retained);
    }
    {
        Host h;Session s(scope(),BASE,h.label_rva);h.throw_write=true;
        assert(s.tick(ready(),h)==Result::held&&s.uncertain()&&h.retained);
        h.throw_write=false;assert(s.tick(ready(),h)==Result::held&&h.writes==1);
    }
    {
        Host h;h.label_rva=0xACFA610+16*20;Session s(scope(),BASE,h.label_rva);
        auto in=ready();assert(s.tick(in,h)==Result::published);
        in.stable=false;assert(s.tick(in,h)==Result::restored&&h.route==Route{});
    }
    for(auto bad:{std::uintptr_t(0),std::uintptr_t(0xACFA611),std::uintptr_t(0xACFCE80)}) {
        Host h;Session s(scope(),BASE,bad);
        assert(s.tick(ready(),h)==Result::held&&h.writes==0&&h.allocations==0);
    }
    std::cout<<"Blacktop category offline admission/publication/restore/retention tests passed\n";
}
