#include "../native/wnba_priority_v2_core.hpp"
#include <cassert>
#include <iostream>
using namespace wnba_priority_v2;
struct Host {
    bool locked{}, checkpoint_ok{true}, call_ok{true}, native{true};
    std::uintptr_t current{}, race_value{};
    int calls{}, locks{}, unlocks{};
    bool lock() { ++locks; locked=true; current=race_value ? race_value : current; return true; }
    void unlock() { assert(locked); locked=false; ++unlocks; }
    bool checkpoint(std::uint64_t id) { assert(locked && id==42); return checkpoint_ok; }
    std::uintptr_t priority() { assert(locked); return current; }
    bool native_register() { assert(locked && !current); ++calls; if(call_ok) current=0x1000; return call_ok; }
    bool is_native_wad(std::uintptr_t p) { assert(locked && p==current); return native; }
};
Admission admitted() { return {true,true,true,true,true,true,42}; }
int main() {
    {
        const auto& normal=pack_profile(false);
        const auto& trial=pack_profile(true);
        assert(std::wstring(normal.folder)==L"qa058-wpf-era" && normal.index_bytes==120 && normal.entries==1);
        assert(std::wstring(trial.folder)==L"qa064-blacktop-source" && trial.index_bytes==144 && trial.entries==2);
        assert(std::string(normal.index_sha256)=="6630BEDCBF708145FDFB55A48DE467265F3DE29CD76081E9C24A1C8EFD7B84BD");
        assert(std::string(trial.index_sha256)=="0C25D0839B11B4AF085F587C1BAF4E74F821CD34D2B902FC950DBC91E5123CC9");
    }
    {
        constexpr std::uint32_t home=0x6509837B,quick=0x079964C0;
        assert(inactive_frontend({home,home,0,0,0}));
        assert(inactive_frontend({home,home,1,0,0}));
        assert(inactive_frontend({quick,quick,1,0,0}));
        assert(!inactive_frontend({home,home,2,0,0}));
        assert(!inactive_frontend({quick,quick,0,0,0}));
        assert(!inactive_frontend({home,quick,1,0,0}));
        assert(!inactive_frontend({home,home,0,1,0}));
        assert(!inactive_frontend({home,home,0,0,1}));
        assert(!inactive_frontend({0x92DF06DE,0x92DF06DE,3,0,0}));
        assert(!inactive_frontend({0xF54D5D12,0xF54D5D12,1,0,0}));
    }
    {
        const std::string nonce(48,'a');
        std::vector<unsigned char> token(72);
        std::memcpy(token.data(),"C27WNPA1",8);
        const std::uint32_t pid=123;const std::uint64_t birth=456;
        std::memcpy(token.data()+8,&pid,4);std::memcpy(token.data()+16,&birth,8);
        std::memcpy(token.data()+24,nonce.data(),48);
        assert(approval_matches(token,pid,birth,nonce));
        assert(!blacktop_approval_matches(token,pid,birth,nonce));
        assert(!approval_matches({},pid,birth,nonce));
        assert(!approval_matches(token,pid+1,birth,nonce));
        assert(!approval_matches(token,pid,birth+1,nonce));
        assert(!approval_matches(token,pid,birth,std::string(48,'b')));
        for(std::size_t n=0;n<token.size();++n) {
            auto changed=token;changed[n]^=1;
            assert(!approval_matches(changed,pid,birth,nonce));
        }
        std::memcpy(token.data(),"C27WNBT1",8);
        assert(blacktop_approval_matches(token,pid,birth,nonce));
        assert(!approval_matches(token,pid,birth,nonce));
        assert(!blacktop_approval_matches(token,pid+1,birth,nonce));
        assert(!blacktop_approval_matches(token,pid,birth+1,nonce));
        assert(!blacktop_approval_matches(token,pid,birth,std::string(48,'b')));
        for(std::size_t n=0;n<token.size();++n) {
            auto changed=token;changed[n]^=1;
            assert(!blacktop_approval_matches(changed,pid,birth,nonce));
        }
        std::memcpy(token.data(),"C27WNPC1",8);
        assert(!approval_matches(token,pid,birth,nonce));
        assert(!blacktop_approval_matches(token,pid,birth,nonce));
    }
    {
        Session s; Host h; auto a=admitted();
        assert(s.publish(a,h)==Result::published);
        assert(s.publish(a,h)==Result::already_published && h.calls==1);
        assert(h.locks==h.unlocks && !h.locked);
        // Launcher shutdown has no unregister action; native pointer remains.
        s.game_ended(41); assert(s.phase()==Phase::native_owned);
        s.game_ended(42); assert(s.publish(a,h)==Result::ended && h.calls==1);
    }
    {
        Session s; Host h; h.race_value=0xDEAD;
        // A backend arriving after the caller's snapshot is still protected.
        assert(s.publish(admitted(),h)==Result::conflict && h.calls==0);
        assert(h.current==0xDEAD && h.unlocks==1);
    }
    {
        Session s; Host h; h.checkpoint_ok=false;
        assert(s.publish(admitted(),h)==Result::waiting && h.calls==0 && h.unlocks==1);
        h.checkpoint_ok=true; assert(s.publish(admitted(),h)==Result::published);
        h.current=0xDEAD;
        assert(s.publish(admitted(),h)==Result::conflict && h.calls==1);
    }
    {
        Session s; Host h; auto a=admitted(); a.inactive_frontend=false;
        assert(s.publish(a,h)==Result::waiting && h.calls==0 && h.locks==0);
        a.inactive_frontend=true; a.owned_offline_session=false;
        assert(s.publish(a,h)==Result::invalid && h.locks==0);
    }
    {
        Session s; Host h; h.call_ok=false;
        assert(s.publish(admitted(),h)==Result::failed && h.unlocks==1);
        h.call_ok=true;
        assert(s.publish(admitted(),h)==Result::conflict && h.calls==1);
    }
    {
        Session s; Host h; assert(s.publish(admitted(),h)==Result::published);
        auto a=admitted(); a.identity=99;
        assert(s.publish(a,h)==Result::conflict && h.calls==1);
    }
    std::cout << "WNBA priority admission/lifetime tests passed\n";
}
