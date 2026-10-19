#include "freestyle_build_profile_v2.hpp"
#include <cassert>
#include <cstdint>
#include <cstring>
int main(){
  using nba2k27::freestyle_build_profile;
  const auto* old=freestyle_build_profile(0x6A974D13,0x368C3000);
  const auto* sep11=freestyle_build_profile(0x6AA2013D,0x36558000);
  const auto* v2=freestyle_build_profile(0x6AC53765,0x35BDE000);
  assert(old&&old->anchor==0x25984e&&old->event==0xA2908F0&&old->keys==0x67184a0);
  assert(sep11&&sep11->anchor==0x25994e&&sep11->event==0xA2948F0&&sep11->keys==0x671b4c0);
  assert(v2&&v2->anchor==0x25C4A7&&v2->event==0xA31E7D0&&v2->keys==0x677F930);
  // The anchor is lea rcx,[rip+disp32] and must resolve to the event object.
  const unsigned char lea[3]={0x48,0x8d,0x0d};
  for(const auto* p:{old,sep11,v2}){
    assert(std::memcmp(p->bytes.data(),lea,3)==0);
    std::uint32_t disp=0;std::memcpy(&disp,p->bytes.data()+3,4);
    assert(p->anchor+7+disp==p->event);
  }
  // Cross-build tuples never match.
  assert(!freestyle_build_profile(0x6AC53765,0x368C3000));
  assert(!freestyle_build_profile(0x6AC53765,0x36558000));
  assert(!freestyle_build_profile(0x6A974D13,0x35BDE000));
  assert(!freestyle_build_profile(0x6AA2013D,0x35BDE000));
  assert(!freestyle_build_profile(0,0));
  return 0;
}
