#pragma once
// Pure bounded label resolution only. No process API or address adoption.
// Native lookup3045970 binary-searches647 unsigned IDs at ACFA610, stride16.
// Native sort3045CE0 sorts those records before setting ready byte ACFD610.
// Initializer3119780 writes Euro into an UNSORTED slot; that slot is not stable.
#include <cstddef>
#include <cstdint>
#include <optional>
#include <span>

namespace wnba_blacktop_label_profile {
inline constexpr std::uintptr_t kArrayRva=0xACFA610,kReadyRva=0xACFD610;
inline constexpr std::size_t kCount=647,kStride=16,kBytes=kCount*kStride;
inline constexpr std::uint32_t kEuroId=0xBA3C982D;
inline constexpr std::uint64_t kEuro=0x6FC058B0442652CD;
inline constexpr std::uint32_t kId=kEuroId;
inline constexpr std::uint64_t kWomen=0x0F39A44C3426C962;
struct Method {std::uintptr_t rva;std::size_t bytes;const char* hash;};
inline constexpr Method kMethods[]={
 {0x3045970,113,"C517E5EDE4831A689C9CADD8ACF49526839E57466F9B5230DBF2803F04A0B228"},
 {0x3045CE0,118,"17A62562A8F48FABFDC1B7D13E069D227D462FB73019AC8385FFF7058876EE8A"},
 {0x3045D60,13,"035D7061D87454287CB59A9844570ADBC5E87194DBE64EB3C71C7574E8588D5F"},
};
struct Resolved {
 std::size_t index{};
 std::uintptr_t record_rva{},key_rva{};
 bool operator==(const Resolved&)const=default;
};
inline std::uint32_t u32(std::span<const std::uint8_t> data,std::size_t at) {
 std::uint32_t result{};
 for(unsigned n=0;n<4;++n)result|=std::uint32_t(data[at+n])<<(n*8);
 return result;
}
inline std::uint64_t u64(std::span<const std::uint8_t> data,std::size_t at) {
 std::uint64_t result{};
 for(unsigned n=0;n<8;++n)result|=std::uint64_t(data[at+n])<<(n*8);
 return result;
}
// Initial baseline only: the caller must pin the entire fixed image array RW,
// the three native methods RX, exact process/image identity, and both reads.
// No lookup result grants write authority. Retain this exact address in the
// ownership journal; never re-resolve/adopt a changed record during restore.
inline std::optional<Resolved> resolve_euro(std::span<const std::uint8_t> first,
 std::uint8_t ready_before,std::span<const std::uint8_t> second,std::uint8_t ready_after) {
 if(ready_before!=1 || ready_after!=1 || first.size()!=kBytes || second.size()!=kBytes)return {};
 for(std::size_t n=0;n<kBytes;++n)if(first[n]!=second[n])return {};
 std::uint32_t previous{};
 for(std::size_t n=0;n<kCount;++n) {
  const auto id=u32(first,n*kStride);
  if(n && id<=previous)return {};
  previous=id;
 }
 // Match native unsigned-ID binary-search semantics, bounded by fixed count.
 std::size_t lo{},hi=kCount;
 while(lo<hi) {
  const auto mid=lo+(hi-lo)/2;
  const auto id=u32(first,mid*kStride);
  if(id<kEuroId)lo=mid+1;
  else if(id>kEuroId)hi=mid;
  else {
   const auto at=mid*kStride;
   if(u32(first,at+4) || u64(first,at+8)!=kEuro)return {};
   return Resolved{mid,kArrayRva+at,kArrayRva+at+8};
  }
 }
 return {};
}
} // namespace wnba_blacktop_label_profile
