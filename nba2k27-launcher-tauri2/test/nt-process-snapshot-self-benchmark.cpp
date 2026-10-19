#include "nt-process-snapshot-research.hpp"
#include <cassert>
#include <iostream>
#include <iomanip>
using namespace nt_snapshot_research;
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
 std::cout<<std::fixed<<std::setprecision(3)<<"{\"schema\":\"nt-process-snapshot-self-research-v1\",\"otherProcessHandlesOpened\":0,\"gameCalls\":0,\"gameWrites\":0,\"stableDoubleComparisons\":"<<stable<<",\"unstableComparisons\":"<<unstable<<",\"ntAvailablePairs\":"<<available<<",\"stableMismatches\":"<<mismatch<<",\"timings\":[";
 bench("fresh_basic_nt_full_parser",100,[&]{return fresh_basic(self).has_value();});std::cout<<",";
 bench("fresh_nt_full_parser",100,[&]{auto rows=fresh_nt(self);return rows.has_value();});std::cout<<",";
 bench("fresh_toolhelp_full_rows",100,[]{return fresh_toolhelp().has_value();});std::cout<<",";
 bench("production_toolhelp_guard",100,[&]{return !outfit_match_v2::bridge::protected_processes(self,self);});std::cout<<",";
 bench("candidate_basic_guard_including_fallback",100,[&]{return !candidate_basic_guard(self,self,self);});std::cout<<",";
 bench("candidate_guard_including_fallback",100,[&]{return !candidate_guard(self,self,self);});std::cout<<"]}\n";
 return mismatch?1:0;
}
