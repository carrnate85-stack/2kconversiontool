#include "character_queue_thread_observer.hpp"
#include <cassert>
#include <cstdio>
#include <cstring>
namespace obs=character_queue_thread_observer;
unsigned delegate_calls=0;
void *stock_destroy(void *node,unsigned flags) {
  assert(*static_cast<std::uintptr_t *>(node)==obs::stock_table);
  assert(flags==1);++delegate_calls;
  return node;
}
int main() {
  std::array<std::uintptr_t,21> stock{};
  stock[0]=123;stock[1]=reinterpret_cast<std::uintptr_t>(&stock_destroy);
  const auto vt=reinterpret_cast<std::uintptr_t>(stock.data()+1);
  std::array<std::uintptr_t,6> node{vt,0,0,0,0,0};
  assert(!obs::completed(GetCurrentThreadId()));
  assert(!obs::install(nullptr,vt,stock));
  assert(!obs::install(node.data(),vt+8,stock));
  assert(obs::install(node.data(),vt,stock));
  assert(stock[1]==reinterpret_cast<std::uintptr_t>(&stock_destroy));
  assert(obs::table[0]==123);
  assert(std::strcmp(obs::hook_state(),"UNCONFIRMED_CLOSE_GAME")==0);
  assert(!obs::install(node.data(),vt,stock));
  assert(!obs::completed(GetCurrentThreadId()));
  const auto callback=reinterpret_cast<obs::Destructor>(*reinterpret_cast<std::uintptr_t *>(node[0]));
  assert(callback(node.data(),1)==node.data());
  assert(node[0]==vt&&delegate_calls==1);
  assert(obs::completed(GetCurrentThreadId()));
  assert(!obs::completed(0));
  assert(!obs::completed(GetCurrentThreadId()+1));
  assert(std::strcmp(obs::hook_state(),"REMOVED_FROM_REQUEST")==0);
  obs::returned=0;assert(!obs::completed(GetCurrentThreadId()));obs::returned=1;
  obs::identity_ok=0;assert(!obs::completed(GetCurrentThreadId()));obs::identity_ok=1;
  obs::calls=2;assert(!obs::completed(GetCurrentThreadId()));obs::calls=1;
  obs::delete_flag=0;assert(!obs::completed(GetCurrentThreadId()));obs::delete_flag=1;
  assert(obs::completed(GetCurrentThreadId()));
  std::puts("PASS: one-shot callback, original delegation, vtable restoration, thread identity, and incomplete-evidence refusal. Synthetic node only.");
}
