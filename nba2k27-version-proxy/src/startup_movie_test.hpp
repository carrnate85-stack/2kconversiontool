#pragma once
#include <Windows.h>
#include <array>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <TlHelp32.h>

// Isolated build only. One breakpoint on a constructor-owned object, no
// polling writes to a menu object and no permanent executable modification.
namespace startup_movie_test {
inline std::uintptr_t site=0,expected_vtable=0;
inline volatile LONG hits=0,changed=0,restored=0;
inline volatile LONG applied=0;
inline bool enable_movie=false;
inline DWORD site_protection=0;
inline HANDLE fault_file=INVALID_HANDLE_VALUE;
inline volatile LONG faults=0;
struct FaultRecord {
 std::uint32_t magic=0x32464d53,version=2,code=0,thread=0;
 std::uint64_t rip=0,module=0,access=0,address=0;
 std::array<std::uint64_t,8> registers{};
 std::array<std::uint64_t,16> stack_code{};
};
inline bool read(std::uintptr_t p,void *out,SIZE_T n) {
 SIZE_T got=0;return p>=0x10000&&ReadProcessMemory(GetCurrentProcess(),reinterpret_cast<void*>(p),out,n,&got)&&got==n;
}
inline bool bytes(std::uintptr_t p,const void *expected,SIZE_T n) {
 std::array<unsigned char,32> b{};return n<=b.size()&&read(p,b.data(),n)&&!std::memcmp(b.data(),expected,n);
}
inline bool executable(DWORD p) {
 return !(p&(PAGE_GUARD|PAGE_NOACCESS))&&(p&(PAGE_EXECUTE|PAGE_EXECUTE_READ|PAGE_EXECUTE_READWRITE|PAGE_EXECUTE_WRITECOPY));
}
inline LONG CALLBACK capture_fault(EXCEPTION_POINTERS *e) {
 const auto code=e->ExceptionRecord->ExceptionCode;
 if(fault_file==INVALID_HANDLE_VALUE||(code!=EXCEPTION_ACCESS_VIOLATION&&code!=EXCEPTION_ILLEGAL_INSTRUCTION&&code!=EXCEPTION_IN_PAGE_ERROR&&code!=EXCEPTION_STACK_OVERFLOW&&code!=0xc0000409u))return EXCEPTION_CONTINUE_SEARCH;
 if(InterlockedIncrement(&faults)>8)return EXCEPTION_CONTINUE_SEARCH;
 FaultRecord r;r.code=code;r.thread=GetCurrentThreadId();r.rip=e->ContextRecord->Rip;
 MEMORY_BASIC_INFORMATION mbi{};
 if(VirtualQuery(reinterpret_cast<void*>(r.rip),&mbi,sizeof(mbi)))r.module=reinterpret_cast<std::uint64_t>(mbi.AllocationBase);
 if(e->ExceptionRecord->NumberParameters>=2) {r.access=e->ExceptionRecord->ExceptionInformation[0];r.address=e->ExceptionRecord->ExceptionInformation[1];}
 const auto &c=*e->ContextRecord;r.registers={c.Rax,c.Rcx,c.Rdx,c.Rbx,c.Rsi,c.Rdi,c.R8,c.R9};
 // Keep only executable-address candidates, never raw stack/heap content.
 std::size_t kept=0;
 for(std::size_t i=0;i<64&&kept<r.stack_code.size();++i) {
  std::uint64_t candidate=0;
  if(read(c.Rsp+i*8,&candidate,8)&&candidate>=0x10000&&VirtualQuery(reinterpret_cast<void*>(candidate),&mbi,sizeof(mbi))&&executable(mbi.Protect))r.stack_code[kept++]=candidate;
 }
 DWORD written=0;WriteFile(fault_file,&r,sizeof(r),&written,nullptr);FlushFileBuffers(fault_file);
 // First-chance evidence only. Never suppress, recover, or alter a fault.
 return EXCEPTION_CONTINUE_SEARCH;
}
inline bool replace(unsigned char from,unsigned char to) {
 DWORD old=0;if(!VirtualProtect(reinterpret_cast<void*>(site),1,PAGE_EXECUTE_READWRITE,&old))return false;
 auto expected=from;
 const bool ok=__atomic_compare_exchange_n(reinterpret_cast<unsigned char*>(site),&expected,to,false,__ATOMIC_SEQ_CST,__ATOMIC_SEQ_CST);
 DWORD ignored=0;const bool protection=VirtualProtect(reinterpret_cast<void*>(site),1,site_protection,&ignored)!=FALSE;
 const bool flushed=FlushInstructionCache(GetCurrentProcess(),reinterpret_cast<void*>(site),1)!=FALSE;
 return ok&&protection&&flushed;
}
inline LONG CALLBACK exception(EXCEPTION_POINTERS *e) {
 if(e->ExceptionRecord->ExceptionCode!=EXCEPTION_BREAKPOINT||reinterpret_cast<std::uintptr_t>(e->ExceptionRecord->ExceptionAddress)!=site)return EXCEPTION_CONTINUE_SEARCH;
 if(InterlockedIncrement(&hits)!=1)return EXCEPTION_CONTINUE_SEARCH;
 const unsigned char original[]{0xc6,0x86,0xa0,0x0e,0,0,1};
 if(!replace(0xcc,0xc6)&&!bytes(site,original,sizeof(original)))return EXCEPTION_CONTINUE_SEARCH;
 InterlockedExchange(&restored,1);
 // RSI is owned by this exact constructor at this instruction.
 auto obj=static_cast<std::uintptr_t>(e->ContextRecord->Rsi);
 std::uintptr_t vt=0;std::array<unsigned char,16> fields{};
 const std::array<unsigned char,16> initial{0,0,0,0,0,0,0x80,0x3f,0,0,0,0,0,0,0,0};
 unsigned char value=enable_movie?0:1;SIZE_T done=0;
 if(read(obj,&vt,8)&&vt==expected_vtable&&read(obj+0xe90,fields.data(),fields.size())&&fields==initial&&
    WriteProcessMemory(GetCurrentProcess(),reinterpret_cast<void*>(obj+0xea0),&value,1,&done)&&done==1) {
  InterlockedExchange(&changed,enable_movie?1:0);
  InterlockedExchange(&applied,1);
  e->ContextRecord->Rip=site+7; // control preserves original MOV value of one
 } else e->ContextRecord->Rip=site; // execute the restored original MOV
 return EXCEPTION_CONTINUE_EXECUTION;
}
inline DWORD worker(bool (*anti_cheat)()) {
 wchar_t exe[32768]{},logpath[32768]{};
 if(!GetModuleFileNameW(nullptr,exe,32768))return 1;
 const auto leaf=wcsrchr(exe,L'\\');if(!leaf||_wcsicmp(leaf+1,L"NBA2K27.exe"))return 0;
 // Explicit test-only sidecar supports Steam's existing process (which does
 // not inherit environment variables from this test runner).
 wchar_t token[64]{};
 wchar_t config[32768]{};wcsncpy_s(config,exe,static_cast<std::size_t>(leaf-exe+1));
 if(wcscat_s(config,L"startup-movie-test.cfg"))return 1;
 FILE *cfg=_wfopen(config,L"rt, ccs=UTF-8");if(!cfg)return 0;
 const bool configured=fgetws(token,64,cfg)&&fgetws(logpath,32768,cfg);fclose(cfg);
 token[wcscspn(token,L"\r\n")]=0;logpath[wcscspn(logpath,L"\r\n")]=0;
 if(!configured||!logpath[0])return 0;
 if(!wcscmp(token,L"LOCAL_NATIVE_CLIP_V1"))enable_movie=true;
 else if(!wcscmp(token,L"LOCAL_NATIVE_CONTROL_V3"))enable_movie=false;
 else return 0;
 FILE *log=_wfopen(logpath,L"wx");if(!log)return 1;
 auto report=[&](const char *state){fprintf(log,"STATE=%s\nHOOK_HITS=%ld\nFLAG_CHANGED=%ld\nINSTRUCTION_RESTORED=%ld\n",state,hits,changed,restored);fflush(log);};
 fprintf(log,"SCHEMA=startup-native-movie-test-v1\nPID=%lu\n",GetCurrentProcessId());fflush(log);
 fprintf(log,"MODE=%s\n",enable_movie?"PLAYBACK":"UNCHANGED_FLAG_CONTROL");fflush(log);
 const auto base=reinterpret_cast<std::uintptr_t>(GetModuleHandleW(nullptr));
 IMAGE_DOS_HEADER dos{};IMAGE_NT_HEADERS64 nt{};
 const unsigned char anchor[]{0xc6,0x86,0xa0,0x0e,0,0,1};
 const unsigned char caller[]{0x48,0x8d,0x05,0x28,0x53,0xfe,0x03};
 site=base+0x304f1a7;expected_vtable=base+0x77f60f8;
 MEMORY_BASIC_INFORMATION mbi{};std::array<unsigned char,1984> menus{};
 if(anti_cheat()||!read(base,&dos,sizeof(dos))||dos.e_magic!=IMAGE_DOS_SIGNATURE||dos.e_lfanew<=0||dos.e_lfanew>=0x1000||
    !read(base+dos.e_lfanew,&nt,sizeof(nt))||nt.Signature!=IMAGE_NT_SIGNATURE||nt.FileHeader.TimeDateStamp!=0x6a974d13||nt.OptionalHeader.SizeOfImage!=0x368c3000||
    !bytes(site,anchor,sizeof(anchor))||!bytes(base+0x386514b,caller,sizeof(caller))||
    !read(base+0xac73660,menus.data(),menus.size())||
    !VirtualQuery(reinterpret_cast<void*>(site),&mbi,sizeof(mbi))) {report("REJECTED");fclose(log);return 1;}
 for(std::size_t i=0;i<menus.size();i+=64) {unsigned id=0;memcpy(&id,menus.data()+i,4);if(id){report("TOO_LATE");fclose(log);return 1;}}
 site_protection=mbi.Protect;
 wchar_t faultpath[32768]{};wcscpy_s(faultpath,logpath);
 if(wcscat_s(faultpath,L".faults.bin")){report("FAULT_PATH_FAILED");fclose(log);return 1;}
 fault_file=CreateFileW(faultpath,GENERIC_WRITE,FILE_SHARE_READ,nullptr,CREATE_NEW,FILE_ATTRIBUTE_NORMAL,nullptr);
 if(fault_file==INVALID_HANDLE_VALUE){report("FAULT_LOG_FAILED");fclose(log);return 1;}
 if(!AddVectoredExceptionHandler(1,capture_fault)){CloseHandle(fault_file);fault_file=INVALID_HANDLE_VALUE;report("FAULT_HANDLER_FAILED");fclose(log);return 1;}
 HANDLE modules=CreateToolhelp32Snapshot(TH32CS_SNAPMODULE,GetCurrentProcessId());
 if(modules!=INVALID_HANDLE_VALUE){MODULEENTRY32W entry{};entry.dwSize=sizeof(entry);if(Module32FirstW(modules,&entry)){do{
  fprintf(log,"MODULE=%ls BASE=%llx SIZE=%lx\n",entry.szModule,static_cast<unsigned long long>(reinterpret_cast<std::uintptr_t>(entry.modBaseAddr)),entry.modBaseSize);
 }while(Module32NextW(modules,&entry));}CloseHandle(modules);fflush(log);}
 auto handler=AddVectoredExceptionHandler(1,exception);
 if(!handler){report("HANDLER_FAILED");fclose(log);return 1;}
 if(!replace(0xc6,0xcc)) {report("ARM_FAILED_CLOSE_GAME");fclose(log);return 1;}
 report("ARMED");const auto deadline=GetTickCount64()+60000;
 while(!InterlockedCompareExchange(&hits,0,0)&&GetTickCount64()<deadline&&!anti_cheat())Sleep(10);
 if(!InterlockedCompareExchange(&hits,0,0)) {
  // Leave the passive handler resident: a thread may already have fetched INT3.
  if(bytes(site,anchor,sizeof(anchor)))InterlockedExchange(&restored,1);
  else if(replace(0xcc,0xc6))InterlockedExchange(&restored,1);
  report("NOT_OBSERVED_CLOSE_GAME");
 } else {
  Sleep(100);report(applied&&restored?(enable_movie?"ATTEMPTED_CHECK_VIDEO":"CONTROL_COMPLETED"):"REJECTED_CLOSE_GAME");
 }
 // DLL and exception handler stay resident for delayed exception/lifetime safety.
 // No interception remains once the original instruction has been restored.
 fclose(log);return 0;
}
}
