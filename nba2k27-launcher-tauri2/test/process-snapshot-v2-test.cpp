#include "../native/outfit_match_v2_proxy_bridge.hpp"
#include <cassert>
#include <cstdio>
#include <stdexcept>
using namespace outfit_match_v2::process_snapshot;
static bool blocked(const std::vector<Row>& rows,DWORD,DWORD worker,DWORD parent) {
 for(const auto& row:rows)if(outfit_match_v2::bridge::protected_entry(row.name,row.pid,worker,parent))return true;
 return false;
}
template<class Snapshot,class Fallback>static bool guard_with(DWORD,DWORD worker,DWORD parent,Snapshot snapshot,Fallback fallback) {
 return outfit_match_v2::process_snapshot::guard_with(snapshot,fallback,[&](const Row& row){return outfit_match_v2::bridge::protected_entry(row.name,row.pid,worker,parent);});
}
static unsigned api_calls{},api_mode{};
static NTSTATUS NTAPI fake_api(SYSTEM_INFORMATION_CLASS kind,PVOID,ULONG bytes,PULONG returned) {
 assert(unsigned(kind)==252);++api_calls;
 if(api_mode==0)return NTSTATUS(0xC0000003); // unsupported class
 if(api_mode==1){*returned=0;return 0;}
 if(api_mode==2){*returned=bytes+1;return 0;}
 if(api_mode==3){*returned=bytes;return NTSTATUS(0xC0000004);}
 if(api_mode==4){*returned=16*1024*1024;return NTSTATUS(0xC0000004);}
 *returned=bytes+1;return NTSTATUS(0xC0000004); // continuing growth, bounded4
}
struct Fixture {
 std::vector<unsigned char> bytes;std::vector<std::size_t> offsets;
 Fixture(std::vector<Row> rows={{0,L""},{123,L"self.exe"},{999,L"safe.exe"}}) {
  std::size_t total{};for(const auto& row:rows){offsets.push_back(total);total+=(sizeof(Basic)+(row.name.empty()?0:(row.name.size()+1)*2)+7)&~std::size_t(7);}
  bytes.resize(total);
  for(unsigned n=0;n<rows.size();++n){Basic p{};const auto off=offsets[n];p.NextEntryOffset=n+1<rows.size()?ULONG(offsets[n+1]-off):0;p.UniqueProcessId=reinterpret_cast<HANDLE>(std::uintptr_t(rows[n].pid));p.SequenceNumber=rows[n].pid?rows[n].pid+100:0;
   if(!rows[n].name.empty()){p.ImageName.Length=USHORT(rows[n].name.size()*2);p.ImageName.MaximumLength=p.ImageName.Length+2;p.ImageName.Buffer=reinterpret_cast<PWSTR>(bytes.data()+off+sizeof p);std::memcpy(p.ImageName.Buffer,rows[n].name.data(),p.ImageName.Length);}
   put(n,p);
  }
 }
 Basic get(unsigned n)const{Basic p{};std::memcpy(&p,bytes.data()+offsets[n],sizeof p);return p;}
 void put(unsigned n,const Basic& p){std::memcpy(bytes.data()+offsets[n],&p,sizeof p);}
};
static void corruption() {
 Fixture f;assert(parse_basic(f.bytes,123)->size()==3);
 assert(!parse_basic(std::span(f.bytes).first(47),123));assert(!parse_basic(f.bytes,0));assert(!parse_basic(f.bytes,444));
 for(unsigned fault=0;fault<24;++fault){const auto backup=f.bytes;auto p=f.get(1);
  if(fault==0)p.NextEntryOffset=40;if(fault==1)p.NextEntryOffset=49;
  if(fault==2)p.NextEntryOffset=ULONG(f.bytes.size()+8);if(fault==3)p.NextEntryOffset=0;
  if(fault==4)p.UniqueProcessId=reinterpret_cast<HANDLE>(~std::uintptr_t{});
  if(fault==5)p.UniqueProcessId=nullptr;if(fault==6)p.SequenceNumber=0;
  if(fault==7)p.ImageName.Length=3;if(fault==8)p.ImageName.MaximumLength=3;
  if(fault==9)p.ImageName.Length=p.ImageName.MaximumLength+2;
  if(fault==10){p.ImageName.Length=520;p.ImageName.MaximumLength=520;}
  if(fault==11)p.ImageName.Buffer=nullptr;
  if(fault==12)p.ImageName.Buffer=reinterpret_cast<PWSTR>(reinterpret_cast<std::uintptr_t>(f.bytes.data())-2);
  if(fault==13)p.ImageName.Buffer=reinterpret_cast<PWSTR>(f.bytes.data()+f.offsets[2]+48);
  if(fault==14)p.ImageName.Buffer=reinterpret_cast<PWSTR>(f.bytes.data()+f.offsets[1]+sizeof p+1);
  if(fault==15)p.ImageName.Buffer=reinterpret_cast<PWSTR>(f.bytes.data()+f.offsets[1]+sizeof p+2);
  if(fault==16)p.ImageName.Length=p.ImageName.MaximumLength=0;
  if(fault==17)p.ImageName.MaximumLength=520;
  if(fault==18)p.ImageName.Buffer=reinterpret_cast<PWSTR>(f.bytes.data()+f.offsets[1]+sizeof p-2);
  if(fault==23)p.InheritedFromUniqueProcessId=reinterpret_cast<HANDLE>(~std::uintptr_t{});
  if(fault>=19 && fault<=22){auto* name=reinterpret_cast<wchar_t*>(f.bytes.data()+f.offsets[1]+sizeof p);name[0]=fault==19?0:fault==20?L'\\':fault==21?wchar_t(0xD800):wchar_t(0xDC00);}
  f.put(1,p);assert(!parse_basic(f.bytes,123));std::copy(backup.begin(),backup.end(),f.bytes.begin());
 }
 // A truncated terminal record and overlapping first/second records reject.
 const auto backup=f.bytes;auto first=f.get(0);first.NextEntryOffset=56;f.put(0,first);assert(!parse_basic(f.bytes,123));std::copy(backup.begin(),backup.end(),f.bytes.begin());
 assert(!parse_basic(std::span(f.bytes).first(f.bytes.size()-8),123));
}
static void count_and_allocation_bounds() {
 std::vector<unsigned char> oversized(16*1024*1024+1);assert(!parse_basic(oversized,123));
 std::vector<Row> rows;rows.reserve(65537);
 for(unsigned n=0;n<65537;++n)rows.push_back({n+1,L"x"});
 Fixture excessive(rows);assert(!parse_basic(excessive.bytes,123));
 rows.pop_back();Fixture maximum(rows);assert(parse_basic(maximum.bytes,123)->size()==65536);
}
static void predicates() {
 const DWORD self=GetCurrentProcessId(),worker=self+1,parent=self+2;
 assert(!blocked({{0,L""},{self,L"NBA2K27.exe"},{worker,L"2k Launcher.exe"},{parent,L"2k Launcher WPF.exe"}},self,worker,parent));
 for(const auto* name:{L"NBA2K27.exe",L"nba2k27.EXE",L"EasyAntiCheat",L"easyanticheat_eos.exe",L"2k Launcher.exe",L"2k Launcher WPF.exe",L"2k Launcher Rust.exe",L"2k Launcher Rust Prototype.exe",L"2K Launcher Tauri QA.exe",L"nba2k27-launcher-tauri2.exe",L"2k-launcher-qa073.exe"}) {
  assert(blocked({{999,name}},self,worker,parent));
  if(std::wstring_view(name).size()>=13 && CompareStringOrdinal(name,13,L"EasyAntiCheat",13,TRUE)==CSTR_EQUAL){assert(blocked({{worker,name}},self,worker,parent));assert(blocked({{parent,name}},self,worker,parent));}
 }
 assert(blocked({{worker,L"NBA2K27.exe"}},self,worker,parent));
 assert(blocked({{parent,L"NBA2K27.exe"}},self,worker,parent));
 for(const auto* name:{L"NBA2K27.exe.bak",L"EasyAntiChea.exe",L"safe.exe",L"2k-launcher.exe"})assert(!blocked({{999,name}},self,worker,parent));
}
static void fallback_and_freshness() {
 assert(!fresh_basic_with(123,nullptr));
 for(api_mode=0;api_mode<6;++api_mode){api_calls=0;assert(!fresh_basic_with(123,&fake_api));assert(api_calls==(api_mode==5?4u:1u));}
 unsigned queries{},fallbacks{};
 auto missing=[&]()->std::optional<std::vector<Row>>{++queries;return {};};
 for(unsigned n=0;n<4;++n)assert(guard_with(123,124,125,missing,[&]{++fallbacks;return true;}));
 assert(queries==4&&fallbacks==4); // fresh per call; no remembered permissive result
 assert(!guard_with(123,124,125,missing,[&]{++fallbacks;return false;}));
 assert(guard_with(123,124,125,[]()->std::optional<std::vector<Row>>{throw std::runtime_error("query failure");},[]{return true;}));
 assert(guard_with(123,124,125,missing,[]()->bool{throw std::runtime_error("legacy snapshot failed");}));
 const auto before=fallbacks;
 assert(!guard_with(123,124,125,[]()->std::optional<std::vector<Row>>{return std::vector<Row>{{123,L"self.exe"}};},[&]{++fallbacks;return true;}));
 assert(fallbacks==before);
 // One failed Basic acquisition must call legacy exactly once. The fallback
 // callable cannot route back through protected_processes()/Basic acquisition.
 unsigned basic_queries{},legacy_calls{};
 assert(guard_with(123,124,125,[&]()->std::optional<std::vector<Row>>{++basic_queries;return {};},[&]{++legacy_calls;return true;}));
 assert(basic_queries==1 && legacy_calls==1);
 Fixture f;auto p=f.get(1);p.ImageName.Length=3;f.put(1,p);
 assert(guard_with(123,124,125,[&]{return parse_basic(f.bytes,123);},[]{return true;}));
}
int main(){corruption();count_and_allocation_bounds();predicates();fallback_and_freshness();std::puts("BasicProcess252 PRODUCTION parser corruption/predicate/fallback/freshness fixtures passed");}
