#define NBA2K27_SAME_OUTFIT_REBUILD
#include "character_description_clone_audit.hpp"
#include <cassert>
#include <cstdio>
namespace audit=character_description_clone_audit;
namespace rebuild=audit::same_outfit_rebuild;
struct Fixture {
  std::array<unsigned char,4096> globals{};
  std::array<unsigned char,0x148> complete{};
  std::array<std::uintptr_t,8> node{};
  std::uintptr_t base{},manager{},player=0x99990000;
  void put(std::uintptr_t address,std::uintptr_t value){std::memcpy(reinterpret_cast<void *>(address),&value,8);}
  Fixture() {
    base=reinterpret_cast<std::uintptr_t>(globals.data())-0xA37BFB0;
    const auto c=reinterpret_cast<std::uintptr_t>(complete.data());manager=c+0x78;
    const auto active=base+0xA37BFB0,managed=manager+0x80;
    const auto n=reinterpret_cast<std::uintptr_t>(node.data());
    put(c,base+0x681CCC8);put(manager,base+0x681D088);
    put(c+8,active);put(c+0x10,active);put(active+8,c);put(active+0x10,c);
    put(c+0x90,1);put(manager+0x98,player);put(manager+0x88,n);put(manager+0x90,n);
    node={base+0x681CB68,managed,managed,123,player,0,0,0};
    const auto queue=base+0xA37C108;
    put(queue,base+0x68506A8);put(queue+0x10,queue+8);put(queue+0x18,queue+8);
    *reinterpret_cast<unsigned char *>(queue+0x5A)=1;
  }
};
int main() {
  Fixture f;
  assert(rebuild::active_member(f.base,f.manager-0x78));
  assert(rebuild::idle(f.base,f.manager,f.player,123));
  assert(!rebuild::idle(f.base,f.manager,f.player,124));
  assert(!rebuild::idle(f.base,f.manager,f.player+8,123));
  f.node[7]=1;assert(!rebuild::idle(f.base,f.manager,f.player,123));f.node[7]=0;
  f.node[1]=1;assert(!rebuild::idle(f.base,f.manager,f.player,123));f.node[1]=f.manager+0x80;
  f.put(f.manager+0xA0,1);assert(!rebuild::idle(f.base,f.manager,f.player,123));f.put(f.manager+0xA0,0);
  f.complete[0x130]=1;assert(!rebuild::idle(f.base,f.manager,f.player,123));f.complete[0x130]=0;
  f.put(f.manager+0x18,0);assert(!rebuild::idle(f.base,f.manager,f.player,123));f.put(f.manager+0x18,1);
  assert(rebuild::queue_ready(f.base,true));
  f.put(f.base+0xA37C118,1);assert(!rebuild::queue_ready(f.base,true));assert(rebuild::queue_ready(f.base,false));
  f.globals[0x158+0x5A]=0;assert(!rebuild::queue_ready(f.base,false));
  std::puts("PASS: synthetic idle manager, exact label, binding, pending replacement, list links and initialized empty queue checks. No allocation/enqueue/rebuild calls.");
}
