#pragma once
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
  if(timestamp==0x6A974D13 && image_size==0x368C3000)return &original;
  if(timestamp==0x6AA2013D && image_size==0x36558000)return &september11;
  return nullptr;
}
}
