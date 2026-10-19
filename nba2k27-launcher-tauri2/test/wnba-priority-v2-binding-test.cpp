#include "../native/wnba_priority_v2_proxy_bridge.hpp"
#include "../native/wnba_blacktop_category_win.hpp"
#include <cassert>
#include <iostream>
int main() {
    {
        using namespace wnba_blacktop_category::win_detail;
        constexpr std::uintptr_t base=0x140000000;
        {
            DeepDiagnostic diagnostic{};Roster out{};bool departed=true;std::string order;
            auto capture=[&](Roster& r){order+='C';r.count=403;return true;};
            auto scope=[&]{order+='S';return false;};
            auto observe=[&](outfit_match_v2::Context&){order+='O';return false;};
            assert(capture_or_departure(base,out,departed,capture,scope,observe,&diagnostic));
            assert(order=="CC" && !departed && diagnostic.first_ok && diagnostic.second_ok);
            assert(!diagnostic.scope_attempted && !diagnostic.departure_first && diagnostic.difference==0);
        }
        for(unsigned fault=0;fault<6;++fault) {
            DeepDiagnostic diagnostic{};Roster out{};bool departed=true;std::string order;unsigned captures{};
            auto capture=[&](Roster& r) {order+='C';++captures;r.count=403;
                if(fault==0) {diagnostic.captures[0].failure=CaptureFailure::row_name;diagnostic.captures[0].row=17;return false;}
                if(fault==1 && captures==2)return false;
                if(fault>=2 && captures==2)r.context.game_mode=6;
                return true;};
            auto scope=[&] {order+='S';return fault!=3;};
            unsigned observations{};
            auto observe=[&](outfit_match_v2::Context& c) {order+='O';++observations;
                c.root_tag=base+outfit_match_v2::kRootTag;c.mode=c.other=0x6509837B;c.stage=1;
                return fault!=4 || observations!=1;};
            assert(!capture_or_departure(base,out,departed,capture,scope,observe,&diagnostic));
            assert(order==(fault==0?"CSOO":fault==3?"CCS":fault==4?"CCSO":"CCSOO"));
            assert(diagnostic.first_ok==(fault!=0) && diagnostic.second_attempted==(fault!=0));
            assert(diagnostic.second_ok==(fault>=2));
            assert(diagnostic.difference==(fault>=2?1u:0u));
            assert(diagnostic.context_difference==(fault>=2?64u:0u));
            assert(diagnostic.scope_attempted && diagnostic.scope_ok==(fault!=3));
            assert(diagnostic.departure_first==(fault!=3 && fault!=4));
            assert(diagnostic.departure_second==(fault!=3 && fault!=4));
            assert(departed==(fault!=3 && fault!=4));
            if(fault==0)assert(diagnostic.captures[0].row==17 && diagnostic.captures[1].failure==CaptureFailure::none);
        }
        for(unsigned bit=0;bit<10;++bit) {
            outfit_match_v2::Context a{},b{};
            if(bit==0)++b.index;if(bit==1)++b.alternate;if(bit==2)++b.mode;if(bit==3)++b.other;
            if(bit==4)++b.stage;if(bit==5)++b.kind;if(bit==6)++b.game_mode;if(bit==7)++b.active;
            if(bit==8)++b.owner;if(bit==9)++b.root_tag;
            assert(context_difference(a,b)==(1u<<bit));
        }
        Roster a{};a.rows.push_back({300,29,{L'A'}});
        for(unsigned bit=0;bit<10;++bit) {
            auto b=a;
            if(bit==0)++b.context.game_mode;if(bit==1)++b.root;if(bit==2)++b.owner;if(bit==3)++b.table;
            if(bit==4)++b.allocation;if(bit==5)++b.count;if(bit==6)b.rows.push_back({});
            if(bit==7)++b.rows[0].id;if(bit==8)++b.rows[0].kind;if(bit==9)++b.rows[0].name[23];
            assert(roster_difference(a,b)==(1u<<bit));
        }
        DeepDiagnostic diagnostic{};diagnostic.difference=diagnostic.context_difference=1023;
        for(auto& c:diagnostic.captures) {c.failure=CaptureFailure::ending_table_range;c.row=c.count=~0u;c.nba=c.clubs=403;}
        std::array<char,384> text{};format_deep_reason(text,diagnostic,ScopeFailure::route);
        assert(std::string(text.data()).starts_with("Blacktop category deep roster capture or repeated departure scope is uncertain;"));
        assert(std::strlen(text.data())<240 && std::string(text.data()).find("DIFF=1023 CTX=1023")!=std::string::npos);
        diagnostic={};diagnostic.captures[0].failure=CaptureFailure::ending_context_changed;
        diagnostic.captures[0].context_difference=64;
        format_deep_reason(text,diagnostic,ScopeFailure::none);
        assert(std::string(text.data()).find("CTX=64")!=std::string::npos);
    }
    {
        using namespace wnba_blacktop_category::win_detail;
        Roster a{};a.root=1;a.owner=2;a.table=3;a.allocation=4;a.count=403;
        a.rows.push_back({300,29,{L'A'}});
        auto changed=a;changed.context.index=7;changed.context.alternate=7;
        changed.context.game_mode=6;
        assert(a!=changed && same_roster(a,changed));
        for(unsigned field=0;field<8;++field) {
            changed=a;
            if(field==0)++changed.root;if(field==1)++changed.owner;
            if(field==2)++changed.table;if(field==3)++changed.allocation;
            if(field==4)++changed.count;if(field==5)++changed.rows[0].id;
            if(field==6)++changed.rows[0].kind;if(field==7)++changed.rows[0].name[0];
            assert(!same_roster(a,changed));
        }
    }
    {
        using namespace wnba_blacktop_category::win_detail;
        constexpr std::uintptr_t base=0x140000000;
        outfit_match_v2::Context c{};c.root_tag=base+outfit_match_v2::kRootTag;
        c.mode=c.other=0x6509837B;c.stage=1;
        assert(stable_offline_departure(base,c,c));
        auto torn=c;++torn.index;assert(!stable_offline_departure(base,c,torn));
        auto mismatched=c;mismatched.other^=1;
        assert(!stable_offline_departure(base,mismatched,mismatched));
        for(unsigned field=0;field<4;++field) {
            auto bad=c;
            if(field==0)bad.active=1;if(field==1)bad.kind=1;
            if(field==2)++bad.root_tag;
            if(field==3){bad.mode=bad.other=0x92DF06DE;bad.stage=3;}
            assert(!stable_offline_departure(base,bad,bad));
        }
    }
    {
        using namespace wnba_blacktop_category::win_detail;
        constexpr std::uintptr_t base=0x140000000;
        outfit_match_v2::Context selected{};selected.root_tag=base+outfit_match_v2::kRootTag;
        selected.mode=selected.other=0x92DF06DE;selected.stage=3;
        auto exited=selected;exited.mode=exited.other=0x6509837B;exited.stage=1;
        unsigned calls{},cas{};
        auto observe=[&](outfit_match_v2::Context& out){++calls;out=selected;return true;};
        if(forward_selection(base,[]{return false;},observe))++cas;
        assert(cas==0 && calls==0);
        assert(forward_selection(base,[]{return true;},observe) && calls==2);
        for(unsigned fault=0;fault<7;++fault) {
            calls=cas=0;auto changed=selected;
            if(fault==0)changed.active=1;if(fault==1)changed.kind=1;
            if(fault==2)changed.other^=1;if(fault==3)++changed.root_tag;
            if(fault==4)changed=exited;
            auto late=[&](outfit_match_v2::Context& out){++calls;out=changed;if(fault==5 && calls==2)++out.index;return fault!=6;};
            if(forward_selection(base,[]{return true;},late))++cas;
            assert(cas==0);
        }
        for(unsigned failed_capture=0;failed_capture<3;++failed_capture) {
            unsigned captured{},proofs{},observed{};bool departed=true;Roster out{};
            auto capture=[&](Roster& value){++captured;value.count=403;if(failed_capture==2)value.count+=captured;return captured!=failed_capture+1;};
            auto scope=[&]{++proofs;return true;};
            auto context=[&](outfit_match_v2::Context& value){++observed;value=exited;return true;};
            assert(!capture_or_departure(base,out,departed,capture,scope,context));
            assert(departed && proofs==1 && observed==2);
        }
        for(unsigned failure=0;failure<7;++failure) {
            bool departed=true;Roster out{};unsigned observed{};
            auto failed=[](Roster&){return false;};
            auto scope=[&]{return failure!=0;};
            auto context=[&](outfit_match_v2::Context& value){++observed;value=exited;
                if(failure==1)value.active=1;if(failure==2)value.kind=1;
                if(failure==3)value.other^=1;if(failure==4)++value.root_tag;
                if(failure==5 && observed==2)++value.index;return failure!=6;};
            assert(!capture_or_departure(base,out,departed,failed,scope,context) && !departed);
            if(failure==0)assert(observed==0);
        }
        bool departed=true;Roster out{};unsigned proofs{};
        auto good=[](Roster& value){value.count=403;return true;};
        assert(capture_or_departure(base,out,departed,good,[&]{++proofs;return false;},observe));
        assert(!departed && proofs==0 && out.count==403);
    }
    // No actual game or native API invocation. An unopened authority and the
    // default-false live approval gate must fail before any target resolution.
    outfit_match_v2::Authority authority;
    wnba_priority_v2::NativeHost host;
    assert(!host.open(authority,L"C:\\invalid"));
    assert(!host.open(authority,L"C:\\invalid",true));
    assert(!host.open(authority,L"C:\\invalid",false,true));
    assert(!host.open(authority,L"C:\\invalid",true,true));
    assert(!host.lock());
    assert(!host.checkpoint(42));
    assert(host.priority()==0);
    assert(!host.native_register());
    assert(!host.is_native_wad(0x1000));
    host.unlock();
    assert(!wnba_priority_v2::bridge::start(nullptr,nullptr));
    std::cout << "WNBA native binding no-call guards passed\n";
}
