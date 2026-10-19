#pragma once
// Fresh process names/PIDs only. Official MS SDK winternl.h defines
// SystemBasicProcessInformation=252 and SYSTEM_BASICPROCESS_INFORMATION.
// API/layout changes MUST fall back to the original fresh Toolhelp guard.
// https://learn.microsoft.com/windows/win32/api/winternl/nf-winternl-ntquerysysteminformation
#include <windows.h>
#include <winternl.h>
#include <algorithm>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <optional>
#include <set>
#include <span>
#include <string>
#include <vector>
namespace outfit_match_v2::process_snapshot {
struct Row {DWORD pid{};std::wstring name;bool operator==(const Row&)const=default;};
static_assert(sizeof(void*)==8 && sizeof(wchar_t)==2);
inline bool unicode(const std::wstring& s) {
 for(std::size_t n=0;n<s.size();++n) {
  const unsigned x=s[n];if(!x || x<32 || x==L'/' || x==L'\\' || x==L':')return false;
  if(x>=0xD800 && x<=0xDBFF) {if(++n==s.size() || s[n]<0xDC00 || s[n]>0xDFFF)return false;}
  else if(x>=0xDC00 && x<=0xDFFF)return false;
 }
 return true;
}
// Official Microsoft SDK 10.0.26100.0 winternl.h: BasicProcess=252,
// SYSTEM_BASICPROCESS_INFORMATION. Local llvm-mingw lacks this new enum/type.
struct Basic {ULONG NextEntryOffset;HANDLE UniqueProcessId;HANDLE InheritedFromUniqueProcessId;ULONG64 SequenceNumber;UNICODE_STRING ImageName;};
static_assert(sizeof(Basic)==48 && offsetof(Basic,UniqueProcessId)==8 && offsetof(Basic,SequenceNumber)==24 && offsetof(Basic,ImageName)==32);
inline std::optional<std::vector<Row>> parse_basic(std::span<const unsigned char> bytes,DWORD self) {
 if(bytes.size()<sizeof(Basic) || bytes.size()>16*1024*1024 || !self)return {};
 std::vector<Row> rows;std::set<DWORD> seen;std::size_t offset{};bool found_self{};
 const auto base=reinterpret_cast<std::uintptr_t>(bytes.data());
 for(unsigned count=0;count<65536;++count) {
  if(offset%8 || offset>bytes.size()-sizeof(Basic))return {};
  Basic p{};std::memcpy(&p,bytes.data()+offset,sizeof p);
  const auto next=std::size_t(p.NextEntryOffset);
  if(next && (next%8 || next<sizeof p || next>bytes.size()-offset))return {};
  const auto record_bytes=next?next:bytes.size()-offset;
  const auto raw_pid=reinterpret_cast<std::uintptr_t>(p.UniqueProcessId);
  if(reinterpret_cast<std::uintptr_t>(p.InheritedFromUniqueProcessId)>MAXDWORD || raw_pid>MAXDWORD || !seen.insert(DWORD(raw_pid)).second || (raw_pid && !p.SequenceNumber))return {};
  Row row{DWORD(raw_pid),{}};found_self|=row.pid==self;
  std::size_t consumed=sizeof p;
  const auto length=std::size_t(p.ImageName.Length),maximum=std::size_t(p.ImageName.MaximumLength);
  const auto name=reinterpret_cast<std::uintptr_t>(p.ImageName.Buffer);
  if(length%2 || maximum%2 || length>maximum || length>518 || maximum>520)return {};
  if(!length) {if(row.pid!=0 || name || maximum)return {};}
  else {
   if(!name || name%2 || name!=base+offset+sizeof p || name>base+offset+record_bytes || maximum>base+offset+record_bytes-name)return {};
   row.name.resize(length/2);std::memcpy(row.name.data(),reinterpret_cast<const void*>(name),length);
   if(!unicode(row.name))return {};
   consumed=name-base-offset+maximum;
  }
  if(consumed>record_bytes || record_bytes-consumed>7)return {};
  rows.push_back(std::move(row));
  if(!next) {
   if(!found_self)return {};
   std::sort(rows.begin(),rows.end(),[](const Row& a,const Row& b){return a.pid<b.pid;});return rows;
  }
  offset+=next;
 }
 return {};
}
using Query=NTSTATUS (NTAPI*)(SYSTEM_INFORMATION_CLASS,PVOID,ULONG,PULONG);
inline std::optional<std::vector<Row>> fresh_basic_with(DWORD self,Query query) {
 if(!query)return {};
 std::vector<unsigned char> buffer(64*1024);
 for(unsigned attempt=0;attempt<4;++attempt) {
  ULONG returned{};const auto status=query(static_cast<SYSTEM_INFORMATION_CLASS>(252),buffer.data(),ULONG(buffer.size()),&returned);
  if(status==0) {if(returned>buffer.size() || !returned)return {};return parse_basic(std::span(buffer).first(returned),self);}
  if(status!=NTSTATUS(0xC0000004) && status!=NTSTATUS(0xC0000023))return {};
  if(returned<=buffer.size() || returned>16*1024*1024-65536)return {};
  buffer.resize(std::size_t(returned)+65536);
 }
 return {};
}
inline std::optional<std::vector<Row>> fresh_basic(DWORD self) {
 const auto dll=GetModuleHandleW(L"ntdll.dll");
 const auto query=dll?reinterpret_cast<Query>(GetProcAddress(dll,"NtQuerySystemInformation")):nullptr;
 return fresh_basic_with(self,query);
}

// Snapshot and parser failures are uncertainty, never an empty allowed list.
// Neither rows, buffers nor allow/deny results survive this call. Every current
// authority check still performs a fresh OS query. No other PID handle opens.
template<class Snapshot,class Fallback,class Predicate>
bool guard_with(Snapshot snapshot,Fallback fallback,Predicate predicate) {
 try {if(auto rows=snapshot()) {for(const auto& row:*rows)if(predicate(row))return true;return false;}}catch(...){}
 try {return fallback();}catch(...){return true;}
}
} // namespace outfit_match_v2::process_snapshot
