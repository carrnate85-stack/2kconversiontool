#pragma once
// Rust-owned copy of WPF nba2k27-version-proxy\src\freestyle_build_profile.hpp
// (left frozen) plus the Version 2 tuple. The V2 proxy includes this header
// instead of the WPF one; the redirect itself is unchanged and read-only.
// V2 evidence: artifacts\jukebox-v2-freestyle-cover-research-20261017.md and
// tools\jukebox_v2_freestyle_cover_audit.py (anchor = lea rcx,[0xA31E7D0];
// event +0x14C bank 34 / +0x44 flag; keys[992] = 0xFAB3211A).
#include <cstdint>
#include <array>

namespace nba2k27 {
struct FreestyleBuildProfile {
  std::uint32_t anchor, event, keys;
  std::array<unsigned char,7> bytes;
};
inline const FreestyleBuildProfile* freestyle_build_profile(std::uint32_t timestamp,
                                                          std::uint32_t image_size) {
  static constexpr FreestyleBuildProfile original={0x25984e,0xA2908F0,0x67184a0,
    {0x48,0x8d,0x0d,0x9b,0x70,0x03,0x0a}};
  static constexpr FreestyleBuildProfile september11={0x25994e,0xA2948F0,0x671b4c0,
    {0x48,0x8d,0x0d,0x9b,0xaf,0x03,0x0a}};
  static constexpr FreestyleBuildProfile version2={0x25C4A7,0xA31E7D0,0x677F930,
    {0x48,0x8d,0x0d,0x22,0x23,0x0c,0x0a}};
  if(timestamp==0x6A974D13 && image_size==0x368C3000)return &original;
  if(timestamp==0x6AA2013D && image_size==0x36558000)return &september11;
  if(timestamp==0x6AC53765 && image_size==0x35BDE000)return &version2;
  return nullptr;
}
}
