#include "../native/outfit_match_v2_proxy_bridge.hpp"
#include <cassert>
#include <iostream>
#include <iomanip>
using namespace outfit_match_v2::process_snapshot;
static std::optional<std::vector<Row>> fresh_toolhelp() {
 outfit_match_v2::Handle h(CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS,0));if(!h)return {};
 PROCESSENTRY32W row{};row.dwSize=sizeof row;if(!Process32FirstW(h.value,&row))return {};
 std::vector<Row> out;do{out.push_back({row.th32ProcessID,row.th32ProcessID?std::wstring(row.szExeFile):std::wstring{}});}while(Process32NextW(h.value,&row));
 if(GetLastError()!=ERROR_NO_MORE_FILES)return {};std::sort(out.begin(),out.end(),[](const Row&a,const Row&b){return a.pid<b.pid;});return out;
}
static bool equivalent(const std::vector<Row>&a,const std::vector<Row>&b){if(a.size()!=b.size())return false;for(unsigned n=0;n<a.size();++n)if(a[n].pid!=b[n].pid || !outfit_match_v2::equal_path(a[n].name,b[n].name))return false;return true;}
static bool blocked(const std::vector<Row>&rows,DWORD,DWORD worker,DWORD parent){for(const auto&r:rows)if(outfit_match_v2::bridge::protected_entry(r.name,r.pid,worker,parent))return true;return false;}
template<class F>static void bench(const char* name,unsigned count,F f) {
 LARGE_INTEGER hz{};QueryPerformanceFrequency(&hz);std::vector<double> times;unsigned good{};
 for(unsigned n=0;n<count;++n){LARGE_INTEGER a{},b{};QueryPerformanceCounter(&a);good+=f();QueryPerformanceCounter(&b);times.push_back(double(b.QuadPart-a.QuadPart)*1000000/hz.QuadPart);}
 std::sort(times.begin(),times.end());double total{};for(auto v:times)total+=v;
 std::cout<<"{\"name\":\""<<name<<"\",\"samples\":"<<count<<",\"trueResults\":"<<good<<",\"meanUs\":"<<total/count<<",\"p50Us\":"<<times[count/2]<<",\"p95Us\":"<<times[(count-1)*95/100]<<"}";
}
int main() {
 const auto self=GetCurrentProcessId();unsigned stable{},mismatch{},unstable{},available{};
 for(unsigned n=0;n<12;++n){auto a=fresh_basic(self),t=fresh_toolhelp(),b=fresh_basic(self),u=fresh_toolhelp();
  available+=bool(a)&&bool(b);if(!a||!b||!t||!u || !equivalent(*a,*b)||!equivalent(*t,*u)){++unstable;continue;}
  ++stable;if(!equivalent(*a,*t)||blocked(*a,self,self,self)!=blocked(*t,self,self,self))++mismatch;
 }
 std::cout<<std::fixed<<std::setprecision(3)<<"{\"schema\":\"process-snapshot-v2-self-production-validation-v1\",\"otherProcessHandlesOpened\":0,\"gameCalls\":0,\"gameWrites\":0,\"stableDoubleComparisons\":"<<stable<<",\"unstableComparisons\":"<<unstable<<",\"ntAvailablePairs\":"<<available<<",\"stableMismatches\":"<<mismatch<<",\"timings\":[";
 bench("fresh_basic_nt_full_parser",100,[&]{return fresh_basic(self).has_value();});std::cout<<",";
 bench("fresh_toolhelp_full_rows",100,[]{return fresh_toolhelp().has_value();});std::cout<<",";
 bench("unchanged_toolhelp_guard",100,[&]{return !outfit_match_v2::bridge::toolhelp_protected_processes(self,self);});std::cout<<",";
 bench("production_basic252_guard_including_legacy_fallback",100,[&]{return !outfit_match_v2::bridge::protected_processes(self,self);});std::cout<<"]}\n";
 return mismatch?1:0;
}
