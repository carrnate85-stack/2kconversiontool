#define OUTFIT_MATCH_V2_OFFLINE_TEST
#include "../native/outfit_match_v2_native.hpp"
#include "../native/outfit_match_v2_proxy_bridge.hpp"
#include <cassert>
#include <iostream>
#include <stdexcept>
#include <sddl.h>
using namespace outfit_match_v2;
namespace {
Choice choice(std::uintptr_t p=0x10000) { return {p,{L"top_one",L"bottom_one",L"acc_sock_one"}}; }
struct Fake final : Operations {
    unsigned calls{},fail{},clones{},packets{},frees{},deletes{},inserts{},post_transfer_reads{};
    bool authority{true},bound{true},alias{false},queue_busy{false},witness{true},observed{true},throw_insert{false};
    bool transferred{false},durable_transfer{false},durable_clone{false},durable_packet{false};
    std::optional<Phase> cancel_phase;
    std::vector<Phase> events;
    bool step(){return ++calls!=fail;}
    bool authorized() override{return step()&&authority;}
    bool all_bindings(const std::vector<Choice>& c) override{return step()&&bound&&valid_choices(c);}
    bool bind(const Choice&) override{return step()&&bound;}
    bool durable(Phase p,const Resources& r,const Choice&) override {
        if(!step()) return false;
        events.push_back(p);
        if(p==Phase::clone_intent)durable_clone=true;
        if(p==Phase::packet_intent)durable_packet=true;
        if(p==Phase::transfer_intent)durable_transfer=true;
        if(transferred && (p==Phase::dispose_clone||p==Phase::dispose_packet))++post_transfer_reads;
        if(p==Phase::transferred)assert(r.transfer_attempted);
        if(cancel_phase&&*cancel_phase==p)authority=false;
        return true;
    }
    std::uintptr_t clone() override {assert(durable_clone);if(!step())return 0;++clones;return 0x20000;}
    bool prove_clone(std::uintptr_t) override{return step()&&!alias;}
    bool expand(std::uintptr_t,const Choice&) override{return step();}
    std::uintptr_t allocate_packet() override{assert(durable_packet);if(!step())return 0;++packets;return 0x30000;}
    bool prepare_packet(std::uintptr_t,std::uintptr_t) override{return step();}
    Transfer transfer(Resources& r) override {
        if(!step()||queue_busy)return Transfer::not_entered;
        assert(durable_transfer);r.transfer_attempted=true;transferred=true;++inserts;
        if(throw_insert)throw std::runtime_error("uncertain native insert");
        return witness?Transfer::witnessed:Transfer::uncertain;
    }
    bool observe(const Choice&) override {assert(transferred);return step()&&observed&&authority;}
    bool dispose_packet(std::uintptr_t) override {assert(!transferred);++frees;return step();}
    bool dispose_clone(std::uintptr_t) override {assert(!transferred&&!alias);++deletes;return step();}
};
void transaction_tests() {
    Fake good;auto ok=execute(good,ConsumedPlan::fixture({choice()}));
    assert(ok.state==Completion::rebuilt&&ok.rebuilt==1&&!ok.gameplay_verified&&good.clones==1&&good.inserts==1&&good.frees==0&&good.deletes==0);
    unsigned cases{};
    for(unsigned fault=1;fault<=good.calls+3;++fault) {
        Fake f;f.fail=fault;const auto outcome=execute(f,ConsumedPlan::fixture({choice()}));
        assert(f.inserts<=1&&f.clones<=1&&f.packets<=1&&f.post_transfer_reads==0);
        if(f.transferred)assert(!f.frees&&!f.deletes);
        if(outcome.state==Completion::rebuilt)assert(f.inserts==1&&outcome.rebuilt==1);
        ++cases;
    }
    Fake aliases;aliases.alias=true;auto a=execute(aliases,ConsumedPlan::fixture({choice()}));
    assert(a.state==Completion::held&&aliases.clones==1&&aliases.deletes==0&&aliases.inserts==0);
    Fake busy;busy.queue_busy=true;auto b=execute(busy,ConsumedPlan::fixture({choice()}));
    assert(b.state==Completion::failed&&busy.inserts==0&&busy.frees==1&&busy.deletes==1);
    Fake uncertain;uncertain.witness=false;auto c=execute(uncertain,ConsumedPlan::fixture({choice()}));
    assert(c.state==Completion::held&&uncertain.inserts==1&&!uncertain.frees&&!uncertain.deletes);
    Fake cancelled;cancelled.authority=false;auto d=execute(cancelled,ConsumedPlan::fixture({choice()}));
    assert(d.state==Completion::failed&&!cancelled.clones&&!cancelled.inserts);
    Fake missing;missing.bound=false;auto e=execute(missing,ConsumedPlan::fixture({choice()}));
    assert(e.state==Completion::failed&&!missing.clones);
    Fake timeout;timeout.observed=false;auto f=execute(timeout,ConsumedPlan::fixture({choice()}));
    assert(f.state==Completion::held&&timeout.inserts==1&&!timeout.deletes&&!timeout.frees);
    Fake throws;throws.throw_insert=true;auto g=execute(throws,ConsumedPlan::fixture({choice()}));
    assert(g.state==Completion::held&&throws.inserts==1&&!throws.deletes&&!throws.frees);
    for(const auto phase:{Phase::clone_owned,Phase::packet_owned,Phase::transferred}) {
        Fake cancelled_later;cancelled_later.cancel_phase=phase;
        auto held=execute(cancelled_later,ConsumedPlan::fixture({choice()}));
        assert(held.state==Completion::held&&!cancelled_later.frees&&!cancelled_later.deletes);
        if(phase!=Phase::transferred)assert(!cancelled_later.inserts);
    }
    auto token=ConsumedPlan::fixture({choice()});Fake first;
    assert(execute(first,std::move(token)).state==Completion::rebuilt);Fake replay;
    assert(execute(replay,std::move(token)).state==Completion::failed&&!replay.calls);
    Fake duplicate;assert(execute(duplicate,ConsumedPlan::fixture({choice(),choice()})).state==Completion::failed&&!duplicate.calls);
    std::cout<<"PASS exactly-once transaction and "<<cases<<" injected phase failures, aliases, queue, cancellation, replay\n";
}
void alias_tests() {
    const auto actor=reinterpret_cast<std::uintptr_t>(VirtualAlloc(nullptr,0x8000,MEM_COMMIT|MEM_RESERVE,PAGE_READWRITE));
    const auto cloned=reinterpret_cast<std::uintptr_t>(VirtualAlloc(nullptr,0x4000,MEM_COMMIT|MEM_RESERVE,PAGE_READWRITE));
    const auto list=reinterpret_cast<std::uintptr_t>(VirtualAlloc(nullptr,4096,MEM_COMMIT|MEM_RESERVE,PAGE_READWRITE));
    assert(actor&&cloned&&list);
    Description source{},copy{};source.at=actor+0xF30;copy.at=cloned;
    source.array=actor+0x6040;source.owned=actor+0x6080;source.owner=actor+0x6000;
    copy.array=list+0x40;copy.owned=list+0x80;copy.owner=list;
    std::memcpy(source.bytes.data()+0x368,&source.owner,8);std::memcpy(copy.bytes.data()+0x368,&copy.owner,8);
    const auto head=copy.at+0x350;
    std::memcpy(copy.bytes.data()+0x358,&head,8);std::memcpy(copy.bytes.data()+0x360,&head,8);
    assert(independent(source,copy,actor)); // Prevent a vacuous always-false test.
    constexpr unsigned fields[]={0x368,0x370,0x2940,0x2948,0x2950,0x2958};unsigned cases{};
    for(auto x:fields)for(auto y:fields) {
        auto a=source,b=copy;std::uintptr_t shared=0x600000;
        std::memcpy(a.bytes.data()+x,&shared,8);std::memcpy(b.bytes.data()+y,&shared,8);
        assert(!independent(a,b,actor));++cases;
    }
    for(auto x:fields)for(auto y:fields)if(x!=y) {
        auto b=copy;const std::uintptr_t shared=list+0x200;
        std::memcpy(b.bytes.data()+x,&shared,8);std::memcpy(b.bytes.data()+y,&shared,8);
        assert(!independent(source,b,actor));++cases;
    }
    auto in_actor=copy;in_actor.at=actor+0x100;assert(!independent(source,in_actor,actor));
    auto intrusive=copy;intrusive.bytes[0x360]^=1;assert(!independent(source,intrusive,actor));
    auto same_string=copy;source.strings={actor+0x7000};same_string.strings=source.strings;
    assert(!independent(source,same_string,actor));
    assert(VirtualFree(reinterpret_cast<void*>(list),0,MEM_RELEASE));
    assert(VirtualFree(reinterpret_cast<void*>(cloned),0,MEM_RELEASE));
    assert(VirtualFree(reinterpret_cast<void*>(actor),0,MEM_RELEASE));
    std::cout<<"PASS "<<cases<<" destructor ownership cross-alias combinations\n";
}
std::vector<unsigned char> plan_bytes() {
    std::vector<unsigned char> b(24+16+7*512);
    auto u32=[&](unsigned at,unsigned n){std::memcpy(b.data()+at,&n,4);};
    auto u64=[&](unsigned at,std::uint64_t n){std::memcpy(b.data()+at,&n,8);};
    u32(0,0x33504F42);u32(4,123);u32(8,1);u64(16,456);u64(24,0x10000);u32(32,3);
    const auto c=choice();for(unsigned i=0;i<3;++i)std::memcpy(b.data()+40+i*512,c.items[i].c_str(),(c.items[i].size()+1)*2);
    return b;
}
void parser_tests() {
    const auto original=plan_bytes();std::vector<Choice> parsed;
    assert(parse_plan(original,123,456,parsed)&&parsed.size()==1&&parsed[0].items==choice().items);
    assert(!parse_plan(original,124,456,parsed)&&!parse_plan(original,123,457,parsed));
    for(unsigned at:{0u,4u,8u,12u,16u,32u,36u,40u+3*512}) {
        auto b=original;b[at]^=0xFF;assert(!parse_plan(b,123,456,parsed));
    }
    for(unsigned n=0;n<original.size();++n) {auto b=original;b.resize(n);assert(!parse_plan(b,123,456,parsed));}
    auto extra=original;extra.push_back(0);assert(!parse_plan(extra,123,456,parsed));
    auto shirt=choice();shirt.items[0]=L"NO SHIRT";assert(valid_choices({shirt}));
    for(const auto& bad:{L"../shirt",L"x y",L"NO SHIRT ",L"shirt.iff",L"a/b"}) {auto c=choice();c.items[0]=bad;assert(!valid_choices({c}));}
    std::cout<<"PASS bounded BOP3 parser, exact identity, truncation, trailing fields and assets\n";
    StableBoundary<std::array<unsigned,2>,4,500> rearm;
    const std::array<unsigned,2> selection={3,0},changed={3,1};
    assert(!rearm.observe(true,selection,0)&&!rearm.observe(true,selection,499)&&!rearm.observe(true,selection,500));
    assert(!rearm.observe(false,selection,1000)); // Match/menu/quick-play cannot themselves arm.
    assert(!rearm.observe(true,selection,1500)&&!rearm.observe(true,selection,2000));
    assert(!rearm.observe(true,changed,2500)&&!rearm.observe(true,changed,3000)&&!rearm.observe(true,changed,3500));
    assert(rearm.observe(true,changed,4000));
    assert(!rearm.observe(true,selection,4500));
    std::cout<<"PASS fresh-match four-observation rearm, transitions, changed owners and timing\n";
}
void win32_self_tests() {
    const auto p=reinterpret_cast<std::uintptr_t>(VirtualAlloc(nullptr,4096,MEM_COMMIT|MEM_RESERVE,PAGE_READWRITE));
    assert(p&&private_span(p,4096,true));DWORD old{};
    assert(VirtualProtect(reinterpret_cast<void*>(p),4096,PAGE_READONLY,&old));
    assert(private_span(p,4096)&&!private_span(p,4096,true));
    assert(VirtualProtect(reinterpret_cast<void*>(p),4096,PAGE_EXECUTE_READWRITE,&old));
    assert(!private_span(p,4096));assert(VirtualFree(reinterpret_cast<void*>(p),0,MEM_RELEASE));
    assert(!private_span(p,1));
    assert(canonical_path(L"C:\\Fixture\\Data")&&!canonical_path(L"C:\\Fixture\\..\\Data")&&!canonical_path(L"C:\\Fixture\\Data:stream"));
    std::vector<unsigned char> abc={'a','b','c'};
    assert(digest(abc)=="BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD");
    // Birth/path alone do not imply liveness: the actual wait status is tested.
    Handle event(CreateEventW(nullptr,TRUE,FALSE,nullptr));assert(event&&live(event.value));
    assert(SetEvent(event.value)&&!live(event.value));
    std::cout<<"PASS actual Win32 self-owned page bounds, protection, SHA and retained wait status\n";
}
template<class T> void put(std::uintptr_t at,T v) {std::memcpy(reinterpret_cast<void*>(at),&v,sizeof v);}
void native_binding_tests() {
    // A synthetic image reservation and fixture heap in THIS test process.
    // No NBA process is opened and no game-native function is called.
    const auto base=reinterpret_cast<std::uintptr_t>(VirtualAlloc(nullptr,0xB000000,MEM_RESERVE,PAGE_NOACCESS));
    const auto heap=reinterpret_cast<std::uintptr_t>(VirtualAlloc(nullptr,0x20000,MEM_COMMIT|MEM_RESERVE,PAGE_READWRITE));
    assert(base&&heap);
    auto commit=[&](std::uintptr_t rva,std::size_t bytes) {
        const auto start=(base+rva)&~std::uintptr_t(4095);
        assert(VirtualAlloc(reinterpret_cast<void*>(start),base+rva+bytes-start,MEM_COMMIT,PAGE_READWRITE));
    };
    commit(kCurrent,4);commit(kAlternate,4);commit(kModes,64);commit(kOwner,8);commit(kActiveGame,1);
    commit(kRoot,0xEB0);commit(kTable,30*0x50);commit(kMainThread,4);commit(kActiveManagers,0x200);commit(kLineup,0x110);
    const auto actor=heap,primary=heap+0x4000,loaded=heap+0x5000,owner=heap+0x6000;
    const auto state=heap+0x7000,ctx=heap+0x8000,list=heap+0x9000,exposed=heap+0xA000,owned=heap+0xA100;
    const auto string1=heap+0xB000,string2=heap+0xB200,node=heap+0xC000,original=heap+0xD000;
    put(base+kLineup+4,2u);put(base+kLineup+8,2u);put(base+kLineup+0xB8,2u);
    put(base+kLineup+0xC0,original);put(base+kLineup+0xC8,original+0x1000);
    put(base+kCurrent,0u);put(base+kAlternate,0u);put(base+kModes,0x3EEF98B9u);put(base+kOwner,owner);
    put(owner+0xA04,6u);put(base+kRoot+0xEA8,1u);put(base+kMainThread,GetCurrentThreadId()+1);
    put(base+kQueue,base+kQueueTag);put(base+kQueueHead,base+kQueueHeadTag);
    put(base+kQueue+0x5A,static_cast<unsigned char>(1));
    put(base+kQueueHead+8,base+kQueueHead);put(base+kQueueHead+0x10,base+kQueueHead);
    put(base+kActiveManagers,base+kActiveHeadTag);put(base+kActiveManagers+8,primary);put(base+kActiveManagers+0x10,primary);
    put(primary,base+kManager);put(primary+8,base+kActiveManagers);put(primary+0x10,base+kActiveManagers);
    put(primary+0x78,base+kSecondary);put(primary+0x90,1u);put(primary+0x98,ctx);put(ctx,heap+0xE000);put(ctx+0x18,heap+0xE100);
    put(primary+0x110,actor);put(actor,base+kPlayer);
    const auto desc=actor+0xF30;put(desc,base+kPrivate);put(desc+0xB8,desc+0x378);
    put(desc+0xC8,exposed);put(desc+0xD0,2u);put(desc+0x368,list);put(desc+0x88,1234u);
    put(list,base+kVariableOutfit);put(list+8,2u);put(list+0x10,exposed);put(list+0x20,owned);
    put(exposed,string1);put(exposed+8,string2);put(owned,string1);put(owned+8,string2);
    std::memcpy(reinterpret_cast<void*>(string1),L"top_one",16);std::memcpy(reinterpret_cast<void*>(string2),L"bottom_one",22);
    const auto head=primary+0xF8;put(head,base+kManagedHead);put(head+8,node);put(head+0x10,node);
    put(node,base+kManaged);put(node+8,head);put(node+0x10,head);put(node+0x18,1234u);put(node+0x20,actor);
    const auto row=base+kTable;put(row,base+kWrapper);put(row+0x10,primary);put(row+0x18,state);
    put(row+0x20,static_cast<unsigned char>(1));put(row+0x28,loaded);put(loaded+0x500,original);
    put(loaded+0x518,primary);put(loaded+0x520,state);
    Choice c=choice(original);Player captured{};
    assert(find_player(base,c,captured)&&captured.primary==primary&&captured.actor==actor&&captured.source.names.size()==2);
    auto bad=c;bad.original+=8;assert(!find_player(base,bad,captured));
    put(base+kLineup+0xC8,original);assert(!find_player(base,c,captured));put(base+kLineup+0xC8,original+0x1000);
    put(row+0x20,static_cast<unsigned char>(0));assert(!find_player(base,c,captured));put(row+0x20,static_cast<unsigned char>(1));
    put(primary+0x118,heap+0xF000);assert(!find_player(base,c,captured));put(primary+0x118,std::uintptr_t(0));
    put(node+0x38,heap+0xF000);assert(!find_player(base,c,captured));put(node+0x38,std::uintptr_t(0));
    put(loaded+0x518,primary+0x78);assert(!find_player(base,c,captured));put(loaded+0x518,primary);
    // Live V2 (probe 2026-10-17): row+0x10 and loaded+0x518 both hold the
    // MANAGER_INSTANCE interface base; the worker normalizes to the full object.
    put(row+0x10,primary+0x78);put(loaded+0x518,primary+0x78);
    assert(find_player(base,c,captured)&&captured.primary==primary&&captured.table==primary+0x78&&captured.actor==actor);
    put(loaded+0x518,primary);assert(!find_player(base,c,captured));
    put(loaded+0x518,primary+0x78);put(primary+0x78,base+kManager);assert(!find_player(base,c,captured));
    put(primary+0x78,base+kSecondary);put(row+0x10,primary);put(loaded+0x518,primary);
    // WPF parity (audit D3): a three-item live source is never a carry source.
    put(exposed+0x10,string1);put(owned+0x10,string1);put(desc+0xD0,3u);put(list+8,3u);
    assert(!find_player(base,c,captured));
    put(desc+0xD0,2u);put(list+8,2u);put(exposed+0x10,std::uintptr_t(0));put(owned+0x10,std::uintptr_t(0));
    assert(find_player(base,c,captured));
    put(base+kQueueHead+8,node);assert(!find_player(base,c,captured));put(base+kQueueHead+8,base+kQueueHead);
    put(actor,base+0x6891598);assert(!find_player(base,c,captured));put(actor,base+kPlayer);
    put(base+kMainThread,GetCurrentThreadId());assert(!find_player(base,c,captured));put(base+kMainThread,GetCurrentThreadId()+1);
    put(base+kAlternate,1u);put(base+kModes+64,0x92DF06DEu);assert(!find_player(base,c,captured));put(base+kAlternate,0u);
    std::memcpy(reinterpret_cast<void*>(row+0x50),reinterpret_cast<void*>(row),0x50);
    assert(!find_player(base,c,captured));std::memset(reinterpret_cast<void*>(row+0x50),0,0x50);
    assert(find_player(base,c,captured));
    put(owned+8,string1);assert(!find_player(base,c,captured));put(owned+8,string2);
    put(exposed+8,string1);put(owned+8,string1);assert(!find_player(base,c,captured));
    assert(VirtualFree(reinterpret_cast<void*>(heap),0,MEM_RELEASE));
    assert(VirtualFree(reinterpret_cast<void*>(base),0,MEM_RELEASE));
    std::cout<<"PASS real native capture on self-owned fake image: V2 row offsets, roster aliases, queue, pending models, actor type and main-thread guard\n";
}
void write_fixture(const std::wstring& path,const std::vector<unsigned char>& bytes) {
    Handle f(CreateFileW(path.c_str(),GENERIC_WRITE,0,nullptr,CREATE_NEW,FILE_ATTRIBUTE_NORMAL,nullptr));DWORD wrote{};
    assert(f&&WriteFile(f.value,bytes.data(),DWORD(bytes.size()),&wrote,nullptr)&&wrote==bytes.size()&&FlushFileBuffers(f.value));
}
void transport_tests(const std::wstring& root) {
    assert(canonical_path(root));
    for(const auto* child:{L"App",L"Data",L"Library",L"Logs",L"Logs\\game-session"})
        assert(CreateDirectoryW((root+L"\\"+child).c_str(),nullptr));
    TrustedBinding b{};b.game_pid=123;b.game_birth=456;b.owner_pid=789;b.owner_birth=901;
    b.session="0123456789abcdef0123456789abcdef";b.portable_root=root;b.owner_executable=root+L"\\App\\fixture.exe";
    const auto prefix=root+L"\\Logs\\game-session\\nba2k27_saved_outfits_123";
    {
        Transport t;assert(t.open(b,1)&&t.belongs(b));
        auto wrong=b;wrong.game_birth++;assert(!t.belongs(wrong));wrong=b;wrong.owner_birth++;assert(!t.belongs(wrong));
        assert(!t.consume()&&!t.failed());
        write_fixture(prefix+L".plan",plan_bytes());
        const std::string marker="SAVED_OUTFITS_V1";write_fixture(prefix+L".trigger",{marker.begin(),marker.end()});
        auto token=t.consume();assert(token&&t.pinned_plan()&&!t.consume());
        assert(GetFileAttributesW((prefix+L".trigger").c_str())==INVALID_FILE_ATTRIBUTES);
        Resources r{};r.clone=0x20000;
        assert(t.event(Phase::clone_intent,r,choice()));
        Handle denied(CreateFileW((prefix+L".plan").c_str(),GENERIC_WRITE,FILE_SHARE_READ,nullptr,OPEN_EXISTING,0,nullptr));
        assert(!denied); // Retained plan denies mutation and deletion.
        Handle denied_report(CreateFileW((prefix+L".log").c_str(),GENERIC_WRITE,FILE_SHARE_READ,nullptr,OPEN_EXISTING,0,nullptr));
        assert(!denied_report);
        assert(t.report("REBUILT","Offline fixture only",1));
        assert(!t.report("REBUILT","line\nbreak",1));
        PinnedDirectory pinned;assert(pinned.open(root+L"\\Data")&&pinned.unchanged());
        assert(!MoveFileW((root+L"\\Data").c_str(),(root+L"\\Data-moved").c_str()));
    }
    Transport repeated;assert(!repeated.open(b,1)); // No overwrite or adoption.
    {
        // Check the actual flushed/read-back journal: timestamps cover arm,
        // single-use consume and player phases, in monotonic order.
        Handle journal(CreateFileW((prefix+L".v2-intents").c_str(),GENERIC_READ,FILE_SHARE_READ,nullptr,OPEN_EXISTING,0,nullptr));
        std::vector<unsigned char> bytes;assert(journal&&read_file(journal.value,65536,bytes));
        const std::string text(bytes.begin(),bytes.end());
        std::uint64_t previous{};unsigned records{};std::size_t begin{};
        while(begin<text.size()) {
            const auto end=text.find('\n',begin);assert(end!=std::string::npos);
            const auto stamp=text.find(" TICK_MS=",begin);assert(stamp!=std::string::npos&&stamp<end);
            const auto digits=text.substr(stamp+9,end-stamp-9);
            assert(!digits.empty()&&std::all_of(digits.begin(),digits.end(),[](char c){return c>='0'&&c<='9';}));
            const auto tick=std::stoull(digits);assert(tick>=previous&&tick<=GetTickCount64());
            previous=tick;++records;begin=end+1;
        }
        assert(records==3&&text.find("CONSUME SHA256=")!=std::string::npos&&text.find("EVENT=0 SEQUENCE=2")!=std::string::npos);
    }
    {
        Transport second;assert(second.open(b,2));
        const auto next=prefix+L"_match2";
        write_fixture(next+L".plan",plan_bytes());
        write_fixture(next+L".trigger",{'b','a','d'});
        assert(!second.consume()&&second.failed()&&!second.consume());
        assert(GetFileAttributesW((next+L".trigger").c_str())!=INVALID_FILE_ATTRIBUTES);
    }
    const outfit_match_v2::bridge::Config config{std::string(48,'a'),root,true};
    outfit_match_v2::bridge::failure_report(config,"Native bootstrap fixture rejected before ARMED");
    const auto failure=root+L"\\Logs\\game-session\\nba2k27_saved_outfits_"+std::to_wstring(GetCurrentProcessId())+L".log";
    std::vector<unsigned char> first,second;
    {
        Handle f(CreateFileW(failure.c_str(),GENERIC_READ,FILE_SHARE_READ,nullptr,OPEN_EXISTING,0,nullptr));
        assert(f&&read_file(f.value,4096,first));
        const std::string text(first.begin(),first.end());
        assert(text.find("STATE=FAILED")!=std::string::npos&&text.find("before ARMED")!=std::string::npos);
    }
    outfit_match_v2::bridge::failure_report(config,"Must not overwrite existing evidence");
    {
        Handle f(CreateFileW(failure.c_str(),GENERIC_READ,FILE_SHARE_READ,nullptr,OPEN_EXISTING,0,nullptr));
        assert(f&&read_file(f.value,4096,second)&&second==first);
    }
    assert(run_session(TrustedBinding{}).reason=="Invalid native authority binding");
    std::cout<<"PASS actual Win32 pinned transport, flush/readback journal, single-use trigger, foreign/replay refusal, startup failure evidence and next-match isolation\n";
}
void bridge_tests() {
    using namespace outfit_match_v2::bridge;
    auto sid=user_sid();assert(!sid.empty());LPWSTR sid_text{};
    assert(ConvertSidToStringSidW(sid.data(),&sid_text));
    const std::wstring user(sid_text);LocalFree(sid_text);
    const auto sddl=L"O:"+user+L"D:P(A;;GA;;;"+user+L")";
    PSECURITY_DESCRIPTOR descriptor{};
    assert(ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl.c_str(),SDDL_REVISION_1,&descriptor,nullptr));
    SECURITY_ATTRIBUTES attributes{sizeof(SECURITY_ATTRIBUTES),descriptor,FALSE};
    Handle event(CreateEventW(&attributes,TRUE,FALSE,nullptr));assert(event&&private_event(event.value));LocalFree(descriptor);
    const auto broad=L"O:"+user+L"D:P(A;;GA;;;"+user+L")(A;;GR;;;WD)";
    descriptor=nullptr;assert(ConvertStringSecurityDescriptorToSecurityDescriptorW(broad.c_str(),SDDL_REVISION_1,&descriptor,nullptr));
    attributes.lpSecurityDescriptor=descriptor;Handle broad_event(CreateEventW(&attributes,TRUE,FALSE,nullptr));
    assert(broad_event&&!private_event(broad_event.value));LocalFree(descriptor);
    const std::wstring log=L"C:\\Fixture\\Logs\\game-session";std::string hex;
    for(auto c:log) {for(int shift:{12,8,4,0})hex.push_back("0123456789ABCDEF"[(c>>shift)&15]);}
    std::wstring decoded;assert(decode_root(hex.c_str(),decoded)&&decoded==L"C:\\Fixture");
    assert(!decode_root("0000",decoded)&&!session_nonce("bad"));
    const std::string nonce(48,'a');assert(configure(nonce.c_str(),hex.c_str()));
    assert(configure(nonce.c_str(),hex.c_str())&&!configure(std::string(48,'b').c_str(),hex.c_str()));
    const auto name=L"\\\\.\\pipe\\OutfitV2Fixture-"+std::to_wstring(GetCurrentProcessId());
    Handle server(CreateNamedPipeW(name.c_str(),PIPE_ACCESS_DUPLEX|FILE_FLAG_FIRST_PIPE_INSTANCE,PIPE_TYPE_BYTE|PIPE_WAIT,1,64,64,0,nullptr));
    assert(server);Handle client(CreateFileW(name.c_str(),GENERIC_READ|GENERIC_WRITE,0,nullptr,OPEN_EXISTING,0,nullptr));assert(client);
    const auto connected=ConnectNamedPipe(server.value,nullptr);assert(connected||GetLastError()==ERROR_PIPE_CONNECTED);
    record_server(client.value);assert(server_pid.load()==GetCurrentProcessId()&&!poisoned.load());
    server_pid=GetCurrentProcessId()+1;record_server(client.value);assert(poisoned.load());
    std::cout<<"PASS real current-user-only event owner/DACL audit, broad-ACL rejection, immutable session config and actual named-pipe server identity\n";
}
}
int wmain(int argc,wchar_t** argv) {
    {
        // Reproduce the pre-ARMED failure without opening any game: the
        // pseudo-handle aliases INVALID_HANDLE_VALUE and fails live().
        assert(GetCurrentProcess()==INVALID_HANDLE_VALUE);
        assert(!live(GetCurrentProcess()));
        auto self=retain_current_process();assert(self&&live(self.value));
        FILETIME born{},exited{},kernel{},user{};
        assert(GetProcessTimes(self.value,&born,&exited,&kernel,&user));
        std::array<wchar_t,32768> name{};DWORD count=DWORD(name.size());
        assert(QueryFullProcessImageNameW(self.value,0,name.data(),&count));
        const std::wstring path(name.data(),count);
        assert(process_identity(self.value,ticks(born),path));
        assert(!process_identity(self.value,ticks(born)+1,path));
        assert(!process_identity(self.value,ticks(born),path+L".wrong"));
        assert(!process_identity(INVALID_HANDLE_VALUE,ticks(born),path));
        std::cout<<"PASS retained real self-process identity; pseudo/invalid handle, changed birth and changed path rejected\n";
    }
    for(const auto* name:{L"2k Launcher.exe",L"2k Launcher WPF.exe",L"2k Launcher Rust.exe",L"2k Launcher Rust Prototype.exe",L"2k-launcher-0.36.45.exe",L"2K-LAUNCHER-0.37.0.EXE"})
        assert(outfit_match_v2::bridge::launcher_name(name));
    for(const auto* name:{L"other.exe",L"2k-launcher-.exe",L"2k-launcher-0.37.0.txt",L"prefix-2k-launcher-0.37.0.exe"})
        assert(!outfit_match_v2::bridge::launcher_name(name));
    transaction_tests();alias_tests();parser_tests();win32_self_tests();native_binding_tests();
    if(argc==2)transport_tests(argv[1]);
    bridge_tests();
    return 0;
}
