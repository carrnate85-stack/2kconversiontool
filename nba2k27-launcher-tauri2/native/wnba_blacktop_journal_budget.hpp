#pragma once
#include <cstdint>
namespace wnba_blacktop_journal {
inline constexpr std::uint64_t kCapacity=256;
// The sole coordinator thread owns this journal sequence. Preflight includes
// ALL remaining forward receipts plus exact-owned cleanup, before any effect.
// Capacity is never evidence of memory ownership and never gates cleanup.
inline constexpr bool available(std::uint64_t used,std::uint64_t need) {
 return need && used<=kCapacity && need<=kCapacity-used;
}
inline constexpr bool roster(std::uint64_t used) {return available(used,4);}
inline constexpr bool category(std::uint64_t used,bool bridge_ready) {
 return available(used,bridge_ready?8:12);
}
}
