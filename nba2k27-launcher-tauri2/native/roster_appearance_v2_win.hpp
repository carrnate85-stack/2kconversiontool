#pragma once

// Actual in-process native CAS executor. No game function calls or code hooks.

#include "outfit_match_v2_win.hpp"

#include "roster_appearance_v2_core.hpp"

#include <functional>

namespace roster_appearance_v2 {

namespace win_detail {

using namespace outfit_match_v2;

struct Method {std::uintptr_t rva;std::size_t bytes;const char* hash;};

inline constexpr Method methods[]={
 {0x2E24B20,13,"85D7B5F786555896AE76A6F1FF8F2B921FC39F4882129F3EAD1558AE219165CC"},
 {0x2E24C10,14,"BF0D729E28C1549182D4861562FBC331562DEEAEAA40F5971DD20833B15DE01A"},
 {0x2E24B37,37,"E27294D6C7A507115CE3FD7D9C70C8041903DF01B7ACC98DC753C6A762B15D01"},
 {0x2E24C27,41,"E2EA550377E8F12A4FB29D7E4CEC9620FBBB794738DE2E80349A72FD3CEABD87"},
 {0x31A46A0,34,"D6A4CCFBD3053D17799FE5A4F042FE39090705D18C396A20FB7C621601183B6A"},
 {0x1B0DF24,49,"CA56B57AC277AB2FC27BE8AFE5B079F938AD3022F00DC284D2BE7C1D29D05F06"},
 {0x2E14540,26,"BE554DE4EC45B69C4F5D0879F28C4D8705230E4AB9FD9080A17A3E7F81F9E0D3"},
 {0x2E24B30,211,"592D0C3635FAE15354FA04853BA86AA1CB6F1CD8043C09720FB91A52339C174A"},
 {0x2E24C20,215,"4543EBF9DFD0A4AD804E5FD6F871737CF5CDB59107563D14CB223AFF6070878F"},
 {0x2CA8380,1775,"93484610C79CD7015EE6E4B636036C62F8E2EECBEEF36F425771DDE3DBC3D6A9"},
 {0x2EDDBB0,180,"6617084F97B5CB3CE9F6D6162F8824E570362AB0BA4C597C7A4CB81783FB4119"},
};

inline bool hash_bytes(const void* p,std::size_t n,Sha& out) {

 Hash hash;if(!hash.update(p,n))return false;const auto hex=hash.finish();if(hex.size()!=64)return false;

 for(unsigned i=0;i<32;++i){auto value=[](char c)->int{return c>='0'&&c<='9'?c-'0':c>='A'&&c<='F'?c-'A'+10:-1;};const int a=value(hex[i*2]),b=value(hex[i*2+1]);if(a<0||b<0)return false;out[i]=static_cast<unsigned char>((a<<4)|b);}return true;

}

inline bool same_hash(const std::vector<unsigned char>& b,const Sha& expected) {Sha actual{};return hash_bytes(b.data(),b.size(),actual)&&actual==expected;}

inline bool committed_range(std::uintptr_t at,std::size_t size,std::uintptr_t allocation,DWORD protection=0) {

 if(at<0x10000||!size||size>0x8000000||at>0x7fff00000000ull-size)return false;

 const auto end=at+size;for(unsigned pages=0;at<end&&pages<4096;++pages){MEMORY_BASIC_INFORMATION m{};

 if(VirtualQuery(reinterpret_cast<void*>(at),&m,sizeof m)!=sizeof m||m.State!=MEM_COMMIT||m.Type!=MEM_PRIVATE||reinterpret_cast<std::uintptr_t>(m.AllocationBase)!=allocation||

 (protection?m.Protect!=protection:(m.Protect!=PAGE_READONLY&&m.Protect!=PAGE_READWRITE&&m.Protect!=PAGE_WRITECOPY)))return false;

 const auto start=reinterpret_cast<std::uintptr_t>(m.BaseAddress);if(start>at||!m.RegionSize||start>0x7fff00000000ull-m.RegionSize||start+m.RegionSize<=at)return false;at=std::min(end,start+m.RegionSize);}return at==end;

}

inline bool allocation_of(std::uintptr_t table,std::uintptr_t& base,std::size_t& length) {

 MEMORY_BASIC_INFORMATION first{};if(VirtualQuery(reinterpret_cast<void*>(table),&first,sizeof first)!=sizeof first)return false;

 base=reinterpret_cast<std::uintptr_t>(first.AllocationBase);if(base<0x10000||base%4096)return false;auto at=base;

 for(unsigned n=0;n<4096;++n){MEMORY_BASIC_INFORMATION m{};if(VirtualQuery(reinterpret_cast<void*>(at),&m,sizeof m)!=sizeof m)return false;

 if(reinterpret_cast<std::uintptr_t>(m.AllocationBase)!=base){length=at-base;return length>=0x800000&&length<=0x8000000;}

 if(m.State!=MEM_COMMIT||m.Type!=MEM_PRIVATE||(m.Protect!=PAGE_READONLY&&m.Protect!=PAGE_READWRITE&&m.Protect!=PAGE_WRITECOPY)||reinterpret_cast<std::uintptr_t>(m.BaseAddress)!=at||!m.RegionSize||m.RegionSize>0x8000000||at-base>0x8000000-m.RegionSize)return false;at+=m.RegionSize;}

 return false;

}

inline bool image_range(std::uintptr_t base,std::uintptr_t at,std::size_t n) {MEMORY_BASIC_INFORMATION m{};return at>=base&&n&&at-base<=0x36532000&&n<=0x36532000-(at-base)&&VirtualQuery(reinterpret_cast<void*>(at),&m,sizeof m)==sizeof m&&m.State==MEM_COMMIT&&m.Type==MEM_IMAGE&&reinterpret_cast<std::uintptr_t>(m.AllocationBase)==base&&m.Protect==PAGE_EXECUTE_READ&&reinterpret_cast<std::uintptr_t>(m.BaseAddress)<=at&&m.RegionSize>=n&&at-reinterpret_cast<std::uintptr_t>(m.BaseAddress)<=m.RegionSize-n;}

inline bool profile(std::uintptr_t base) {

 for(const auto& p:methods){if(!image_range(base,base+p.rva,p.bytes))return false;std::vector<unsigned char> b(p.bytes);if(!read_memory(base+p.rva,b.data(),b.size())||digest(b)!=p.hash)return false;}

 for(const auto rva:{0x211A1ABull,0x211A1D0ull}){std::array<unsigned char,7>b{};if(!read_memory(base+rva,b.data(),b.size())||b[0]!=0x48||b[1]!=0x8B||b[2]!=0x05)return false;std::int32_t d{};std::memcpy(&d,b.data()+3,4);if(static_cast<std::int64_t>(rva)+7+d!=0x9ACE338)return false;}

 std::uint32_t cm{};if(!value(base+0x5D0E560,cm)||cm!=0x3C23D70A)return false;

 return q(base+0x74EBE98)==base+0x24E66A0&&q(base+0x74EBEA0)==base+0x24E6710;

}

struct Directory {std::uintptr_t pointer{};std::size_t count{},stride{};};

inline bool separate(std::uintptr_t a,std::size_t an,std::uintptr_t b,std::size_t bn){return a+an<=b||b+bn<=a;}

struct Scope {std::uintptr_t root{},owner{},base{},appearance{};std::size_t length{};std::vector<unsigned char> header;std::array<Directory,6> directories;};

inline bool directory(const Scope& s,unsigned i,std::size_t offset,std::size_t stride,std::size_t min,std::size_t max,bool count32,Directory& out) {

 std::uint64_t count{},pointer{};std::memcpy(&count,s.header.data()+offset,count32?4:8);std::memcpy(&pointer,s.header.data()+offset+8,8);

 if(i==3&&count==0&&pointer==0){out={};return true;}

 if(count<min||count>max||pointer%8||pointer<s.base+0x680||count>0x8000000/stride||count*stride>s.length||pointer>s.base+s.length-count*stride)return false;

 out={static_cast<std::uintptr_t>(pointer),static_cast<std::size_t>(count),stride};return committed_range(out.pointer,out.count*out.stride,s.base);

}

// The only actual atomic effect lives in a separate POD-only translation unit.
// Compile that TU with the proven asynchronous SEH flags, never the C++ host.
extern "C" bool c27_roster_appearance_cas32(std::uintptr_t,std::uint32_t,std::uint32_t,std::uint32_t&) noexcept;
inline bool cas32(std::uintptr_t at,std::uint32_t before,std::uint32_t after,std::uint32_t& observed) noexcept {
 return c27_roster_appearance_cas32(at,before,after,observed);
}
} // namespace win_detail

class NativeScope {

