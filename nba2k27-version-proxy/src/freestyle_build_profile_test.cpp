#include "freestyle_build_profile.hpp"
#include <cassert>
int main(){
  using nba2k27::freestyle_build_profile;
  const auto* old=freestyle_build_profile(0x6A974D13,0x368C3000);
  const auto* current=freestyle_build_profile(0x6AA2013D,0x36558000);
  assert(old&&current&&old!=current);
  assert(old->anchor==0x25984e&&old->event==0xA2908F0&&old->keys==0x67184a0);
  assert(current->anchor==0x25994e&&current->event==0xA2948F0&&current->keys==0x671b4c0);
  assert(!freestyle_build_profile(0x6A974D13,0x36558000));
  assert(!freestyle_build_profile(0x6AA2013D,0x368C3000));
  assert(!freestyle_build_profile(0,0));
}
