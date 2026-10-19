#pragma once
#include <Windows.h>
#include <array>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <string>
#include "character_queue_noop_policy.hpp"
#ifdef NBA2K27_CHARACTER_QUEUE_THREAD_AUDIT
#include "character_queue_thread_observer.hpp"
#endif

// Experimental builds only. No provider registration or code patches.
// The separately authorized THREAD_AUDIT variant adds one private request callback.
namespace character_queue_noop {
constexpr std::uintptr_t kQueueRva=0xA37C168, kQueueVtable=0x68507A8;
constexpr std::uintptr_t kNodeVtable=0x67F8DF8, kSubmitRva=0x5F9FE0;
constexpr char kProducerHex[]=
"5541565657534883ec50488d6c245048c745f8feffffff4488c34889d74989ce488b15d121d809c644243800c644243001c744242808000000c74424208a060000b9300000004531c041b911d97fade89c5052044889c64885c074654889760848897610488d05aded1f064889064c89761848897e20885e28488d0d2821d809e82b85540448397608750648397610740c488b064889f1ff9098000000488b05f420d80948894608488d0de120d80948894e1048897010488b461048897008eb174c89761848897e20885e28488d0dd520d809e8d8845404488d0dc920d809e88c845404904883c4505b5f5e415e5dc3";
constexpr char kConsumerHex[]=
"555657534883ec38488d6c243048c74500feffffff4889ce4883c12048894df8e8eb8d5404488b7e184883c6084839f70f94c04885ff0f94c108c1755f0f1f00488b07488b5f104889f9ff9098000000488b47184885c07429c680c800000000440fb64728488b4f18488b5720c681c800000000488b41784883c178ff90b8000000488b074889f9ba01000000ff104839f374084889df4885db75a4488b4df8e82b8d5404904883c4385b5f5e5dc3";
constexpr char kDestructorHex[]=
"55564883ec28488d6c242048c74500feffffff4889ce488d05cb86ff05488901c5f857c0c5fc11410885d2740b4889f1c5f877e838e431044889f04883c4285e5dc5f877c3";

inline bool read(std::uintptr_t address, void *out, std::size_t size) {
  SIZE_T got=0;
  return address>=0x10000 && ReadProcessMemory(GetCurrentProcess(),
      reinterpret_cast<void *>(address),out,size,&got) && got==size;
}
template<class T> bool value(std::uintptr_t address,T *out) { return read(address,out,sizeof(T)); }
inline std::uintptr_t q(std::uintptr_t address) {
  std::uintptr_t result=0; value(address,&result); return result;
}
inline unsigned i(std::uintptr_t address) { unsigned result=0;value(address,&result);return result; }
inline bool match_hex(std::uintptr_t address,const char *hex) {
  std::array<unsigned char,512> bytes{};
  const auto count=std::strlen(hex)/2;
  if(count>bytes.size()||!read(address,bytes.data(),count))return false;
  auto digit=[](char c)->unsigned{return c>='0'&&c<='9'?unsigned(c-'0'):unsigned(c-'a'+10);};
  for(std::size_t n=0;n<count;++n)
    if(bytes[n]!=(digit(hex[2*n])*16+digit(hex[2*n+1])))return false;
  return true;
}
inline bool exact_build(std::uintptr_t base) {
  IMAGE_DOS_HEADER dos{};IMAGE_NT_HEADERS64 nt{};
  return value(base,&dos)&&dos.e_magic==IMAGE_DOS_SIGNATURE&&dos.e_lfanew>0&&dos.e_lfanew<0x1000&&
      value(base+dos.e_lfanew,&nt)&&nt.Signature==IMAGE_NT_SIGNATURE&&
      nt.FileHeader.Machine==IMAGE_FILE_MACHINE_AMD64&&
      nt.FileHeader.TimeDateStamp==0x6A974D13&&nt.OptionalHeader.SizeOfImage==0x368C3000;
}
inline bool write_log(const std::wstring &path,const char *text) {
  HANDLE file=CreateFileW(path.c_str(),GENERIC_WRITE,FILE_SHARE_READ|FILE_SHARE_DELETE,
                         nullptr,CREATE_ALWAYS,FILE_ATTRIBUTE_NORMAL,nullptr);
  if(file==INVALID_HANDLE_VALUE)return false;
  DWORD size=static_cast<DWORD>(std::strlen(text)),written=0;
  bool ok=WriteFile(file,text,size,&written,nullptr)&&written==size;
  FlushFileBuffers(file);CloseHandle(file);return ok;
}
inline bool in_match(std::uintptr_t base) {
  return q(base+0xAEB29D0)==base+0x78AABA8&&i(q(base+0xAAC79A0)+0xA04)==6;
}
inline bool queue_identity(std::uintptr_t base) {
  unsigned char initialized=0;
  return q(base+kQueueRva)==base+kQueueVtable&&
      value(base+kQueueRva+0x5A,&initialized)&&initialized==1&&
      q(base+kNodeVtable)==base+0x800ED0&&q(base+kNodeVtable+0x98)==base+0x60BAF0;
}
inline bool queue_empty(std::uintptr_t base) {
  const auto sentinel=base+kQueueRva+8;
  return q(base+kQueueRva+0x10)==sentinel&&q(base+kQueueRva+0x18)==sentinel;
}
struct CriticalGuard {
  CRITICAL_SECTION *lock; bool entered;
  explicit CriticalGuard(std::uintptr_t address):lock(reinterpret_cast<CRITICAL_SECTION *>(address)),
      entered(TryEnterCriticalSection(lock)!=FALSE){}
  ~CriticalGuard(){if(entered)LeaveCriticalSection(lock);}
  CriticalGuard(const CriticalGuard &)=delete;
};

inline DWORD worker(HMODULE, bool (*anti_cheat_present)()) {
  wchar_t exe[32768]{};
  if(!GetModuleFileNameW(nullptr,exe,32768))return 1;
  const wchar_t *leaf=wcsrchr(exe,L'\\');
  if(!leaf||_wcsicmp(leaf+1,L"NBA2K27.exe"))return 0; // Forwarding smoke test.
  const auto base=reinterpret_cast<std::uintptr_t>(GetModuleHandleW(nullptr));
  const auto folder=std::wstring(exe,static_cast<std::size_t>(leaf-exe+1));
  const auto pid=GetCurrentProcessId();
#ifdef NBA2K27_CHARACTER_QUEUE_THREAD_AUDIT
  constexpr auto prefix=L"nba2k27_character_queue_thread_";
  constexpr auto token_expected="NULL_TARGET_THREAD_V2";
#else
  constexpr auto prefix=L"nba2k27_character_queue_noop_";
  constexpr auto token_expected="NULL_TARGET_NOOP_V1";
#endif
  const auto report_path=folder+prefix+std::to_wstring(pid)+L".log";
  const auto trigger_path=folder+prefix+std::to_wstring(pid)+L".trigger";
  bool submitted=false,witnessed=false,dequeued=false;unsigned expected_main=0;
  auto report=[&](const char *state,const char *reason) {
    char text[2048]{};
#ifdef NBA2K27_CHARACTER_QUEUE_THREAD_AUDIT
    namespace obs=character_queue_thread_observer;
    std::snprintf(text,sizeof(text),
      "SCHEMA=blacktop-character-queue-thread-v2\r\nPID=%lu\r\nSTATE=%s\r\nREASON=%s\r\n"
      "NATIVE_REQUEST_CALLS=%u\r\nPACKET_WITNESSED=%s\r\nQUEUE_DRAIN_OBSERVED=%s\r\n"
      "TARGET_ATTACHED=NO\r\nCUSTOM_CALLBACK_CALLS=%u\r\nGAME_CODE_HOOKS=0\r\n"
      "OBJECT_HOOK=%s\r\nORIGINAL_DESTRUCTOR_RETURNED=%s\r\n"
      "PROVIDER_REGISTRATION=OFF\r\nOUTFIT_WRITES=0\r\nEXPECTED_MAIN_THREAD=%u\r\n"
      "OBSERVED_THREAD=%u\r\nTHREAD_MATCH=%s\r\n"
      "NOTE=Temporary one-request object hook only. Proxy code stays loaded until game exit for lifetime safety. No player rebuild was tested.\r\n",
      pid,state,reason,unsigned(submitted),witnessed?"YES":"NO",dequeued?"YES":"NO",
      obs::calls.load(),obs::hook_state(),obs::returned.load()?"YES":"NO",expected_main,
      obs::thread_id.load(),obs::completed(expected_main)?"YES":"NO");
#else
    std::snprintf(text,sizeof(text),
      "SCHEMA=blacktop-character-queue-noop-v1\r\nPID=%lu\r\nSTATE=%s\r\nREASON=%s\r\n"
      "NATIVE_REQUEST_CALLS=%u\r\nPACKET_WITNESSED=%s\r\nQUEUE_DRAIN_OBSERVED=%s\r\n"
      "TARGET_ATTACHED=NO\r\nCUSTOM_CALLBACKS=0\r\nGAME_CODE_HOOKS=0\r\nPROVIDER_REGISTRATION=OFF\r\n"
      "OUTFIT_WRITES=0\r\nEXPECTED_MAIN_THREAD=%u\r\nCONSUMER_THREAD_OBSERVED=NO\r\n"
      "NOTE=Dequeuing is not proof of safe outfit replacement. Queue locks and native request allocation are the only test mutations.\r\n",
      pid,state,reason,unsigned(submitted),witnessed?"YES":"NO",dequeued?"YES":"NO",expected_main);
#endif
    return write_log(report_path,text);
  };
  if(!exact_build(base)||anti_cheat_present()) {report("FAILED","Build or anti-cheat guard");return 2;}
  if(!report("ARMED","Waiting for manual trigger; no request submitted"))return 3;
  bool triggered=false;
  for(unsigned n=0;n<1800;++n) {
    HANDLE file=CreateFileW(trigger_path.c_str(),GENERIC_READ,FILE_SHARE_READ,nullptr,OPEN_EXISTING,0,nullptr);
    if(file!=INVALID_HANDLE_VALUE) {
      char token[64]{};DWORD got=0;
      const bool ok=ReadFile(file,token,sizeof(token)-1,&got,nullptr)!=FALSE;
      CloseHandle(file);
      triggered=ok&&got==std::strlen(token_expected)&&std::memcmp(token,token_expected,got)==0;
      if(!triggered){report("FAILED","Invalid trigger token");return 4;}
      break;
    }
    Sleep(500);
  }
  if(!triggered){report("EXPIRED","No trigger; no request submitted");return 0;}
  // User-provided PID-scoped trigger is single-use. Removal failure fails closed.
  if(!DeleteFileW(trigger_path.c_str())){report("FAILED","Could not consume trigger");return 5;}
  expected_main=i(base+0x89986D0);
  Preconditions pre{};
  pre.exact_build=exact_build(base);
  pre.native_code=match_hex(base+kSubmitRva,kProducerHex)&&match_hex(base+0x5F9780,kConsumerHex)&&
      match_hex(base+0x800ED0,kDestructorHex)&&match_hex(base+0x824ACF,"488d0d3276b509e85549ddff488d0d8676b509e8994cddff");
  pre.anti_cheat_absent=!anti_cheat_present();pre.in_blacktop=in_match(base);
  pre.worker_thread=expected_main&&expected_main!=GetCurrentThreadId();
  pre.queue_class=queue_identity(base);pre.lock_initialized=pre.queue_class;
  std::uintptr_t heap_first=0;pre.heap_ready=value(q(base+0xA37C1D8),&heap_first)&&heap_first!=0;
  pre.queue_empty=queue_empty(base);
  if(!may_submit(pre)){report("FAILED","Preconditions rejected; no request submitted");return 6;}
#ifdef NBA2K27_CHARACTER_QUEUE_THREAD_AUDIT
  std::array<std::uintptr_t,21> private_table{};
  if(!read(base+kNodeVtable-8,private_table.data(),sizeof(private_table))||
      private_table[1]!=base+0x800ED0||private_table[20]!=base+0x60BAF0) {
    report("FAILED","Stock request table did not verify; no request submitted");return 12;
  }
  for(std::size_t n=1;n<private_table.size();++n) {
    MEMORY_BASIC_INFORMATION page{};
    if(private_table[n]<base||private_table[n]>=base+0x368C3000||
        !VirtualQuery(reinterpret_cast<void *>(private_table[n]),&page,sizeof(page))||
        page.State!=MEM_COMMIT||(page.Protect&PAGE_GUARD)||
        !(page.Protect&(PAGE_EXECUTE|PAGE_EXECUTE_READ|PAGE_EXECUTE_READWRITE|PAGE_EXECUTE_WRITECOPY))) {
      report("FAILED","Request table has unexpected executable targets; no request submitted");return 13;
    }
  }
  if(!character_queue_thread_observer::pin_callback_module()) {
    report("FAILED","Could not secure callback lifetime; no request submitted");return 14;
  }
#endif
  if(!report("CHECKING","Validated native no-op path; acquiring initialized queue lock"))return 7;
  const char *failure=nullptr;
  {
    CriticalGuard guard(base+kQueueRva+0x20);
    if(!guard.entered)failure="Queue busy; no submission";
    else if(!queue_identity(base)||!queue_empty(base)||!in_match(base)||anti_cheat_present()||
            i(base+0x89986D0)!=expected_main||i(base+kQueueRva+0x54)!=0)
      failure="Preconditions changed under queue lock; no submission";
    else {
      using Submit=void (*)(void *,void *,bool);
      // Outer Windows critical-section acquisition prevents the game consuming
      // the packet until we verify it. Native Enter/Leave recurse on the same lock.
      submitted=true;
      reinterpret_cast<Submit>(base+kSubmitRva)(nullptr,nullptr,false);
      const auto sentinel=base+kQueueRva+8;
      const auto node=q(base+kQueueRva+0x18);
      std::array<std::uintptr_t,6> packet{};
      unsigned recursion=~0u;
      witnessed=node!=sentinel&&node&&read(node,packet.data(),sizeof(packet))&&
          q(base+kQueueRva+0x10)==node&&packet[0]==base+kNodeVtable&&
          packet[1]==sentinel&&packet[2]==sentinel&&
          packet_is_noop(packet[3],packet[4],static_cast<unsigned char>(packet[5]))&&
          value(base+kQueueRva+0x54,&recursion)&&recursion==0;
      if(!witnessed)failure="Native call returned but exact null packet was not verified; no retry";
#ifdef NBA2K27_CHARACTER_QUEUE_THREAD_AUDIT
      else if(!character_queue_thread_observer::install(reinterpret_cast<void *>(node),base+kNodeVtable,private_table))
        failure="Private one-request hook was not installed; no retry";
#endif
    }
  }
  if(failure){report("FAILED",failure);return 8;}
  if(!report("QUEUED","One null-target stock request witnessed; waiting for native dequeue"))return 9;
  for(unsigned n=0;n<400;++n) {
    Sleep(25);
    if(!queue_identity(base)||!in_match(base)||anti_cheat_present()||i(base+0x89986D0)!=expected_main) {
      report("INCONCLUSIVE","Session changed; no queue cleanup or retry attempted");return 10;
    }
    {
      CriticalGuard guard(base+kQueueRva+0x20);
      if(guard.entered&&queue_identity(base)&&queue_empty(base)&&in_match(base)&&
          i(base+0x89986D0)==expected_main)dequeued=true;
    }
    if(may_report_dequeued(witnessed,true,dequeued,false)) {
#ifdef NBA2K27_CHARACTER_QUEUE_THREAD_AUDIT
      if(!character_queue_thread_observer::completed(expected_main)) {
        report("INCONCLUSIVE","Queue empty but callback/thread/cleanup evidence incomplete; close game");return 15;
      }
      report("PASS","One callback ran on the expected thread, restored its request vtable, and returned from stock destruction");return 0;
#else
      report("PASS","Null-target request was queued and subsequently removed by the native queue path");return 0;
#endif
    }
  }
  report("INCONCLUSIVE","Queue did not return empty within 10 seconds; no retry or forced removal");return 11;
}
} // namespace character_queue_noop