 std::uintptr_t image_{};std::function<bool(bool)> authorized_;win_detail::Scope last_{};bool checked_{};

 bool capture(const Request& r,bool after,win_detail::Scope& s) {

  using namespace win_detail;if(!authorized_(false)||!profile(image_))return false;

  s.root=q(image_+0x9ACE338);if(!private_span(s.root,0xB0)||q(s.root)!=image_+0x74EBE98)return false;s.owner=q(s.root+0xA8);

  const auto table=q(s.owner+0x18);if(!allocation_of(table,s.base,s.length)||!committed_range(s.owner,0x3D0,s.base))return false;

  s.header.resize(0x3D0);if(!read_memory(s.owner,s.header.data(),s.header.size()))return false;

  const std::array<std::size_t,6> offsets={0x10,0x60,0x80,0x290,0x180,0x3C0},strides={0x4F8,0x1278,0x1550,0x170,0x1C8,0x68},mins={100,1,1,1,1,1},maxs={8192,2048,1024,8192,8192,65536};

  for(unsigned i=0;i<6;++i)if(!directory(s,i,offsets[i],strides[i],mins[i],maxs[i],i>=4,s.directories[i]))return false;

  for(unsigned i=0;i<6;++i){const auto&a=s.directories[i];if(!a.count)continue;if(!separate(a.pointer,a.count*a.stride,s.owner,0x3D0)||!separate(a.pointer,a.count*a.stride,s.root,0xB0))return false;for(unsigned j=0;j<i;++j){const auto&b=s.directories[j];if(b.count&&!separate(a.pointer,a.count*a.stride,b.pointer,b.count*b.stride))return false;}}

  const auto& players=s.directories[0];if(r.index>=players.count)return false;std::vector<unsigned char> row(players.stride);if(!read_memory(players.pointer+r.index*players.stride,row.data(),row.size())||!same_hash(row,r.row))return false;

  std::memcpy(&s.appearance,row.data()+0x78,8);const auto&pool=s.directories[5];if(s.appearance%4||s.appearance<pool.pointer||(s.appearance-pool.pointer)%0x68||(s.appearance-pool.pointer)/0x68>=pool.count||!committed_range(s.appearance,0x68,s.base)||!committed_range(s.appearance,4,s.base,PAGE_READWRITE))return false;

  std::vector<unsigned char> pointers;auto append=[&](const void* p,std::size_t n){const auto*b=static_cast<const unsigned char*>(p);pointers.insert(pointers.end(),b,b+n);};

  for(unsigned kind:{0u,4u}){const auto&dir=s.directories[kind];const auto count=static_cast<std::uint32_t>(dir.count);append(&count,4);for(std::size_t i=0;i<dir.count;++i){std::uintptr_t p{};if(!value(dir.pointer+i*dir.stride+(kind==0?0x78:0),p))return false;if(p&&(p<pool.pointer||(p-pool.pointer)%0x68||(p-pool.pointer)/0x68>=pool.count))return false;if(p&&!(kind==0&&i==r.index)&&!separate(s.appearance,4,p,4))return false;append(&p,8);}}

  if(!same_hash(pointers,r.pointers))return false;

  std::vector<unsigned char> metadata;auto meta=[&](const void* p,std::size_t n){const auto*b=static_cast<const unsigned char*>(p);metadata.insert(metadata.end(),b,b+n);};

  for(auto n:{image_,s.root,s.owner,s.base,static_cast<std::uintptr_t>(s.length)})meta(&n,8);meta(s.header.data(),s.header.size());meta(&s.appearance,8);

  std::array<unsigned char,0x68> appearance{};if(!read_memory(s.appearance,appearance.data(),appearance.size()))return false;meta(appearance.data()+4,appearance.size()-4);

  if(!same_hash(metadata,r.metadata)||!std::equal(appearance.begin(),appearance.begin()+4,(after?r.after:r.before).begin()))return false;

  return authorized_(false)&&q(image_+0x9ACE338)==s.root&&q(s.root+0xA8)==s.owner&&q(players.pointer+r.index*players.stride+0x78)==s.appearance;

 }

public:

 NativeScope(std::uintptr_t image,std::function<bool(bool)> authority):image_(image),authorized_(std::move(authority)){}

 bool check(const Request& r,bool after) {win_detail::Scope a,b;checked_=false;if(!capture(r,after,a)||!capture(r,after,b)||a.root!=b.root||a.owner!=b.owner||a.base!=b.base||a.length!=b.length||a.appearance!=b.appearance||a.header!=b.header)return false;last_=std::move(b);checked_=true;return true;}

 Atomic compare(const Request& r,bool undo) {

  using namespace win_detail;if(!authorized_(!undo)||!check(r,undo)||!authorized_(!undo)||!committed_range(last_.appearance,4,last_.base,PAGE_READWRITE)||q(image_+0x9ACE338)!=last_.root||q(last_.root+0xA8)!=last_.owner||q(last_.directories[0].pointer+r.index*0x4F8+0x78)!=last_.appearance)return Atomic::conflict;

  std::uint32_t observed{};const auto before=word(undo?r.after:r.before),after=word(undo?r.before:r.after);

  if(!cas32(last_.appearance,before,after,observed))return Atomic::uncertain;

  return observed==before?Atomic::applied:Atomic::conflict;

 }

};

} // namespace roster_appearance_v2

