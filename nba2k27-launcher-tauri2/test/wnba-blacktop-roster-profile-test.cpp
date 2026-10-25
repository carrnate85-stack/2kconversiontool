// Disk-only verification. No process handles, game calls, or memory writes.
#define NOMINMAX
#include <windows.h>
#include <bcrypt.h>
#include "../native/wnba_blacktop_roster_profile.hpp"
#include "../native/wnba_blacktop_bridge_unwind.hpp"
#include <algorithm>
#include <cassert>
#include <fstream>
#include <iostream>
#include <stdexcept>
#include <string>
#include <vector>
using namespace wnba_blacktop_roster_profile;
using Bytes=std::vector<std::uint8_t>;
std::uint64_t integer(const Bytes& bytes,std::size_t at,unsigned count) {
 std::uint64_t value{};
 for(unsigned n=0;n<count;++n)value|=std::uint64_t(bytes.at(at+n))<<(8*n);
 return value;
}
std::string sha256(const Bytes& bytes) {
 BCRYPT_ALG_HANDLE algorithm{};BCRYPT_HASH_HANDLE hash{};
 if(BCryptOpenAlgorithmProvider(&algorithm,BCRYPT_SHA256_ALGORITHM,nullptr,0))throw std::runtime_error("SHA provider");
 DWORD object_bytes{},got{};
 if(BCryptGetProperty(algorithm,BCRYPT_OBJECT_LENGTH,reinterpret_cast<PUCHAR>(&object_bytes),sizeof object_bytes,&got,0))throw std::runtime_error("SHA object");
 Bytes object(object_bytes),digest(32);
 if(BCryptCreateHash(algorithm,&hash,object.data(),object_bytes,nullptr,0,0) ||
  BCryptHashData(hash,const_cast<PUCHAR>(bytes.data()),ULONG(bytes.size()),0) ||
  BCryptFinishHash(hash,digest.data(),ULONG(digest.size()),0))throw std::runtime_error("SHA digest");
 BCryptDestroyHash(hash);BCryptCloseAlgorithmProvider(algorithm,0);
 constexpr char hex[]="0123456789ABCDEF";std::string result;
 for(auto byte:digest){result.push_back(hex[byte>>4]);result.push_back(hex[byte&15]);}
 return result;
}
int main(int argc,char** argv) {
 std::ifstream file(argc>1?argv[1]:"C:\\Program Files (x86)\\Steam\\steamapps\\common\\NBA 2K27\\NBA2K27.exe",std::ios::binary);
 if(!file)throw std::runtime_error("read-only game image open failed");
 auto at=[&](std::uint64_t offset,std::size_t size) {
  if(!size || size>65536)throw std::runtime_error("bounded disk read");
  Bytes result(size);file.seekg(std::streamoff(offset));file.read(reinterpret_cast<char*>(result.data()),std::streamsize(size));
  if(!file)throw std::runtime_error("disk image read failed");return result;
 };
 const auto header=at(0,4096);assert(header[0]=='M' && header[1]=='Z');
 const auto pe=std::size_t(integer(header,60,4));assert(pe<3500);
 assert(integer(header,pe,4)==0x4550 && integer(header,pe+4,2)==0x8664 && integer(header,pe+8,4)==0x6AD2E48C);
 assert(integer(header,pe+24+56,4)==0x36532000);
 const auto image_base=integer(header,pe+24+24,8);assert(image_base==0x140000000);
 struct Section {std::uint64_t rva,bytes,offset,characteristics;};std::vector<Section> sections;
 const auto section_at=pe+24+std::size_t(integer(header,pe+20,2));
 for(std::size_t n=0;n<integer(header,pe+6,2);++n) {
  const auto offset=section_at+40*n;
  sections.push_back({integer(header,offset+12,4),integer(header,offset+16,4),integer(header,offset+20,4),integer(header,offset+36,4)});
 }
 auto read=[&](std::uintptr_t rva,std::size_t size) {
  for(const auto& section:sections)if(rva>=section.rva && rva+size<=section.rva+section.bytes)return at(section.offset+rva-section.rva,size);
  throw std::runtime_error("RVA is not fully file backed");
 };
 for(const auto& method:kMethods)assert(sha256(read(method.rva,method.bytes))==method.hash);
 const auto pdata=std::find_if(sections.begin(),sections.end(),[&](const Section& section){
  return wnba_blacktop_bridge_unwind::kStockFunctionRva>=section.rva &&
   wnba_blacktop_bridge_unwind::kStockFunctionRva+12<=section.rva+section.bytes;
 });
 assert(pdata!=sections.end() && (pdata->characteristics&IMAGE_SCN_MEM_READ) &&
  !(pdata->characteristics&(IMAGE_SCN_MEM_WRITE|IMAGE_SCN_MEM_EXECUTE)));
 const auto runtime=read(wnba_blacktop_bridge_unwind::kStockFunctionRva,12);
 assert(integer(runtime,0,4)==0x230BE80 && integer(runtime,4,4)==0x230BF7B &&
  integer(runtime,8,4)==wnba_blacktop_bridge_unwind::kStockUnwindRva);
 assert(bytes_equal(read(wnba_blacktop_bridge_unwind::kStockUnwindRva,
  wnba_blacktop_bridge_unwind::kStockFrame.size()),wnba_blacktop_bridge_unwind::kStockFrame));
 assert(sha256(read(0x230BE80,0x230BF7B-0x230BE80))==
  "C8B6AA7952EABD418A8AD788E661728A5F223790031B7BF84DE6EF57F6CF7A92");
 const auto leaf=read(kGeneral,kGeneralLeaf.size()),asset=read(kAssetRva,sizeof(kAsset));
 assert(general_leaf_matches(leaf,asset));
 for(std::size_t n=0;n<leaf.size();++n) {auto changed=leaf;changed[n]^=1;assert(!general_leaf_matches(changed,asset));}
 assert(bytes_equal(read(kCallsiteRva,kCallsite.size()),kCallsite));
 assert(bytes_equal(read(kManagerCallsiteRva,kManagerCallsite.size()),kManagerCallsite));
 assert(read(kSameDelegateCompare,4)==Bytes({0x48,0x3B,0x46,0x10}));
 assert(integer(read(kDelegate,8),0,8)-image_base==kVtable);
 const auto prefix=read(kVtable,kVtablePrefixBytes);Bytes normalized(prefix.size());
 std::vector<std::uintptr_t> neighbors;
 for(std::size_t n=0;n<prefix.size()/8;++n) {
  const auto pointer=integer(prefix,n*8,8);assert(pointer>=image_base && pointer<image_base+0x36532000);
  const auto rva=pointer-image_base;
  for(unsigned j=0;j<8;++j)normalized[n*8+j]=std::uint8_t(rva>>(8*j));
  if(n*8>=kNeighborOffset)neighbors.push_back(std::uintptr_t(rva));
 }
 assert(sha256(normalized)==kNormalizedVtableSha256 && neighbors_match(neighbors));
 neighbors[8]=kGeneral;assert(!neighbors_match(neighbors) && neighbors_match(neighbors,true));
 neighbors[7]=kGeneral;assert(!neighbors_match(neighbors,true));
 static_assert(kSlot==0x74CDE88 && kInstalledDelegateRva==0xAB160E8 && kStock!=0x23531A0);
 std::cout<<"Blacktop roster disk profile: 9 complete method hashes, leaf asset/RIP targets, native caller/manager fields, static delegate, 427 normalized vtable slots, owned-neighbor guards and exact read-only Blacktop bridge unwind source PASS\n";
}
