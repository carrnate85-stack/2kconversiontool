#pragma once
#include "outfit_match_v2_proxy_bridge.hpp"
#include "wnba_priority_v2_win.hpp"
#include "wnba_blacktop_category_proxy_bridge.hpp"
namespace wnba_priority_v2::bridge {
inline std::atomic<bool> started{false};
struct Start { outfit_match_v2::bridge::Config config; HMODULE module{}; bool(*anti_cheat)(){}; };
inline std::wstring approval_path(const outfit_match_v2::bridge::Config& c) {
    return c.root+L"\\Data\\wnba-priority-v2\\approval-"+std::wstring(c.session.begin(),c.session.end())+L".bin";
}
// Progress is separate from the immutable terminal result. Each bounded line
// includes the nonce so an old PID's trace can never be mistaken for this game.
// Default Play writes only metadata-watch stages; this never authorizes a call.
inline void progress(const outfit_match_v2::bridge::Config& c,const char* state,const std::string& detail) {
    using namespace outfit_match_v2;
    PinnedDirectory directory;if(!directory.open(c.root+L"\\Logs\\game-session"))return;
    const auto path=directory.path()+L"\\wnba-source-test-"+std::to_wstring(GetCurrentProcessId())+L".progress";
    Handle f(CreateFileW(path.c_str(),FILE_APPEND_DATA|GENERIC_READ,FILE_SHARE_READ,nullptr,OPEN_ALWAYS,
        FILE_FLAG_OPEN_REPARSE_POINT|FILE_FLAG_WRITE_THROUGH,nullptr));
    StableId id{};FILE_STANDARD_INFO size{};
    if(!f || !file_id(f.value,false,id) ||
        !GetFileInformationByHandleEx(f.value,FileStandardInfo,&size,sizeof size) || size.EndOfFile.QuadPart>65536) return;
    const std::string text="TICK_MS="+std::to_string(GetTickCount64())+" SESSION="+c.session+
        " STATE="+state+" DETAIL="+detail.substr(0,220)+"\n";
    DWORD wrote{};WriteFile(f.value,text.data(),DWORD(text.size()),&wrote,nullptr);FlushFileBuffers(f.value);
}
inline void report(const outfit_match_v2::bridge::Config& c,const char* state,const std::string& reason) {
    progress(c,state,reason);
    using namespace outfit_match_v2;
    PinnedDirectory directory;if(!directory.open(c.root+L"\\Logs\\game-session"))return;
    const auto path=directory.path()+L"\\wnba-source-test-"+std::to_wstring(GetCurrentProcessId())+L".log";
    Handle f(CreateFileW(path.c_str(),GENERIC_WRITE,FILE_SHARE_READ,nullptr,CREATE_NEW,
        FILE_FLAG_OPEN_REPARSE_POINT|FILE_FLAG_WRITE_THROUGH,nullptr));
    if(!f)return;
    const std::string text="SCHEMA=wnba-native-source-admission-test-v1\nSTATE="+std::string(state)+
        "\nPID="+std::to_string(GetCurrentProcessId())+"\nSESSION="+c.session+
        "\nREASON="+reason.substr(0,300)+"\nCATEGORY_WRITES=0\nGAME_CODE_HOOKS=0\nWNBA_GAMEPLAY_VERIFIED=0\n";
    DWORD wrote{};WriteFile(f.value,text.data(),DWORD(text.size()),&wrote,nullptr);FlushFileBuffers(f.value);
}
inline bool read_approval(const outfit_match_v2::bridge::Config& c,
                          const outfit_match_v2::TrustedBinding& b,outfit_match_v2::Handle& held,
                          bool& blacktop_source_trial) {
    using namespace outfit_match_v2;
    PinnedDirectory directory;if(!directory.open(c.root+L"\\Data\\wnba-priority-v2"))return false;
    held=Handle(CreateFileW(approval_path(c).c_str(),GENERIC_READ,FILE_SHARE_READ,nullptr,OPEN_EXISTING,
        FILE_FLAG_OPEN_REPARSE_POINT,nullptr));
    StableId id{};std::vector<unsigned char> bytes;
    blacktop_source_trial=false;
    if(!held || !file_id(held.value,false,id) || !read_file(held.value,72,bytes) || !directory.unchanged()) return false;
    if(approval_matches(bytes,b.game_pid,b.game_birth,b.session)) return true;
    blacktop_source_trial=blacktop_approval_matches(bytes,b.game_pid,b.game_birth,b.session);
    return blacktop_source_trial;
}
inline DWORD run(Start* start) {
    using namespace outfit_match_v2;
    DWORD code=1;
    try {
        progress(start->config,"WATCHER_STARTED","Awaiting exact owned dev Play lease and permit; no WNBA native calls");
        outfit_match_v2::bridge::Session lease;
        if(!lease.open(start->config,start->anti_cheat)) {
            report(start->config,"REFUSED",lease.reason_);
        } else {
            const auto& binding=lease.native_binding();Handle approval;
            progress(start->config,"WAITING_FOR_APPROVAL","Exact dev lease owned; waiting for matching per-session approval; no WNBA native calls");
            // Home -> explicit approval works after ordinary DLL bootstrap.
            // Until approval exists this loop reads only the exact lease/game
            // identity and fixed token metadata. No Authority/profile hash or
            // native game function is opened/called here.
            const auto approval_deadline=GetTickCount64()+600000;
            auto observed_game=retain_current_process();
            while(observed_game && live(observed_game.value) &&
                  binding.session_owned(binding.context) &&
                  WaitForSingleObject(binding.cancellation,0)==WAIT_TIMEOUT &&
                  GetTickCount64()<approval_deadline &&
                  GetFileAttributesW(approval_path(start->config).c_str())==INVALID_FILE_ATTRIBUTES) Sleep(1000);
            if(!observed_game || !live(observed_game.value) || !binding.session_owned(binding.context) ||
                WaitForSingleObject(binding.cancellation,0)!=WAIT_TIMEOUT || GetTickCount64()>=approval_deadline) {
                return 0;
            }
            bool blacktop_source_trial{};
            if(!read_approval(start->config,binding,approval,blacktop_source_trial)) {
                report(start->config,"REFUSED","Exact-session diagnostic approval missing or invalid; no native calls");
            } else {
                progress(start->config,"APPROVAL_MATCHED","Checking native authority/profile; remain at Home or Quick Play");
                Authority authority;
                if(!authority.open(binding)) {
                    report(start->config,"REFUSED",authority.reason());
                } else {
                    progress(start->config,"WAITING_FOR_HOME","Authority accepted; remain at Home or Quick Play until REGISTERED before entering Freestyle");
                    const auto deadline=GetTickCount64()+120000; bool published=false;
                    while(authority.safe() && GetTickCount64()<deadline) {
                        Context before{},after{};
                        if(!context(authority.base(),before) ||
                            !inactive_frontend({before.mode,before.other,before.stage,before.active,before.kind}) ||
                            !context(authority.base(),after) || before!=after) { Sleep(100);continue; }
                        NativeHost host;
                        const auto& profile=pack_profile(blacktop_source_trial);
                        if(!host.open(authority,start->config.root+L"\\Data\\wnba-priority-v2\\"+profile.folder,true,blacktop_source_trial)) {
                            report(start->config,"REFUSED","Pinned private pack, native profile, or inactive-menu gate rejected");break;
                        }
                        Session publication;
                        Admission a{true,true,true,true,true,true,binding.game_birth};
                        // Scope-unwind protection must precede the native API.
                        // Once an admission attempt can create native entries,
                        // only process exit may release the host/file/module
                        // pins, including errors and C++ exception unwinding.
                        struct KeepUntilProcessExit {
                            ~KeepUntilProcessExit() { for(;;) Sleep(1000); }
                        } lifetime;
                        progress(start->config,"ADMISSION_ATTEMPT","Stable inactive frontend and pinned pack accepted; validating empty priority under native mutex");
                        const auto result=publication.publish(a,host);
                        published=result==Result::published;
                        report(start->config,published?"REGISTERED":"REFUSED",
                            published?(blacktop_source_trial?
                                "Blacktop source trial registered a two-entry pack. Category availability is reported separately":
                                "Native priority WAD registered. Enter Freestyle for read-only roster verification; WNBA categories remain disabled"):
                            "Native priority registration refused or failed; no replacement of a foreign override was allowed");
                        // Even a refused native call may have created native-owned
                        // entries. Hold the pack and module until this game exits.
                        // No unregister, cancellation, or launcher-loss teardown.
                        // The game terminates its own threads at process exit.
                        // Do not turn an unknown handle wait/query failure into
                        // permission to release native-owned live resources.
                        if(published && blacktop_source_trial)
                            wnba_blacktop_category::bridge::run(start->config,authority,host);
                        for(;;) Sleep(1000);
                    }
                    if(!published && GetTickCount64()>=deadline)
                        report(start->config,"REFUSED","No stable inactive Home/Quick Play boundary within 120 seconds");
                }
            }
        }
    } catch(...) { report(start->config,"FAILED","Unexpected exception in diagnostic worker");code=3; }
    return code;
}
inline DWORD WINAPI worker(void* raw) {
    std::unique_ptr<Start> start(static_cast<Start*>(raw));const auto module=start->module;
    const DWORD code=run(start.get());
    start.reset();FreeLibraryAndExitThread(module,code);
}
inline bool start(HMODULE module,bool(*anti_cheat)()) {
    const auto& c=outfit_match_v2::bridge::config;
    if(!outfit_match_v2::bridge::configured || !c.enabled ||
        !outfit_match_v2::bridge::session_nonce(c.session)) return false;
    // Ordinary Play only starts a bounded metadata watcher. The exact approval
    // file is mandatory before Authority/profile hashing or any native call.
    if(started.exchange(true)) return false;
    HMODULE retained{};
    if(!GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS,reinterpret_cast<LPCWSTR>(&worker),&retained) || retained!=module) {
        report(c,"REFUSED","WNBA metadata watcher module retention failed before native authority");
        if(retained)FreeLibrary(retained);return false;
    }
    auto context=std::make_unique<Start>();context->config=c;context->module=retained;context->anti_cheat=anti_cheat;
    outfit_match_v2::Handle thread(CreateThread(nullptr,0,&worker,context.get(),0,nullptr));
    if(!thread){report(c,"REFUSED","WNBA metadata watcher thread creation failed before native authority");FreeLibrary(retained);return false;}
    context.release();return true;
}
} // namespace wnba_priority_v2::bridge
