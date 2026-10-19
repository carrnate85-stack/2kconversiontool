#pragma once
#include "wnba_blacktop_category_win.hpp"
#include "wnba_priority_v2_win.hpp"
#include "wnba_blacktop_roster_win.hpp"
#include "wnba_blacktop_coordinator_core.hpp"

// This worker is reachable only after the explicit two-entry source trial has
// published. The separate exact-session category permit is mandatory. Ordinary
// Freestyle Play never enters this path and no product toggle enables it yet.
namespace wnba_blacktop_category::bridge {
inline void progress(const outfit_match_v2::bridge::Config& c,const char* state,const char* detail) {
    using namespace outfit_match_v2;
    PinnedDirectory directory;if(!directory.open(c.root+L"\\Logs\\game-session"))return;
    const auto path=directory.path()+L"\\wnba-blacktop-categories-"+std::to_wstring(GetCurrentProcessId())+L".progress";
    Handle f(CreateFileW(path.c_str(),FILE_APPEND_DATA|GENERIC_READ,FILE_SHARE_READ,nullptr,OPEN_ALWAYS,
        FILE_FLAG_OPEN_REPARSE_POINT|FILE_FLAG_WRITE_THROUGH,nullptr));
    StableId id{};FILE_STANDARD_INFO size{};
    if(!f || !file_id(f.value,false,id) ||
       !GetFileInformationByHandleEx(f.value,FileStandardInfo,&size,sizeof size) || size.EndOfFile.QuadPart>65536)return;
    const auto text="TICK_MS="+std::to_string(GetTickCount64())+" SESSION="+c.session+
        " STATE="+state+" DETAIL="+std::string(detail).substr(0,240)+"\n";
    DWORD wrote{};WriteFile(f.value,text.data(),DWORD(text.size()),&wrote,nullptr);FlushFileBuffers(f.value);
}
inline void run(const outfit_match_v2::bridge::Config& c,outfit_match_v2::Authority& authority,
                wnba_priority_v2::NativeHost& source) {
    const auto token=c.root+L"\\Data\\wnba-priority-v2\\blacktop-categories-"+
        std::wstring(c.session.begin(),c.session.end())+L".bin";
    progress(c,"WAITING_FOR_APPROVAL","Waiting for WNBA Blacktop permission from this Play session");
    const auto deadline=GetTickCount64()+600000;
    while(authority.safe() && GetTickCount64()<deadline &&
          GetFileAttributesW(token.c_str())==INVALID_FILE_ATTRIBUTES)Sleep(250);
    if(!authority.safe() || GetTickCount64()>=deadline) {
        progress(c,"HELD","Category test approval expired or owned session ended; no category changes");return;
    }
    NativeHost<wnba_priority_v2::NativeHost> host;
    if(!host.open(authority,source)) {
        progress(c,"HELD",host.reason());return;
    }
    wnba_blacktop_roster::NativeHost<decltype(host)> roster_host;
    if(!roster_host.open(host)) {
        progress(c,"HELD",roster_host.reason());return;
    }
    struct KeepCategoryUntilProcessExit {
        ~KeepCategoryUntilProcessExit() { for(;;)Sleep(1000); }
    } lifetime;
    const auto& b=authority.binding();
    wnba_blacktop_coordinator::Coordinator coordinator(
        {b.game_pid,b.game_birth,b.session},authority.base(),host.label_rva());
    const auto resolved="Sorted Euro category record RVA="+std::to_string(host.label_rva());
    progress(c,"LABEL_RESOLVED",resolved.c_str());
    progress(c,"WAITING","WNBA Blacktop is waiting for player selection and loaded teams");
    // No scope exit after executable memory or route publication. Code/unwind,
    // source, journal and module pins remain valid until the process exits.
    // Every uncertain result becomes a permanent hold; never retry or adopt.
    bool stock_reported=false;
    wnba_blacktop_coordinator::LimitNotice limit_notice;
    for(;;) {
        try {
            const auto input=host.sample_input();
            const auto result=coordinator.tick(input,roster_host,host);
            if(result.roster_result==wnba_blacktop_roster::Result::armed) {
                const auto detail=roster_host.event_detail(result.roster_reason);
                progress(c,"ROSTER_ARMED",detail.c_str());
            } else if(result.roster_result==wnba_blacktop_roster::Result::restored) {
                const auto detail=roster_host.event_detail(result.roster_reason);
                progress(c,"ROSTER_RESTORED",detail.c_str());
            }
            if(result.held) {
                progress(c,"HELD",result.reason);for(;;)Sleep(1000);
            }
            if(input.stable && input.current==0x92DF06DE && input.alternate==input.current &&
               input.stage==3 && input.observed_team_count==370 && !stock_reported) {
                progress(c,"WAITING_ROSTER","Blacktop still has the 370-team stock roster; the WNBA source has not been consumed");
                stock_reported=true;
            }
            if(result.category_result==Result::published)progress(c,"PUBLISHED",result.category_reason);
            else if(result.category_result==Result::restored)progress(c,"RESTORED",result.category_reason);
            // Report once after clean ownership, but keep ticking for normal
            // lifecycle/conditional cleanup. This is never a mutation permit.
            if(limit_notice.observe(result,coordinator.roster_owned(),coordinator.category_owned()))
                progress(c,"LIMIT_REACHED",limit_notice.detail());
        } catch(...) {
            progress(c,"HELD","Unexpected category observation failure; all resources retained, no retry");
            for(;;)Sleep(1000);
        }
        Sleep(100);
    }
}
} // namespace wnba_blacktop_category::bridge
