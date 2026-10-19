#pragma once
#include "outfit_match_v2_native.hpp"
#include "process_snapshot_v2_win.hpp"
#include <aclapi.h>
#include <tlhelp32.h>
#include <atomic>
#include <optional>
namespace outfit_match_v2::bridge {
struct Config { std::string session;std::wstring root;bool enabled{};bool operator==(const Config&)const=default; };
inline Config config;
inline bool configured=false;
inline std::atomic<DWORD> server_pid{0};
inline std::atomic<bool> poisoned{false},started{false};
inline bool session_nonce(const std::string& s) {
    if(s.size()!=48)return false;
    for(auto c:s)if(!((c>='a'&&c<='f')||(c>='A'&&c<='F')||(c>='0'&&c<='9')))return false;
    return true;
}
inline bool decode_root(const char* encoded,std::wstring& root) {
    if(!encoded)return false;const std::string h(encoded);
    if(h.empty()||h.size()>1600||h.size()%4)return false;
    std::wstring path;
    for(std::size_t i=0;i<h.size();i+=4) {
        unsigned unit{};
        for(unsigned j=0;j<4;++j) {const char c=h[i+j];int digit=c>='0'&&c<='9'?c-'0':c>='A'&&c<='F'?c-'A'+10:c>='a'&&c<='f'?c-'a'+10:-1;
            if(digit<0)return false;unit=(unit<<4)|unsigned(digit);}
        if(unit<32||(unit>=0xD800&&unit<=0xDFFF))return false;path.push_back(wchar_t(unit));
    }
    const std::wstring suffix=L"\\Logs\\game-session";
    if(path.size()<=suffix.size()||!equal_path(path.substr(path.size()-suffix.size()),suffix))return false;
    root=path.substr(0,path.size()-suffix.size());return canonical_path(root);
}
inline bool configure(const char* session,const char* portable) {
    Config next{};if(!decode_root(portable,next.root))return false;
    if(session){next.session=session;if(!session_nonce(next.session))return false;next.enabled=true;}
    if(configured)return config==next;
    config=std::move(next);configured=true;return true;
}
inline void record_server(HANDLE pipe) {
    if(!config.enabled)return;DWORD observed{};
    if(!GetNamedPipeServerProcessId(pipe,&observed)||!observed){poisoned=true;return;}
    DWORD expected=0;
    if(!server_pid.compare_exchange_strong(expected,observed)&&expected!=observed)poisoned=true;
}
// The diagnostics route is strict portable-only. It is needed before the
// provider parser runs, so it reads only the existing fixed game-side config.
inline std::wstring diagnostics_for_executable(const wchar_t* executable) {
    const std::wstring exe(executable);const auto slash=exe.find_last_of(L'\\');
    if(slash==std::wstring::npos||!equal_path(exe.substr(slash+1),L"NBA2K27.exe"))return {};
    Handle f(CreateFileW((exe.substr(0,slash)+L"\\nba2k27_provider_audit.cfg").c_str(),GENERIC_READ,
        FILE_SHARE_READ|FILE_SHARE_WRITE|FILE_SHARE_DELETE,nullptr,OPEN_EXISTING,FILE_FLAG_OPEN_REPARSE_POINT,nullptr));
    StableId id{};std::vector<unsigned char> b;
    if(!f||!file_id(f.value,false,id)||!read_file(f.value,2047,b)||b.empty())return {};
    std::string text(b.begin(),b.end()),encoded;std::size_t begin=0;unsigned found{};
    while(begin<text.size()) {const auto end=text.find('\n',begin);auto line=text.substr(begin,end==std::string::npos?std::string::npos:end-begin);
        if(!line.empty()&&line.back()=='\r')line.pop_back();
        if(line.starts_with("PORTABLE_LOG_DIR_HEX=")){encoded=line.substr(21);++found;}
        if(end==std::string::npos)break;begin=end+1;}
    std::wstring root;if(found!=1||!decode_root(encoded.c_str(),root))return {};
    PinnedDirectory pinned;if(!pinned.open(root+L"\\Logs\\game-session"))return {};
    return root+L"\\Logs\\game-session\\";
}
inline std::vector<unsigned char> user_sid() {
    Handle token;HANDLE handle{};
    if(!OpenProcessToken(GetCurrentProcess(),TOKEN_QUERY,&handle))return {};token=Handle(handle);
    DWORD size{};GetTokenInformation(token.value,TokenUser,nullptr,0,&size);
    if(size<sizeof(TOKEN_USER)||size>65536)return {};
    std::vector<unsigned char> buffer(size);
    if(!GetTokenInformation(token.value,TokenUser,buffer.data(),size,&size))return {};
    const auto* user=reinterpret_cast<const TOKEN_USER*>(buffer.data());
    if(!IsValidSid(user->User.Sid))return {};const auto count=GetLengthSid(user->User.Sid);
    if(!count||count>1024)return {};std::vector<unsigned char> result(count);
    if(!CopySid(count,result.data(),user->User.Sid))return {};return result;
}
inline bool private_event(HANDLE event) {
    auto sid=user_sid();if(sid.empty())return false;
    PSID owner{};PACL acl{};PSECURITY_DESCRIPTOR descriptor{};
    const auto status=GetSecurityInfo(event,SE_KERNEL_OBJECT,OWNER_SECURITY_INFORMATION|DACL_SECURITY_INFORMATION,&owner,nullptr,&acl,nullptr,&descriptor);
    if(status!=ERROR_SUCCESS)return false;
    struct Release {PSECURITY_DESCRIPTOR p;~Release(){if(p)LocalFree(p);}} release{descriptor};
    if(!owner||!EqualSid(owner,sid.data())||!acl||acl->AceCount!=1)return false;
    void* raw{};if(!GetAce(acl,0,&raw)||!raw)return false;
    const auto* ace=static_cast<const ACCESS_ALLOWED_ACE*>(raw);
    SECURITY_DESCRIPTOR_CONTROL control{};DWORD revision{};
    return ace->Header.AceType==ACCESS_ALLOWED_ACE_TYPE&&ace->Header.AceFlags==0&&
        (ace->Mask==GENERIC_ALL||ace->Mask==EVENT_ALL_ACCESS)&&EqualSid(const_cast<DWORD*>(&ace->SidStart),sid.data())&&
        GetSecurityDescriptorControl(descriptor,&control,&revision)&&(control&SE_DACL_PROTECTED);
}
inline bool launcher_name(const std::wstring& name) {
    return equal_path(name,L"2k Launcher.exe")||equal_path(name,L"2k Launcher WPF.exe")||
        equal_path(name,L"2k Launcher Rust.exe")||equal_path(name,L"2k Launcher Rust Prototype.exe")||
        equal_path(name,L"2K Launcher Tauri QA.exe")||equal_path(name,L"nba2k27-launcher-tauri2.exe")||
        (name.size()>16&&CompareStringOrdinal(name.data(),12,L"2k-launcher-",12,TRUE)==CSTR_EQUAL&&
         CompareStringOrdinal(name.data()+name.size()-4,4,L".exe",4,TRUE)==CSTR_EQUAL);
}
// One unchanged predicate is shared by the new complete name snapshot,
// original Toolhelp fallback, and failure-name diagnostics. PID exemptions
// apply to launchers only; anti-cheat names and foreign game PIDs stay blocking.
inline bool protected_entry(const std::wstring& name,DWORD pid,DWORD worker,DWORD parent) {
    return (name.size()>=13&&CompareStringOrdinal(name.data(),13,L"EasyAntiCheat",13,TRUE)==CSTR_EQUAL)||
        (equal_path(name,L"NBA2K27.exe")&&pid!=GetCurrentProcessId())||
        (pid!=worker&&pid!=parent&&launcher_name(name));
}
// ASCII names of the processes protected_processes() would refuse, for the
// saved-outfits FAILED reason (bounded; non-ASCII becomes '?').
inline std::string protected_names(DWORD worker,DWORD parent) {
    Handle snapshot(CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS,0));
    if(!snapshot)return "process list unavailable";PROCESSENTRY32W entry{};entry.dwSize=sizeof entry;
    if(!Process32FirstW(snapshot.value,&entry))return "process list unavailable";
    std::string names;
    do {
        const std::wstring name(entry.szExeFile);
        const bool blocking=protected_entry(name,entry.th32ProcessID,worker,parent);
        if(!blocking)continue;
        std::string item;for(auto c:name)item.push_back(c>=32&&c<127?char(c):'?');
        item+=" (PID "+std::to_string(entry.th32ProcessID)+")";
        if(names.size()+item.size()+2>150){names+=", ...";break;}
        if(!names.empty())names+=", ";
        names+=item;
    } while(Process32NextW(snapshot.value,&entry));
    return names.empty()?"none found at report time":names;
}
inline bool toolhelp_protected_processes(DWORD worker,DWORD parent) {
    Handle snapshot(CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS,0));
    if(!snapshot)return true;PROCESSENTRY32W entry{};entry.dwSize=sizeof entry;
    if(!Process32FirstW(snapshot.value,&entry))return true;
    do {
        const std::wstring name(entry.szExeFile);
        if(protected_entry(name,entry.th32ProcessID,worker,parent))return true;
    } while(Process32NextW(snapshot.value,&entry));
    return GetLastError()!=ERROR_NO_MORE_FILES;
}
inline bool protected_processes(DWORD worker,DWORD parent) {
    return process_snapshot::guard_with([&]{return process_snapshot::fresh_basic(GetCurrentProcessId());},
        [&]{return toolhelp_protected_processes(worker,parent);},
        [&](const process_snapshot::Row& row){return protected_entry(row.name,row.pid,worker,parent);});
}
class Session {
    Config config_;Handle lease_,worker_,parent_,cancel_,permit_;
    StableId lease_id_{};std::vector<unsigned char> bytes_;
    PinnedDirectory root_,data_,app_,library_;
    bool(*anti_cheat_)(){};
    TrustedBinding binding_{};
    bool unchanged() {
        StableId now{};std::vector<unsigned char> bytes;
        return root_.unchanged()&&data_.unchanged()&&app_.unchanged()&&library_.unchanged()&&
            file_id(lease_.value,false,now)&&now==lease_id_&&read_file(lease_.value,1632,bytes)&&bytes==bytes_;
    }
public:
    static bool owned(void* raw) noexcept {
        try {auto& s=*static_cast<Session*>(raw);
            return !poisoned.load()&&server_pid.load()==s.binding_.owner_pid&&
                WaitForSingleObject(s.cancel_.value,0)==WAIT_TIMEOUT&&WaitForSingleObject(s.permit_.value,0)==WAIT_OBJECT_0&&
                process_identity(s.worker_.value,s.binding_.owner_birth,s.binding_.owner_executable)&&
                process_identity(s.parent_.value,s.parent_birth_,s.parent_path_)&&s.unchanged();
        }catch(...){return false;}
    }
    static bool protected_present(void* raw) noexcept {
        try {auto& s=*static_cast<Session*>(raw);return !owned(raw)||!s.anti_cheat_||s.anti_cheat_()||
            protected_processes(s.binding_.owner_pid,s.parent_pid_);
        }catch(...){return true;}
    }
    DWORD parent_pid_{};std::uint64_t parent_birth_{};std::wstring parent_path_;
    // Which open() check failed (written to the saved-outfits log by worker()).
    std::string reason_="Bridge not opened";bool transient_{};
    bool fail(std::string why,bool transient=false){reason_=std::move(why);transient_=transient;return false;}
    bool open(const Config& c,bool(*anti_cheat)()) {
        config_=c;anti_cheat_=anti_cheat;
        if(!c.enabled||!session_nonce(c.session)||!root_.open(c.root)||!data_.open(c.root+L"\\Data")||
            !app_.open(c.root+L"\\App")||!library_.open(c.root+L"\\Library"))return fail("Portable session directories or session id rejected");
        const std::wstring nonce(c.session.begin(),c.session.end());
        const auto event=L"Local\\NBA2K27OutfitsV2-"+nonce;
        cancel_=Handle(OpenEventW(SYNCHRONIZE|READ_CONTROL,FALSE,(event+L"-Cancel").c_str()));
        permit_=Handle(OpenEventW(SYNCHRONIZE|READ_CONTROL,FALSE,(event+L"-Permit").c_str()));
        if(!cancel_||!permit_||!private_event(cancel_.value)||!private_event(permit_.value))return fail("Launcher Cancel/Permit events missing or not private");
        const auto path=c.root+L"\\Data\\blacktop-outfit-v2-"+nonce+L".lease";
        const auto end=GetTickCount64()+120000;
        while(GetTickCount64()<end&&WaitForSingleObject(cancel_.value,0)==WAIT_TIMEOUT) {
            lease_=Handle(CreateFileW(path.c_str(),GENERIC_READ,FILE_SHARE_READ,nullptr,OPEN_EXISTING,FILE_FLAG_OPEN_REPARSE_POINT,nullptr));
            if(lease_)break;Sleep(100);
        }
        if(!lease_||!file_id(lease_.value,false,lease_id_)||!read_file(lease_.value,1632,bytes_)||bytes_.size()!=1632||
            std::memcmp(bytes_.data(),"2KOV2L01",8)||std::memcmp(bytes_.data()+48,c.session.data(),48))
            return fail(WaitForSingleObject(cancel_.value,0)==WAIT_OBJECT_0?"Launcher cancelled before the lease was published":"lease-mismatch: outfit lease absent after 120 s or not this session");
        auto u32=[&](std::size_t n){unsigned v{};std::memcpy(&v,bytes_.data()+n,4);return v;};
        auto u64=[&](std::size_t n){std::uint64_t v{};std::memcpy(&v,bytes_.data()+n,8);return v;};
        auto path_at=[&](std::size_t n)->std::wstring {
            std::array<wchar_t,256> words{};std::memcpy(words.data(),bytes_.data()+n,512);
            const auto zero=std::find(words.begin(),words.end(),wchar_t(0));
            if(zero==words.begin()||zero==words.end()||std::any_of(zero,words.end(),[](auto v){return v!=0;}))return {};
            return {words.begin(),zero};
        };
        binding_.game_pid=u32(8);binding_.owner_pid=u32(12);parent_pid_=u32(16);
        binding_.game_birth=u64(24);binding_.owner_birth=u64(32);parent_birth_=u64(40);
        binding_.game_executable=path_at(96);binding_.owner_executable=path_at(608);parent_path_=path_at(1120);
        binding_.portable_root=c.root;binding_.session=c.session;binding_.cancellation=cancel_.value;
        binding_.session_owned=&owned;binding_.protected_present=&protected_present;binding_.context=this;
        const auto app=c.root+L"\\App\\";
        if(u32(20)||binding_.game_pid!=GetCurrentProcessId()||!binding_.game_birth||!binding_.owner_birth||!parent_birth_||
            !parent_pid_||parent_pid_==binding_.game_pid||parent_pid_==binding_.owner_pid||
            server_pid.load()!=binding_.owner_pid||!canonical_path(parent_path_)||parent_path_.size()<=app.size()||
            !equal_path(parent_path_.substr(0,app.size()),app))return fail("Outfit lease identities do not match this game, worker or launcher");
        worker_=Handle(OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION|SYNCHRONIZE,FALSE,binding_.owner_pid));
        parent_=Handle(OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION|SYNCHRONIZE,FALSE,parent_pid_));
        if(!worker_||!parent_)return fail("Launcher worker or parent process is not running");
        while(GetTickCount64()<end&&WaitForSingleObject(cancel_.value,0)==WAIT_TIMEOUT&&WaitForSingleObject(permit_.value,0)==WAIT_TIMEOUT) {
            if(!process_identity(worker_.value,binding_.owner_birth,binding_.owner_executable)||!process_identity(parent_.value,parent_birth_,parent_path_))return fail("Launcher worker or parent identity changed");
            Sleep(100);
        }
        if(!owned(this))return fail(WaitForSingleObject(cancel_.value,0)==WAIT_OBJECT_0?"Launcher cancelled the outfit lease":
            WaitForSingleObject(permit_.value,0)!=WAIT_OBJECT_0?"permit-timeout: launcher did not permit the outfit worker within 120 s":
            "Outfit ownership proof failed (pipe owner, process identity or lease changed)");
        if(anti_cheat_&&anti_cheat_())return fail("Anti-cheat module present");
        if(protected_processes(binding_.owner_pid,parent_pid_))
            return fail("protected-present: "+protected_names(binding_.owner_pid,parent_pid_),true);
        return true;
    }
    const TrustedBinding& native_binding() const { return binding_; }
    // Appearance undo only, and only after that worker owns an actual successful
    // atomic effect receipt. This grants NO new forward publication authority.
    // An ordinary Cancel/Permit transition must not erase conditional undo proof;
    // exact process/lease/directory ownership and rival-process checks remain.
    bool retained_appearance() noexcept {
        try {
            auto game=retain_current_process();
            return game && !poisoned.load() && server_pid.load()==binding_.owner_pid &&
                binding_.game_pid==GetCurrentProcessId() &&
                process_identity(game.value,binding_.game_birth,binding_.game_executable) &&
                process_identity(worker_.value,binding_.owner_birth,binding_.owner_executable) &&
                process_identity(parent_.value,parent_birth_,parent_path_) && unchanged() &&
                anti_cheat_ && !anti_cheat_() && !protected_processes(binding_.owner_pid,parent_pid_);
        } catch(...) { return false; }
    }
    // Duplicate only a reply file handle from an already authenticated lease
    // process. The appearance bridge separately proves exact fixed path/FileID,
    // ordinary one-link file and access; no target memory handle is accepted.
    bool duplicate_appearance_reply(DWORD source_pid,std::uint64_t token,HANDLE& output) noexcept {
        output=INVALID_HANDLE_VALUE;
        try {
            if(!retained_appearance() || token<4 || token>0x7fffFFFFffffull)return false;
            const bool worker=source_pid==binding_.owner_pid;
            if(!worker && source_pid!=parent_pid_)return false;
            const auto birth=worker?binding_.owner_birth:parent_birth_;
            const auto& path=worker?binding_.owner_executable:parent_path_;
            Handle process(OpenProcess(PROCESS_DUP_HANDLE|PROCESS_QUERY_LIMITED_INFORMATION|SYNCHRONIZE,FALSE,source_pid));
            if(!process || !process_identity(process.value,birth,path))return false;
            HANDLE copied{};
            if(!DuplicateHandle(process.value,reinterpret_cast<HANDLE>(static_cast<std::uintptr_t>(token)),GetCurrentProcess(),&copied,0,FALSE,DUPLICATE_SAME_ACCESS))return false;
            Handle held(copied);
            if(!process_identity(process.value,birth,path) || !retained_appearance())return false;
            output=std::exchange(held.value,INVALID_HANDLE_VALUE);return true;
        } catch(...) {return false;}
    }
    Outcome run(){return run_session(binding_);}
};
struct Start {Config config;HMODULE module{};bool(*anti_cheat)(){};};
// Standard saved-outfits report (same schema as Transport::report) so the
// launcher shows why the worker never armed. CREATE_NEW: never overwrite.
inline void failure_report(const Config& c,const std::string& reason) {
    PinnedDirectory directory;if(!directory.open(c.root+L"\\Logs\\game-session"))return;
    const auto path=directory.path()+L"\\nba2k27_saved_outfits_"+std::to_wstring(GetCurrentProcessId())+L".log";
    Handle f(CreateFileW(path.c_str(),GENERIC_WRITE,FILE_SHARE_READ,nullptr,CREATE_NEW,
        FILE_FLAG_OPEN_REPARSE_POINT|FILE_FLAG_WRITE_THROUGH,nullptr));
    if(!f)return;
    const std::string text="SCHEMA=blacktop-saved-outfits-v1\nPID="+std::to_string(GetCurrentProcessId())+
        "\nSTATE=FAILED\nREASON=Version2 worker did not arm: "+reason.substr(0,200)+
        "\nPLAYERS_REBUILT=0\nGAME_CODE_HOOKS=0\nOBJECT_HOOKS=0\n";
    DWORD wrote{};WriteFile(f.value,text.data(),DWORD(text.size()),&wrote,nullptr);FlushFileBuffers(f.value);
}
inline DWORD WINAPI worker(void* raw) {
    std::unique_ptr<Start> start(static_cast<Start*>(raw));const auto module=start->module;
    DWORD code=1;
    try {
        // A briefly running launcher/smoke-test executable is transient: retry
        // for up to 10 minutes. Any other refusal is final and reported.
        const auto deadline=GetTickCount64()+600000;
        for(;;) {
            Session session;
            if(session.open(start->config,start->anti_cheat)){
                const auto result=session.run();
                // Authority/transport failures before ARMED used to disappear:
                // the launcher waited forever and guessed at rival processes.
                // CREATE_NEW preserves any existing per-match durable result.
                if(result.state!=Completion::rebuilt)failure_report(start->config,result.reason);
                code=result.state==Completion::rebuilt?0:2;break;
            }
            if(session.transient_&&GetTickCount64()<deadline){Sleep(2000);continue;}
            failure_report(start->config,session.reason_);code=4;break;
        }
    }
    catch(...){failure_report(start->config,"Unexpected exception before native worker completion");code=3;}
    start.reset();FreeLibraryAndExitThread(module,code);
}
inline bool start(HMODULE module,bool(*anti_cheat)()) {
    if(!configured||!config.enabled)return false;
    if(poisoned.load()||!server_pid.load()){
        failure_report(config,"Native bootstrap rejected: provider pipe identity unavailable or changed");return false;
    }
    if(started.exchange(true))return false;
    HMODULE retained{};
    if(!GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS,reinterpret_cast<LPCWSTR>(&worker),&retained)||retained!=module) {
        const auto error=GetLastError();
        if(retained)FreeLibrary(retained);
        failure_report(config,"Native bootstrap module retention failed (Win32 "+std::to_string(error)+")");return false;
    }
    auto context=std::make_unique<Start>();context->config=config;context->module=retained;context->anti_cheat=anti_cheat;
    Handle thread(CreateThread(nullptr,0,&worker,context.get(),0,nullptr));
    if(!thread){const auto error=GetLastError();FreeLibrary(retained);
        failure_report(config,"Native bootstrap thread creation failed (Win32 "+std::to_string(error)+")");return false;}
    context.release();return true;
}
} // namespace outfit_match_v2::bridge
