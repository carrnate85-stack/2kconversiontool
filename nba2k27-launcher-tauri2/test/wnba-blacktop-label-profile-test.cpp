#include "../native/wnba_blacktop_label_profile.hpp"
#include <array>
#include <cassert>
#include <iostream>
using namespace wnba_blacktop_label_profile;
using Bytes=std::array<std::uint8_t,kBytes>;
void put(Bytes& bytes,std::size_t at,std::uint64_t value,unsigned count) {
 for(unsigned n=0;n<count;++n)bytes[at+n]=std::uint8_t(value>>(n*8));
}
Bytes fixture(std::size_t euro_index) {
 Bytes out{};
 for(std::size_t n=0;n<kCount;++n) {
  const auto id=n<euro_index?std::uint32_t(n+1):n==euro_index?kEuroId:kEuroId+std::uint32_t(n-euro_index);
  put(out,n*kStride,id,4);
  put(out,n*kStride+8,n==euro_index?kEuro:std::uint64_t(n+1),8);
 }
 return out;
}
int main() {
 static_assert(kBytes==10352 && kArrayRva+kBytes==0xACFCE80);
 for(const auto index:{std::size_t(0),std::size_t(321),kCount-1}) {
  auto data=fixture(index);
  const auto result=resolve_euro(data,1,data,1);
  assert(result && result->index==index && result->record_rva==kArrayRva+16*index && result->key_rva==kArrayRva+16*index+8);
 }
 auto data=fixture(321);
 assert(!resolve_euro(data,0,data,1) && !resolve_euro(data,1,data,0) && !resolve_euro(data,2,data,1));
 assert(!resolve_euro(std::span(data).first(kBytes-1),1,data,1));
 assert(!resolve_euro(data,1,std::span(data).first(kBytes-1),1));
 auto changed=data;changed.back()^=1;
 assert(!resolve_euro(data,1,changed,1));
 auto duplicate=data;put(duplicate,320*kStride,kEuroId,4);
 assert(!resolve_euro(duplicate,1,duplicate,1));
 auto unsorted=data;put(unsorted,400*kStride,1,4);
 assert(!resolve_euro(unsorted,1,unsorted,1));
 auto reserved=data;put(reserved,321*kStride+4,1,4);
 assert(!resolve_euro(reserved,1,reserved,1));
 auto foreign=data;put(foreign,321*kStride+8,0x0F39A44C3426C962,8);
 assert(!resolve_euro(foreign,1,foreign,1)); // Never adopt an existing Women label.
 auto absent=fixture(kCount-1);put(absent,(kCount-1)*kStride,kEuroId-1,4);
 assert(!resolve_euro(absent,1,absent,1));
 std::cout<<"Blacktop label profile sorted lookup/default-deny tests passed\n";
}
