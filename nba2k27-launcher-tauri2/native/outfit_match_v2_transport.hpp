#pragma once
#include "outfit_match_v2_win.hpp"
#include <optional>
namespace outfit_match_v2 {
inline bool parse_plan(const std::vector<unsigned char>& data,DWORD pid,std::uint64_t birth,std::vector<Choice>& out) {
    constexpr std::size_t stride=16+7*512;
    if(data.size()<24) return false;
    auto u32=[&](std::size_t at){unsigned v{};std::memcpy(&v,data.data()+at,4);return v;};
    auto u64=[&](std::size_t at){std::uint64_t v{};std::memcpy(&v,data.data()+at,8);return v;};
    const auto count=u32(8);
    if(u32(0)!=0x33504F42 || u32(4)!=pid || count<1 || count>10 || u32(12) || u64(16)!=birth ||
        data.size()!=24+count*stride) return false;
    std::vector<Choice> parsed;
    for(unsigned n=0;n<count;++n) {
        const auto offset=24+n*stride;
        const auto pieces=u32(offset+8);
        if(pieces<3 || pieces>7 || u32(offset+12)) return false;
        Choice c{};c.original=u64(offset);
        for(unsigned piece=0;piece<7;++piece) {
            std::wstring name;bool ended=false;
            for(unsigned j=0;j<256;++j) {
                unsigned short unit{}; std::memcpy(&unit,data.data()+offset+16+piece*512+j*2,2);
                if(!unit) {ended=true;continue;}
                if(ended || piece>=pieces) return false;
                name.push_back(wchar_t(unit));
            }
            if(!ended) return false;
            if(piece<pieces) c.items.push_back(std::move(name));
        }
        parsed.push_back(std::move(c));
    }
    if(!valid_choices(parsed)) return false;
    out=std::move(parsed);return true;
}
inline std::vector<ExpansionCall> expansion_calls(const ExpansionPlan& p) {
    if(p.stage==ExpansionStage::release_init)return {
        {ExpansionAction::release_list,p.base+kReleaseList,p.owner,0,0,0,{}},
        {ExpansionAction::initialize_list,p.base+kInitList,p.owner,p.heap,p.count,0,{}}};
    std::vector<ExpansionCall> calls;
    for(unsigned i=0;i<p.items.size();++i)
        calls.push_back({ExpansionAction::set_item,p.base+kSetItem,p.owner,0,0,i,p.items[i]});
    calls.push_back({ExpansionAction::bind_list,p.base+kBind,p.clone,p.exposed,p.count,0,{}});
    return calls;
}
inline bool valid_expansion_plan(const ExpansionPlan& p) {
    auto pointer=[](std::uintptr_t v){return v>=0x10000 && v<0x7FFF00000000ull;};
    if(p.stage!=ExpansionStage::release_init && p.stage!=ExpansionStage::set_bind)return false;
    for(auto v:{p.base,p.original,p.clone,p.owner,p.heap,p.actor,p.source,p.row,p.manager,p.loaded})
        if(!pointer(v))return false;
    if(!p.state || p.base>0x7FFF00000000ull-0x35BDE000 || p.clone==p.actor || p.clone==p.source || p.owner==p.source ||
       p.count!=p.items.size() || !valid_choices({Choice{p.original,p.items}}))return false;
    if(p.stage==ExpansionStage::release_init) {if(p.exposed || p.owned)return false;}
    else if(!pointer(p.exposed) || !pointer(p.owned) || p.exposed==p.owned)return false;
    return p.calls==expansion_calls(p);
}
class Transport {
    PinnedDirectory directory_,app_,data_,library_;
    Handle journal_,report_,plan_;
    StableId journal_id_{},report_id_{},plan_id_{};
    std::vector<unsigned char> plan_bytes_;
    std::wstring prefix_;
    std::string session_;
    DWORD pid_{};std::uint64_t birth_{},sequence_{},journal_bytes_{};
    DWORD owner_pid_{};std::uint64_t owner_birth_{};
    std::wstring root_,owner_executable_;
    bool consumed_{false},poisoned_{false};
    struct ExpansionClaim {std::uintptr_t original{},clone{};bool second{};};
    std::array<ExpansionClaim,10> expansion_claims_{};
    unsigned expansion_count_{};
#ifdef OUTFIT_MATCH_V2_OFFLINE_TEST
    unsigned append_calls_{},flush_calls_{};
    mutable unsigned pinned_plan_calls_{},ordinary_calls_{};
    unsigned append_fault_{};
#endif
    bool ordinary(HANDLE h,const StableId& expected) const {
#ifdef OUTFIT_MATCH_V2_OFFLINE_TEST
        ++ordinary_calls_;
#endif
        StableId id{};return directory_.unchanged() && app_.unchanged() && data_.unchanged() && library_.unchanged() &&
            file_id(h,false,id) && id==expected;
    }
    bool append(const std::string& record) {
        // Same monotonic clock as the readiness/poll loops. Timestamp the
        // existing durable record rather than adding another disk flush or
        // changing any admission, consumption or ownership step.
        if(record.empty() || record.back()!='\n') return false;
        const std::string event=record.substr(0,record.size()-1)+" TICK_MS="+std::to_string(GetTickCount64())+"\n";
        if(poisoned_ || !journal_ || !ordinary(journal_.value,journal_id_) || event.size()>4096 ||
            sequence_>=8192 || journal_bytes_>4*1024*1024-event.size()) return false;
        FILE_STANDARD_INFO st{};
        if(!GetFileInformationByHandleEx(journal_.value,FileStandardInfo,&st,sizeof st) ||
            std::uint64_t(st.EndOfFile.QuadPart)!=journal_bytes_) {poisoned_=true;return false;}
        LARGE_INTEGER pos{};pos.QuadPart=LONGLONG(journal_bytes_); DWORD wrote{},got{};
        std::vector<char> check(event.size());
#ifdef OUTFIT_MATCH_V2_OFFLINE_TEST
        ++append_calls_;
        const auto fault=std::exchange(append_fault_,0u);
        if(fault==1) {
            const auto size=DWORD(event.size()/2);
            SetFilePointerEx(journal_.value,pos,nullptr,FILE_BEGIN);
            WriteFile(journal_.value,event.data(),size,&wrote,nullptr);
            FlushFileBuffers(journal_.value);++flush_calls_;
            poisoned_=true;return false;
        }
#endif
        const bool ok=SetFilePointerEx(journal_.value,pos,nullptr,FILE_BEGIN) &&
            WriteFile(journal_.value,event.data(),DWORD(event.size()),&wrote,nullptr) && wrote==event.size() &&
            FlushFileBuffers(journal_.value) && SetFilePointerEx(journal_.value,pos,nullptr,FILE_BEGIN) &&
            ReadFile(journal_.value,check.data(),DWORD(check.size()),&got,nullptr) && got==event.size() &&
            std::equal(check.begin(),check.end(),event.begin()) && ordinary(journal_.value,journal_id_);
#ifdef OUTFIT_MATCH_V2_OFFLINE_TEST
        if(ok)++flush_calls_;
        // The bytes really reached disk; model a reported flush failure or
        // mismatching readback without granting a capability after uncertainty.
        if(fault==2 || fault==3) {poisoned_=true;return false;}
#endif
        if(!ok) {poisoned_=true;return false;}
        journal_bytes_+=event.size();++sequence_;return true;
    }
public:
    bool open(const TrustedBinding& b,unsigned number) {
        if(journal_ || report_ || number<1 || number>10000 || !b.game_pid || !b.game_birth ||
            b.session.size()<16 || b.session.size()>96 || !canonical_path(b.portable_root) ||
            !app_.open(b.portable_root+L"\\App") || !data_.open(b.portable_root+L"\\Data") ||
            !library_.open(b.portable_root+L"\\Library") || !directory_.open(b.portable_root+L"\\Logs\\game-session")) return false;
        const auto suffix=number==1?L"":L"_match"+std::to_wstring(number);
        prefix_=directory_.path()+L"\\nba2k27_saved_outfits_"+std::to_wstring(b.game_pid)+suffix;
        if(prefix_.size()>215) return false;
        journal_=Handle(CreateFileW((prefix_+L".v2-intents").c_str(),GENERIC_READ|GENERIC_WRITE,FILE_SHARE_READ,nullptr,
            CREATE_NEW,FILE_FLAG_OPEN_REPARSE_POINT|FILE_FLAG_WRITE_THROUGH,nullptr));
        if(!journal_ || !file_id(journal_.value,false,journal_id_)) return false;
        report_=Handle(CreateFileW((prefix_+L".log").c_str(),GENERIC_READ|GENERIC_WRITE,FILE_SHARE_READ,nullptr,
            CREATE_NEW,FILE_FLAG_OPEN_REPARSE_POINT|FILE_FLAG_WRITE_THROUGH,nullptr));
        if(!report_ || !file_id(report_.value,false,report_id_)) return false;
        pid_=b.game_pid;birth_=b.game_birth;session_=b.session;root_=b.portable_root;
        owner_pid_=b.owner_pid;owner_birth_=b.owner_birth;owner_executable_=b.owner_executable;
        return append("SCHEMA=blacktop-outfit-native-v2-intents-v1 PID="+std::to_string(pid_)+" BIRTH="+
            std::to_string(birth_)+" MATCH="+std::to_string(number)+" SESSION="+session_+"\n") &&
            report("ARMED","Version2 worker awaiting one durable owned plan",0);
    }
    bool pinned_plan() const {
#ifdef OUTFIT_MATCH_V2_OFFLINE_TEST
        ++pinned_plan_calls_;
#endif
        if(!consumed_ || !plan_ || !ordinary(plan_.value,plan_id_)) return false;
        std::vector<unsigned char> now;
        return read_file(plan_.value,24+10*(16+7*512),now) && now==plan_bytes_;
    }
    bool belongs(const TrustedBinding& b) const {
        return pid_==b.game_pid && birth_==b.game_birth && owner_pid_==b.owner_pid && owner_birth_==b.owner_birth &&
            session_==b.session && equal_path(root_,b.portable_root) && equal_path(owner_executable_,b.owner_executable);
    }
    std::optional<ConsumedPlan> consume() {
        if(consumed_ || poisoned_ || !directory_.unchanged()) return {};
        Handle trigger(CreateFileW((prefix_+L".trigger").c_str(),GENERIC_READ|DELETE,FILE_SHARE_READ,nullptr,
            OPEN_EXISTING,FILE_FLAG_OPEN_REPARSE_POINT,nullptr));
        if(!trigger) return {};
        StableId trigger_id{};std::vector<unsigned char> token;
        constexpr char marker[]="SAVED_OUTFITS_V1";
        if(!file_id(trigger.value,false,trigger_id) || !read_file(trigger.value,64,token) ||
            token.size()!=sizeof marker-1 || std::memcmp(token.data(),marker,sizeof marker-1)) {poisoned_=true;return {};}
        plan_=Handle(CreateFileW((prefix_+L".plan").c_str(),GENERIC_READ,FILE_SHARE_READ,nullptr,
            OPEN_EXISTING,FILE_FLAG_OPEN_REPARSE_POINT,nullptr));
        std::vector<Choice> choices;
        if(!plan_ || !file_id(plan_.value,false,plan_id_) || !read_file(plan_.value,24+10*(16+7*512),plan_bytes_) ||
            !parse_plan(plan_bytes_,pid_,birth_,choices)) {poisoned_=true;return {};}
        consumed_=true; // No subsequent call may consume this match again.
        const auto plan_hash=digest(plan_bytes_);
        if(plan_hash.empty() || !append("CONSUME SHA256="+plan_hash+"\n") || !pinned_plan()) {poisoned_=true;return {};}
        StableId trigger_after{}; std::vector<unsigned char> after;
        FILE_DISPOSITION_INFO disposition{};disposition.DeleteFile=TRUE;
        if(!file_id(trigger.value,false,trigger_after) || trigger_after!=trigger_id || !read_file(trigger.value,64,after) ||
            after!=token || !SetFileInformationByHandle(trigger.value,FileDispositionInfo,&disposition,sizeof disposition)) {
            poisoned_=true;return {};
        }
        return ConsumedPlan(std::move(choices));
    }
    std::optional<ExpansionPermit> expansion_bundle(const ExpansionPlan& plan,const ExpansionPermit* previous=nullptr) {
        if(poisoned_ || !pinned_plan() || !valid_expansion_plan(plan))return {};
        ExpansionClaim* claim=nullptr;
        for(unsigned i=0;i<expansion_count_;++i)if(expansion_claims_[i].original==plan.original || expansion_claims_[i].clone==plan.clone)
            {claim=&expansion_claims_[i];break;}
        if(plan.stage==ExpansionStage::release_init) {
            if(previous || claim || expansion_count_>=expansion_claims_.size())return {};
        }else {
            if(!previous || previous->transport_!=this || !previous->complete() ||
               !claim || claim->original!=plan.original || claim->clone!=plan.clone || claim->second)return {};
            auto before=plan;before.stage=ExpansionStage::release_init;before.exposed=before.owned=0;before.calls=expansion_calls(before);
            if(previous->plan_!=before)return {};
        }
        std::vector<Choice> choices;
        if(!parse_plan(plan_bytes_,pid_,birth_,choices) ||
           std::none_of(choices.begin(),choices.end(),[&](const Choice& c){return c.original==plan.original && c.items==plan.items;}))return {};
        const auto hash=digest(plan_bytes_);if(hash.size()!=64)return {};
        const auto decimal=[](auto value){return std::to_string(value);};
        std::string file;
        constexpr char digits[]="0123456789ABCDEF";
        for(auto b:plan_id_.index){file.push_back(digits[b>>4]);file.push_back(digits[b&15]);}
        const std::string stage=plan.stage==ExpansionStage::release_init?"A":"B";
        std::string record="EXPANSION_BUNDLE=1 STAGE="+stage+" SEQUENCE="+decimal(sequence_)+
            " PID="+decimal(pid_)+" BIRTH="+decimal(birth_)+" SESSION="+session_+
            " PLAN_SHA256="+hash+" PLAN_VOLUME="+decimal(plan_id_.volume)+" PLAN_FILE="+file+
            " BASE="+decimal(plan.base)+" ORIGINAL="+decimal(plan.original)+" CLONE="+decimal(plan.clone)+
            " OWNER="+decimal(plan.owner)+" HEAP="+decimal(plan.heap)+" ACTOR="+decimal(plan.actor)+
            " SOURCE="+decimal(plan.source)+" ROW="+decimal(plan.row)+" MANAGER="+decimal(plan.manager)+
            " LOADED="+decimal(plan.loaded)+" STATE="+decimal(plan.state)+" EXPOSED="+decimal(plan.exposed)+
            " OWNED="+decimal(plan.owned)+" COUNT="+decimal(plan.count)+" ACTIONS="+decimal(plan.calls.size())+"\n";
        for(const auto& call:plan.calls) {
            std::string name;for(auto c:call.name)name.push_back(char(c));
            record+="CALL="+decimal(unsigned(call.action))+" FUNCTION="+decimal(call.function)+
                " TARGET="+decimal(call.target)+" ARGUMENT="+decimal(call.argument)+
                " COUNT="+decimal(call.count)+" INDEX="+decimal(call.index)+
                " NAME_LENGTH="+decimal(name.size())+" NAME="+name+"\n";
        }
        record+="END_EXPANSION_BUNDLE="+stage+"\n";
        if(!append(record))return {};
        if(plan.stage==ExpansionStage::release_init)
            expansion_claims_[expansion_count_++]={plan.original,plan.clone,false};
        else claim->second=true;
        return ExpansionPermit(this,plan);
    }
#ifdef OUTFIT_MATCH_V2_OFFLINE_TEST
    void inject_append_fault(unsigned fault){append_fault_=fault;}
    unsigned append_calls()const{return append_calls_;}
    unsigned flush_calls()const{return flush_calls_;}
    unsigned pinned_plan_calls()const{return pinned_plan_calls_;}
    unsigned ordinary_calls()const{return ordinary_calls_;}
#endif
    bool event(Phase phase,const Resources& r,const Choice& c) {
        return pinned_plan() && append("EVENT="+std::to_string(unsigned(phase))+" SEQUENCE="+std::to_string(sequence_)+
            " ORIGINAL="+std::to_string(c.original)+" CLONE="+std::to_string(r.clone)+" PACKET="+
            std::to_string(r.packet)+" TRANSFER="+std::to_string(r.transfer_attempted?1:0)+"\n");
    }
    bool report(const char* state,const std::string& reason,unsigned rebuilt) {
        if(!report_ || !ordinary(report_.value,report_id_) || reason.size()>256 || rebuilt>10 ||
            reason.find_first_of("\r\n")!=std::string::npos) return false;
        const std::string text="SCHEMA=blacktop-saved-outfits-v1\nPID="+std::to_string(pid_)+"\nSTATE="+state+
            "\nREASON="+reason+"\nPLAYERS_REBUILT="+std::to_string(rebuilt)+"\nGAME_CODE_HOOKS=0\nOBJECT_HOOKS=0\n";
        LARGE_INTEGER zero{};DWORD wrote{};std::vector<unsigned char> check;
        return text.size()<=2048 && SetFilePointerEx(report_.value,zero,nullptr,FILE_BEGIN) &&
            WriteFile(report_.value,text.data(),DWORD(text.size()),&wrote,nullptr) && wrote==text.size() &&
            SetEndOfFile(report_.value) && FlushFileBuffers(report_.value) && read_file(report_.value,2048,check) &&
            check.size()==text.size() && std::equal(check.begin(),check.end(),text.begin());
    }
    bool failed() const {return poisoned_;}
};
} // namespace outfit_match_v2
