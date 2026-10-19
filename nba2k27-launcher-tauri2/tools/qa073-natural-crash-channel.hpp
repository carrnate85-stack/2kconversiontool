#pragma once
// Offline diagnostic only. Never include in a production game/launcher build.
#define NOMINMAX
#include <windows.h>
#include <cstdint>
namespace natural_crash {
constexpr std::uint64_t magic=0x5141303733435241ull;
struct Channel {
 std::uint64_t signature;DWORD version,pid;std::uint64_t birth;
 volatile LONG claimed;DWORD tid;std::uint64_t pointers;
};
inline Channel* channel{};inline HANDLE request{},ack{};
inline std::uint64_t birth() noexcept {FILETIME a{},b{},c{},d{};if(!GetProcessTimes(GetCurrentProcess(),&a,&b,&c,&d))return 0;return(std::uint64_t(a.dwHighDateTime)<<32)|a.dwLowDateTime;}
inline HANDLE environment_handle(const wchar_t* key) noexcept {
 wchar_t value[32]{};const auto n=GetEnvironmentVariableW(key,value,32);if(!n||n>=32)return nullptr;
 std::uintptr_t number{};for(DWORD i=0;i<n;++i){const auto c=value[i];unsigned digit;if(c>=L'0'&&c<=L'9')digit=c-L'0';else if(c>=L'a'&&c<=L'f')digit=c-L'a'+10;else return nullptr;
 if(number>(UINTPTR_MAX-digit)/16)return nullptr;number=number*16+digit;}return reinterpret_cast<HANDLE>(number);
}
inline LONG WINAPI fatal(EXCEPTION_POINTERS* pointers) noexcept {
 auto* c=channel;if(!c||c->pid!=GetCurrentProcessId()||c->birth!=birth()||InterlockedCompareExchange(&c->claimed,1,0)!=0)return EXCEPTION_CONTINUE_SEARCH;
 c->tid=GetCurrentThreadId();c->pointers=reinterpret_cast<std::uintptr_t>(pointers);MemoryBarrier();
 if(SetEvent(request))WaitForSingleObject(ack,20000);
 return EXCEPTION_CONTINUE_SEARCH;
}
inline void __cdecl initialize() noexcept {
 const auto mapping=environment_handle(L"QA073_CRASH_MAPPING");if(!mapping)return;
 const auto ready=environment_handle(L"QA073_CRASH_READY");request=environment_handle(L"QA073_CRASH_REQUEST");ack=environment_handle(L"QA073_CRASH_ACK");
 if(!ready||!request||!ack)return;
 auto* c=static_cast<Channel*>(MapViewOfFile(mapping,FILE_MAP_READ|FILE_MAP_WRITE,0,0,sizeof(Channel)));if(!c)return;
 if(WaitForSingleObject(ready,5000)!=WAIT_OBJECT_0||c->signature!=magic||c->version!=1||c->pid!=GetCurrentProcessId()||!c->birth||c->birth!=birth()){UnmapViewOfFile(c);return;}
 channel=c;SetUnhandledExceptionFilter(fatal);
}
#ifdef QA073_NATURAL_CRASH_CRT
using Initializer=void(__cdecl*)();
__attribute__((section(".CRT$XCU"),used)) inline Initializer registration=initialize;
#endif
}
