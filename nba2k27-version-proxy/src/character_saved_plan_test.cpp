#include <array>
#include <cstdint>
#include <cstring>
#include <cwchar>
#include <cassert>
#include <cstdio>
#include <fstream>
#include <vector>
#include <string>
#include "character_saved_plan.hpp"
int main(int argc,char **argv){
 if(argc==4){
  std::ifstream f(argv[1],std::ios::binary);std::vector<unsigned char> bytes((std::istreambuf_iterator<char>(f)),{});SavedPlan plan{};
  if(!parse_saved_plan(bytes.data(),bytes.size(),static_cast<unsigned>(std::stoul(argv[2])),std::stoull(argv[3]),plan))return 1;
  for(unsigned n=0;n<plan.count;++n)std::printf("PIECES=%u\n",plan.choices[n].pieces);return 0;
 }
 constexpr char token[]="SAVED_OUTFITS_V1";
 assert(saved_trigger_matches(token,sizeof(token)-1));
 assert(!saved_trigger_matches(token,sizeof(token)-2));
 assert(!saved_trigger_matches(token,sizeof(token)));
 assert(!saved_trigger_matches("SAVED_OUTFITS_X1",sizeof(token)-1));
 std::array<unsigned char,24+2*2056> b{};
 auto put=[&](unsigned at,auto value){std::memcpy(b.data()+at,&value,sizeof(value));};
 put(0,0x31504F42u);put(4,42u);put(8,2u);put(16,std::uint64_t(123));
 for(unsigned n=0;n<2;++n){put(24+n*2056,std::uint64_t(0x10000+n*0x100));
  for(unsigned j=0;j<4;++j){const wchar_t text[]=L"asset_test";std::memcpy(b.data()+32+n*2056+j*512,text,sizeof(text));}}
 SavedPlan p{};assert(parse_saved_plan(b.data(),b.size(),42,123,p));assert(p.count==2);
 assert(!parse_saved_plan(b.data(),b.size(),43,123,p));assert(!parse_saved_plan(b.data(),b.size(),42,124,p));
 assert(!parse_saved_plan(b.data(),b.size()-1,42,123,p));
 put(24+2056,std::uint64_t(0x10000));assert(!parse_saved_plan(b.data(),b.size(),42,123,p));
 put(24+2056,std::uint64_t(0x10100));b[32]='/';assert(!parse_saved_plan(b.data(),b.size(),42,123,p));
 b[32]='a';put(8,11u);assert(!parse_saved_plan(b.data(),b.size(),42,123,p));
 std::array<unsigned char,24+2*2576> v{};
 auto set=[&](unsigned at,auto value){std::memcpy(v.data()+at,&value,sizeof(value));};
 set(0,0x32504F42u);set(4,42u);set(8,2u);set(16,std::uint64_t(123));
 for(unsigned n=0;n<2;++n){set(24+n*2576,std::uint64_t(0x10000+n*0x100));set(32+n*2576,4+n);
  for(unsigned j=0;j<4+n;++j){const wchar_t text[]=L"asset_test";std::memcpy(v.data()+40+n*2576+j*512,text,sizeof(text));}}
 assert(parse_saved_plan(v.data(),v.size(),42,123,p));assert(p.choices[0].pieces==4&&p.choices[1].pieces==5);
 assert(!parse_saved_plan(v.data(),v.size()-1,42,123,p));
 set(32+2576,6u);assert(!parse_saved_plan(v.data(),v.size(),42,123,p));set(32+2576,5u);
 v[40+4*512]='x';assert(!parse_saved_plan(v.data(),v.size(),42,123,p));v[40+4*512]=0;
 v[40+2576+4*512]='/';assert(!parse_saved_plan(v.data(),v.size(),42,123,p));
 std::vector<unsigned char> w(kSavedMaxBytes);
 auto write=[&](unsigned at,auto value){std::memcpy(w.data()+at,&value,sizeof(value));};
 write(0,0x33504F42u);write(4,42u);write(8,10u);write(16,std::uint64_t(123));
 for(unsigned n=0;n<10;++n){const auto at=24+n*kSavedV3Stride;write(at,std::uint64_t(0x10000+n*256));write(at+8,4+n%4);
  for(unsigned j=0;j<4+n%4;++j){const wchar_t text[]=L"asset_test";std::memcpy(w.data()+at+16+j*512,text,sizeof(text));}}
 assert(parse_saved_plan(w.data(),w.size(),42,123,p));assert(p.count==10);
 for(unsigned n=0;n<10;++n){assert(p.choices[n].pieces==4+n%4);for(unsigned j=0;j<p.choices[n].pieces;++j)assert(std::wcscmp(p.choices[n].names[j].data(),L"asset_test")==0);}
 assert(!parse_saved_plan(w.data(),w.size()-1,42,123,p));
 write(32,8u);assert(!parse_saved_plan(w.data(),w.size(),42,123,p));write(32,4u);
 w[40+4*512]='x';assert(!parse_saved_plan(w.data(),w.size(),42,123,p));w[40+4*512]=0;
 w[40+3*kSavedV3Stride+6*512]='/';assert(!parse_saved_plan(w.data(),w.size(),42,123,p));w[40+3*kSavedV3Stride+6*512]='a';
 assert(parse_saved_plan(w.data(),w.size(),42,123,p));
 // Three-piece V3 choices omit explicit shoes; unused slots must remain zero.
 write(32,3u);std::memset(w.data()+40+3*512,0,512);
 assert(parse_saved_plan(w.data(),w.size(),42,123,p));assert(p.choices[0].pieces==3);
 assert(p.choices[0].names[3][0]==0);
 write(32,2u);assert(!parse_saved_plan(w.data(),w.size(),42,123,p));write(32,3u);
 w[40+3*512]='x';assert(!parse_saved_plan(w.data(),w.size(),42,123,p));w[40+3*512]=0;
 // Permit only the exact stock shirtless marker, including clean trailing padding.
 std::memset(w.data()+40,0,512);const wchar_t bare[]=L"NO SHIRT";
 std::memcpy(w.data()+40,bare,sizeof(bare));
 assert(parse_saved_plan(w.data(),w.size(),42,123,p));
 assert(std::wcscmp(p.choices[0].names[0].data(),bare)==0);
 w[40+sizeof(bare)]='x';assert(!parse_saved_plan(w.data(),w.size(),42,123,p));w[40+sizeof(bare)]=0;
 w[40]='n';assert(!parse_saved_plan(w.data(),w.size(),42,123,p));w[40]='N';
 w[40+6]=' ';assert(!parse_saved_plan(w.data(),w.size(),42,123,p));w[40+6]='S';
 assert(parse_saved_plan(w.data(),w.size(),42,123,p));
 assert(!parse_saved_plan(w.data(),w.size(),43,123,p));
 w.push_back(0);assert(!parse_saved_plan(w.data(),w.size(),42,123,p));
 std::puts("PASS: v1/v2/v3 plans, ten players, four-to-seven pieces, exact size, session identity, distinct players, bounded names and count. No game calls.");
}
