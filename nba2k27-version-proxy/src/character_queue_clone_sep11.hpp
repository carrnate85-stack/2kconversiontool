#pragma once
#include <Windows.h>
#include <array>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <string>
// Private-copy diagnostic utilities only; no queue worker or submission API.
namespace character_queue_sep11 {
struct CriticalGuard {
  CRITICAL_SECTION *lock; bool entered;
  explicit CriticalGuard(std::uintptr_t address):lock(reinterpret_cast<CRITICAL_SECTION *>(address)),
      entered(TryEnterCriticalSection(lock)!=FALSE){}
  ~CriticalGuard(){if(entered)LeaveCriticalSection(lock);}
  CriticalGuard(const CriticalGuard &)=delete;
};
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
      nt.FileHeader.TimeDateStamp==0x6AA2013D&&nt.OptionalHeader.SizeOfImage==0x36558000;
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
  return q(base+0xAEB69D0)==base+0x78AEA48&&i(q(base+0xAACB9A0)+0xA04)==6;
}
}
