#include "../native/outfit_match_v2_win.hpp"
#include <cassert>
#include <chrono>
#include <iostream>
#include <thread>
using namespace outfit_match_v2;
static std::string old_digest(const std::vector<unsigned char>& b) {
 BCRYPT_ALG_HANDLE a{};BCRYPT_HASH_HANDLE h{};DWORD size{},got{};
 assert(BCryptOpenAlgorithmProvider(&a,BCRYPT_SHA256_ALGORITHM,nullptr,0)>=0);
 assert(BCryptGetProperty(a,BCRYPT_OBJECT_LENGTH,reinterpret_cast<PUCHAR>(&size),sizeof size,&got,0)>=0&&got==sizeof size);
 std::vector<unsigned char> object(size);std::array<unsigned char,32> out{};
 assert(BCryptCreateHash(a,&h,object.data(),size,nullptr,0,0)>=0);
 assert(BCryptHashData(h,const_cast<PUCHAR>(b.data()),ULONG(b.size()),0)>=0);
 assert(BCryptFinishHash(h,out.data(),DWORD(out.size()),0)>=0);
 BCryptDestroyHash(h);BCryptCloseAlgorithmProvider(a,0);
 std::string value;for(auto n:out){value.push_back("0123456789ABCDEF"[n>>4]);value.push_back("0123456789ABCDEF"[n&15]);}return value;
}
int main() {
 const auto abc=std::string("BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD");
 const auto empty=std::string("E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855");
 assert(digest({})==empty);assert(digest({'a','b','c'})==abc);
 {Hash first;assert(first.update("a",1));
  {Hash second;assert(second.update("abc",3));assert(second.finish()==abc);}
  assert(first.update("bc",2));assert(first.finish()==abc);}
 std::vector<unsigned char> b(8192,0x5A);const auto expected=old_digest(b);
 assert(digest(b)==expected);b[123]^=1;assert(digest(b)!=expected);b[123]^=1;
 std::vector<std::thread> workers;
 for(unsigned t=0;t<8;++t)workers.emplace_back([=](){for(unsigned n=0;n<250;++n){assert(digest(b)==expected);assert(digest({'a','b','c'})==abc);}});
 for(auto& worker:workers)worker.join();
 using Clock=std::chrono::steady_clock;
 constexpr unsigned loops=2000;
 auto started=Clock::now();for(unsigned n=0;n<loops;++n)assert(old_digest(b)==expected);
 const auto old_ms=std::chrono::duration<double,std::milli>(Clock::now()-started).count();
 started=Clock::now();for(unsigned n=0;n<loops;++n)assert(digest(b)==expected);
 const auto new_ms=std::chrono::duration<double,std::milli>(Clock::now()-started).count();
 std::cout<<"PASS SHA256 known empty/abc, independent overlapping objects, changed input and8threads; fresh2000x8192B oldProvider="<<old_ms<<"ms currentImplementation="<<new_ms<<"ms\n";
}
