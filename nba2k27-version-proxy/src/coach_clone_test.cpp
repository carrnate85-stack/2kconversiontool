#include "coach_clone_audit.hpp"
#include <cassert>
struct Fixture {
 std::array<unsigned char,0xf38> data{};
 std::array<std::uintptr_t,5> owner{};
 std::array<std::uintptr_t,8> exposed{},owned{};
 std::array<std::array<wchar_t,16>,8> names{};
 static constexpr std::uintptr_t base=0x10000000;
 std::uintptr_t at(){return reinterpret_cast<std::uintptr_t>(data.data());}
 void put(unsigned o,std::uintptr_t p){std::memcpy(data.data()+o,&p,8);}
 Fixture(){put(0,base+0x6804818);put(0xe8,at()+0x128);put(0x108,at()+0x100);put(0x110,at()+0x100);data[0x7d]=1;}
 bool capture(coach_clone_audit::Snapshot &s){return coach_clone_audit::capture(at(),base,s);}
 void clothing(){
  for(unsigned i=0;i<8;++i){std::swprintf(names[i].data(),16,L"piece%u",i);exposed[i]=owned[i]=reinterpret_cast<std::uintptr_t>(names[i].data());}
  owner={base+0x683a4c8,8,reinterpret_cast<std::uintptr_t>(exposed.data()),0,reinterpret_cast<std::uintptr_t>(owned.data())};
  put(0x118,reinterpret_cast<std::uintptr_t>(owner.data()));put(0xc8,owner[2]);put(0xd0,8);
 }
};
int main(){Fixture a,b;coach_clone_audit::Snapshot x,y;assert(a.capture(x)&&b.capture(y));assert(coach_clone_audit::independent(x,y));
#ifdef NBA2K27_COACH_SAME_SIZE_REBUILD
 assert(coach_clone_audit::coach_rebuild::same_size(1.03f,1.03f));
 assert(!coach_clone_audit::coach_rebuild::same_size(1.03f,0.97f));
 assert(!coach_clone_audit::coach_rebuild::same_size(NAN,1.f));
 assert(!coach_clone_audit::coach_rebuild::same_size(1.f,INFINITY));
 assert(!coach_clone_audit::coach_rebuild::same_size(0.f,0.f));
 assert(coach_clone_audit::coach_rebuild::scale_input(.98f));
 assert(coach_clone_audit::coach_rebuild::scale_input(.9506f));
 assert(!coach_clone_audit::coach_rebuild::scale_input(NAN));
 assert(!coach_clone_audit::coach_rebuild::scale_input(1.f));
 assert(!coach_clone_audit::coach_rebuild::scale_input(.90f));
 assert(coach_clone_audit::coach_rebuild::same_size(1.03674f*.98f,1.0160052f));
 assert(!coach_clone_audit::coach_rebuild::same_size(1.03674f*.98f,1.03674f));
 assert(coach_clone_audit::unchanged(x,x));assert(!coach_clone_audit::unchanged(x,y));
#endif
 assert(!coach_clone_audit::independent(x,x));auto altered=y;altered.bytes[0x7d]=3;assert(!coach_clone_audit::independent(x,altered));
 x.override_data=0x10000;y.override_data=0x10000;assert(!coach_clone_audit::independent(x,y));
 a.put(0xe8,b.at()+0x128);assert(!a.capture(x));a.put(0xe8,a.at()+0x128);a.put(0x110,b.at()+0x100);assert(!a.capture(x));
 Fixture c,d;c.clothing();d.clothing();assert(c.capture(x)&&d.capture(y));assert(x.array!=x.owned_array);assert(coach_clone_audit::independent(x,y));
 d.owned[3]=d.owned[2];assert(!d.capture(y));d.owned=d.exposed;
 assert(d.capture(y));y.owned_array=x.array;assert(!coach_clone_audit::independent(x,y));
 for(unsigned i=3;i<8;++i){c.exposed[i]=c.owned[i]=d.exposed[i]=d.owned[i]=0;}
 assert(c.capture(x)&&d.capture(y));assert(coach_clone_audit::independent(x,y));
 d.owned[4]=d.exposed[0];assert(!d.capture(y));d.owned[4]=0;
 d.names[0][0]=0;assert(!d.capture(y));
 std::puts("PASS: coach copy identity, separate 8-item arrays, mismatched contents, cross-alias and owned-list guards. No game access.");}
