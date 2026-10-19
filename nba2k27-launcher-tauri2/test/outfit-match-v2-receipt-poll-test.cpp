#include "../native/outfit_match_v2_core.hpp"
#include <cassert>
#include <cstdint>
#include <iostream>
using namespace outfit_match_v2;
int main() {
 assert(receipt_poll_delay(1000,61000)==20);
 assert(receipt_poll_delay(60980,61000)==20);
 assert(receipt_poll_delay(60999,61000)==1);
 assert(receipt_poll_delay(61000,61000)==0);
 assert(receipt_poll_delay(61001,61000)==0);
 // Fake clock: retain the same full receipt predicate, alter only scheduling.
 // Queue drain alone never suffices: later actor/items/idle proof must pass.
 for(const auto ready: {1ull,19ull,20ull,21ull,99ull,101ull,157ull}) {
  std::uint64_t now=0;unsigned observations=0;
  while(now<60000) {
   ++observations;
   const bool drained=now>=1, new_actor=now>=ready, items=now>=ready, idle=now>=ready;
   if(drained&&new_actor&&items&&idle)break;
   const auto delay=receipt_poll_delay(now,60000);assert(delay>0&&delay<=20);now+=delay;
  }
  assert(now>=ready&&now-ready<20&&observations>1);
  const auto legacy=((ready+99)/100)*100;
  assert(now<=legacy);
 }
 // Never invent a receipt at deadline or oversleep its final bounded interval.
 std::uint64_t now=59991;unsigned waits=0;
 while(const auto delay=receipt_poll_delay(now,60000)) {now+=delay;++waits;}
 assert(now==60000&&waits==1);
 assert(receipt_poll_delay(0,0)==0);
 assert(receipt_poll_delay(0,~std::uint64_t(0))==20);
 std::cout<<"PASS bounded20ms receipt observation, unchanged predicate, no early acceptance, deadline/overflow boundaries\n";
}
