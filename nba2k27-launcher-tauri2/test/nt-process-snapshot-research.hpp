#pragma once
// RESEARCH ONLY: no production include. Official NtQuerySystemInformation
// docs: https://learn.microsoft.com/windows/win32/api/winternl/nf-winternl-ntquerysysteminformation
#include "../native/outfit_match_v2_proxy_bridge.hpp"
#include <winternl.h>
#include <span>
#include <set>
#include <optional>
namespace nt_snapshot_research {
struct Row {DWORD pid{};std::wstring name;bool operator==(const Row&)const=default;};
static_assert(sizeof(void*)==8 && sizeof(wchar_t)==2);
static_assert(offsetof(SYSTEM_PROCESS_INFORMATION,NextEntryOffset)==0);
static_assert(offsetof(SYSTEM_PROCESS_INFORMATION,ImageName)==56);
static_assert(offsetof(SYSTEM_PROCESS_INFORMATION,UniqueProcessId)==80);
static_assert(sizeof(SYSTEM_PROCESS_INFORMATION)==256 && sizeof(SYSTEM_THREAD_INFORMATION)==80);
inline bool unicode(const std::wstring& s) {
 for(std::size_t n=0;n<s.size();++n) {
  const unsigned x=s[n];if(!x || x<32 || x==L'/' || x==L'\\' || x==L':')return false;
  if(x>=0xD800 && x<=0xDBFF) {if(++n==s.size() || s[n]<0xDC00 || s[n]>0xDFFF)return false;}
  else if(x>=0xDC00 && x<=0xDFFF)return false;
 }
 return true;
}
inline std::optional<std::vector<Row>> parse(std::span<const unsigned char> bytes,DWORD self) {
 if(bytes.size()<sizeof(SYSTEM_PROCESS_INFORMATION) || bytes.size()>16*1024*1024 || !self)return {};
 std::vector<Row> rows;std::set<DWORD> seen;std::size_t offset{};bool found_self{};
 const auto base=reinterpret_cast<std::uintptr_t>(bytes.data());
 for(unsigned count=0;count<65536;++count) {
  if(offset%8 || offset>bytes.size()-sizeof(SYSTEM_PROCESS_INFORMATION))return {};
  SYSTEM_PROCESS_INFORMATION p{};std::memcpy(&p,bytes.data()+offset,sizeof p);
  const auto next=std::size_t(p.NextEntryOffset);
  if(next && (next%8 || next<sizeof p || next>bytes.size()-offset))return {};
  const auto record_bytes=next?next:bytes.size()-offset;
  if(p.NumberOfThreads>65536 || p.NumberOfThreads>(record_bytes-sizeof p)/sizeof(SYSTEM_THREAD_INFORMATION))return {};
  const auto raw_pid=reinterpret_cast<std::uintptr_t>(p.UniqueProcessId);
  if(raw_pid>MAXDWORD || !seen.insert(DWORD(raw_pid)).second)return {};
  Row row{DWORD(raw_pid),{}};found_self|=row.pid==self;
  const auto body=sizeof p+std::size_t(p.NumberOfThreads)*sizeof(SYSTEM_THREAD_INFORMATION);std::size_t consumed=body;
  const auto length=std::size_t(p.ImageName.Length),maximum=std::size_t(p.ImageName.MaximumLength);
  const auto name=reinterpret_cast<std::uintptr_t>(p.ImageName.Buffer);
  if(length%2 || maximum%2 || length>maximum || length>518 || maximum>520)return {};
  if(!length) {if(row.pid!=0 || name || maximum)return {};}
  else {
   // Validate full declared UTF16 capacity inside this record before touching
   // any name bytes, not merely inside a reusable allocation's larger tail.
   if(!name || name%2 || name<base+offset+body || name>base+offset+record_bytes || maximum>base+offset+record_bytes-name)return {};
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
inline std::optional<std::vector<Row>> fresh_nt(DWORD self) {
 const auto dll=GetModuleHandleW(L"ntdll.dll");
 const auto query=dll?reinterpret_cast<Query>(GetProcAddress(dll,"NtQuerySystemInformation")):nullptr;
 if(!query)return {};
 std::vector<unsigned char> buffer(256*1024);
 for(unsigned attempt=0;attempt<4;++attempt) {
  ULONG returned{};const auto status=query(SystemProcessInformation,buffer.data(),ULONG(buffer.size()),&returned);
  if(status==0) {if(returned>buffer.size() || !returned)return {};return parse(std::span(buffer).first(returned),self);}
  if(status!=NTSTATUS(0xC0000004) && status!=NTSTATUS(0xC0000023))return {};
  if(returned<=buffer.size() || returned>16*1024*1024-65536)return {};
  buffer.resize(std::size_t(returned)+65536);
 }
 return {};
}
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
inline std::optional<std::vector<Row>> fresh_toolhelp() {
 outfit_match_v2::Handle snapshot(CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS,0));
 if(!snapshot)return {};PROCESSENTRY32W entry{};entry.dwSize=sizeof entry;
 if(!Process32FirstW(snapshot.value,&entry))return {};
 std::vector<Row> rows;
 do {rows.push_back({entry.th32ProcessID,entry.th32ProcessID?std::wstring(entry.szExeFile):std::wstring{}});}while(Process32NextW(snapshot.value,&entry));
 if(GetLastError()!=ERROR_NO_MORE_FILES)return {};
 std::sort(rows.begin(),rows.end(),[](const Row& a,const Row& b){return a.pid<b.pid;});return rows;
}
inline bool blocked(const std::vector<Row>& rows,DWORD self,DWORD worker,DWORD parent) {
 using namespace outfit_match_v2;
 for(const auto& row:rows) {
  const auto& name=row.name;
  if(name.size()>=13 && CompareStringOrdinal(name.data(),13,L"EasyAntiCheat",13,TRUE)==CSTR_EQUAL)return true;
  if(equal_path(name,L"NBA2K27.exe") && row.pid!=self)return true;
  if(row.pid!=worker && row.pid!=parent && bridge::launcher_name(name))return true;
 }
 return false;
}
inline bool equivalent(const std::vector<Row>& a,const std::vector<Row>& b) {
 if(a.size()!=b.size())return false;
 for(std::size_t n=0;n<a.size();++n)if(a[n].pid!=b[n].pid || !outfit_match_v2::equal_path(a[n].name,b[n].name))return false;
 return true;
}
template<class Snapshot,class Fallback> bool guard_with(DWORD self,DWORD worker,DWORD parent,Snapshot snapshot,Fallback fallback) {
 try {if(auto rows=snapshot())return blocked(*rows,self,worker,parent);}catch(...){}
 try {return fallback();}catch(...){return true;}
}
inline bool candidate_basic_guard(DWORD self,DWORD worker,DWORD parent) {
 return guard_with(self,worker,parent,[&]{return fresh_basic(self);},[&]{return outfit_match_v2::bridge::toolhelp_protected_processes(worker,parent);});
}
inline bool candidate_guard(DWORD self,DWORD worker,DWORD parent) {
 try {if(auto rows=fresh_nt(self))return blocked(*rows,self,worker,parent);}catch(...){}
 // Compatibility/parser/query failures must never turn into a permissive
 // empty process list. Existing failclosed production Toolhelp stays fallback.
 return outfit_match_v2::bridge::toolhelp_protected_processes(worker,parent);
}
}
