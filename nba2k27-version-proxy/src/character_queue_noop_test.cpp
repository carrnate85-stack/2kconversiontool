#include "character_queue_noop_policy.hpp"
#include <cassert>
#include <cstdio>
int main() {
  using namespace character_queue_noop;
  Preconditions good{true,true,true,true,true,true,true,true,true,false};
  assert(may_submit(good));
  bool Preconditions::*fields[]={&Preconditions::exact_build,&Preconditions::native_code,
    &Preconditions::anti_cheat_absent,&Preconditions::in_blacktop,&Preconditions::worker_thread,
    &Preconditions::queue_class,&Preconditions::lock_initialized,&Preconditions::heap_ready,
    &Preconditions::queue_empty};
  for(auto field:fields){auto bad=good;bad.*field=false;assert(!may_submit(bad));}
  good.already_submitted=true;assert(!may_submit(good));
  assert(packet_is_noop(0,0,0));assert(!packet_is_noop(1,0,0));
  assert(!packet_is_noop(0,1,0));assert(!packet_is_noop(0,0,1));
  assert(may_report_dequeued(true,true,true,false));
  assert(!may_report_dequeued(false,true,true,false));
  assert(!may_report_dequeued(true,false,true,false));
  assert(!may_report_dequeued(true,true,false,false));
  assert(!may_report_dequeued(true,true,true,true));
  std::puts("PASS: no-op policy guard matrix, null packet, repeat refusal, and dequeue evidence checks. No game calls.");
}
