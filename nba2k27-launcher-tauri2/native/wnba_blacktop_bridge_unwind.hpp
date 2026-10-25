#pragma once
#include <array>
#include <cstdint>

// A bridge is reached after the stock switch has already pushed RSI/RDI/RBX
// and reserved32bytes. It therefore needs a complete established-frame unwind
// at byte0, not the original function's partially executed prologue offsets.
// Microsoft CHAININFO describes a noncontiguous fragment sharing the original
// procedure frame. A zero-prologue clone fails at E9 (epilogue detection), while
// this chain passes every bridge byte in the offline Windows fixture.
// https://learn.microsoft.com/en-us/cpp/build/exception-handling-x64
namespace wnba_blacktop_bridge_unwind {
inline constexpr std::array<std::uint8_t,12> kZeroPrologueNegativeControl{
    0x01,0x00,0x04,0x00,0x00,0x32,0x00,0x30,0x00,0x70,0x00,0x60
};
inline constexpr std::array<std::uint8_t,12> kStockFrame{
    0x01,0x07,0x04,0x00,0x07,0x32,0x03,0x30,0x02,0x70,0x01,0x60
};
inline constexpr std::uintptr_t kStockUnwindRva=0x85CDBBC;
// Exact static .pdata entry, disk-derived and independently read back from the
// owned live V2 image. This image maps that section READONLY (0x02).
inline constexpr std::uintptr_t kStockFunctionRva=0xC0AC13C;
inline constexpr std::array<std::uint8_t,16> kChainInfo{
    0x21,0x00,0x00,0x00, // Version1, CHAININFO, no local prologue/codes/frame.
    0x80,0xBE,0x30,0x02, // Original RUNTIME_FUNCTION BeginAddress230BE80.
    0x7B,0xBF,0x30,0x02, // EndAddress230BF7B (exclusive).
    0xBC,0xDB,0x5C,0x08  // UnwindData85CDBBC, all relative to game image base.
};
} // namespace wnba_blacktop_bridge_unwind
