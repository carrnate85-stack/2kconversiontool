#pragma once
// Pure immutable disk/code profile. No process, protection, call, or write APIs.
// A native game-thread InstallDelegate invocation may use the General Learn2K
// leaf only through the exact Blacktop +D30 callback; this is not +D10 and is
// not the withdrawn Freestyle general->WNBA callback substitution.
#include <array>
#include <cstddef>
#include <cstdint>
#include <span>

namespace wnba_blacktop_roster_profile {
// Loader DIR64 relocation of the file-backed static delegate may retain
// PAGE_WRITECOPY for its data page. This is never a code or callback-table rule.
inline constexpr bool delegate_image_metadata(std::uint32_t kind,std::uint32_t protection,
 bool committed,bool same_image_allocation) {
 return committed && same_image_allocation && kind==0x1000000u &&
  (protection==0x02u || protection==0x04u || protection==0x08u);
}
inline constexpr std::uintptr_t kVtable=0x74CD158,kDelegate=0x9B31BB0;
inline constexpr std::uintptr_t kOffset=0xD30,kSlot=kVtable+kOffset;
inline constexpr std::uintptr_t kStock=0x236E5C0,kGeneral=0x236FBE0,kReset=0x2F1E160;
inline constexpr std::uintptr_t kAssetRva=0x74E079A;
inline constexpr char kAsset[]="roster/learn2k_roster.iff";
// PlayNow_InitGameMode tail LEA proves STATIC image object, not a pointer slot.
inline constexpr std::uintptr_t kManager=0xAB160D8,kInstalledOffset=0x10;
inline constexpr std::uintptr_t kInstalledDelegateRva=kManager+kInstalledOffset;
inline constexpr std::uintptr_t kRequestedDelegateRva=0xAB16318,kPlayNowDelegateRva=0xAB3CE38;
inline constexpr std::uintptr_t kInstaller=0x222AB70,kOldExitCall=0x222ACE0,kNewEntryCall=0x222ACF0;
inline constexpr std::uintptr_t kSameDelegateCompare=0x222AB97,kInstalledStore=0x222ACE6;
inline constexpr std::uintptr_t kCallsiteRva=0x222ACD2,kManagerCallsiteRva=0x236F0C5;
inline constexpr std::size_t kNeighborOffset=0xCF0,kVtablePrefixBytes=0xD58;
// Little-endian uint64 RVAs, slots[0..0xD50], normalize only an exactly OWNED
// General callback at +D30 back to kStock before hashing. Never adopt foreign.
inline constexpr char kNormalizedVtableSha256[]="0D7E644D4740EE9BDEA231461D2DB50B7C9F852A724CCF5219F982E43B1EF16C";
struct Method {std::uintptr_t rva;std::size_t bytes;const char* hash;};
inline constexpr Method kMethods[]={
 {0x222AB70,616,"F677FBAD3074C2EE299114AF3929D5957637CA14DB0BDEF417DCA1CFD9FAEF52"},
 {0x236EBE0,1285,"ED3ED3E7DA115AB5ED0BDD3DCD1FD0681D289976D57FE91F77DDD8D87E943B13"},
 {0x32FF650,208,"F93BEDFE8FE2AA534E9263D70938FBEC434D3B65DB5FA18BEC2115121E3B0C22"},
 {0x2F1E160,536,"633AF390781FC291AE7FB479F355B155E534C2EB25BD5B4DFC12BBD5DCCAC14C"},
 {0x2F1E410,432,"1BCA4E9D13B1C5FA37BFD85D6B0012EAF8B517BF4B6F3970C90EE7FD3EFE49C5"},
 {0x236E4F0,198,"7B1C91E9611703E6901DD176EB6032F678E0779A0AEC81F77DAFDEFAD5976F48"},
 {kGeneral,26,"B5424DA355D48CC2FCD4CB8F2BD2C484643CDBFBEF474ACA2C715A695C5B125B"},
 {kStock,1,"AE3F4619B0413D70D3004B9131C3752153074E45725BE13B9A148978895E359E"},
 {0x2350CA0,1,"AE3F4619B0413D70D3004B9131C3752153074E45725BE13B9A148978895E359E"},
};
// These stock RET leaves have no .pdata record; no replacement unwind is
// created because callback target is an existing, unchanged native image leaf.
inline constexpr bool kStockRetHasUnwind=false;
inline constexpr std::array<std::uintptr_t,13> kNeighbors={
 0x236DD20,0x236DDE0,0x236E020,0x2350910,0x2353250,0x2353270,0x2353260,
 0x236E4F0,kStock,0x2350CA0,0x2350CB0,0x2353D40,0x234C5E0};
inline constexpr std::array<std::uint8_t,26> kGeneralLeaf={
 0x48,0x8D,0x0D,0xB3,0x0B,0x17,0x05,0xB2,0x01,0x41,0xB8,0xD2,0xB8,
 0x9C,0x50,0x41,0xB9,0x2F,0,0,0,0xE9,0x66,0xE5,0xBA,0};
inline constexpr std::array<std::uint8_t,36> kCallsite={
 0x48,0x8B,0x4E,0x10,0x48,0x85,0xC9,0x74,0x0B,0x48,0x8B,0x01,
 0x89,0xDA,0xFF,0x90,0x38,0x0D,0,0,0x48,0x89,0x7E,0x10,0x48,0x8B,
 0x07,0x48,0x89,0xF9,0xFF,0x90,0x30,0x0D,0,0};
inline constexpr std::array<std::uint8_t,32> kManagerCallsite={
 0x48,0x8B,0x15,0x4C,0x72,0x7A,0x08,0x48,0x8D,0x0D,0x05,0x70,0x7A,
 0x08,0x45,0x31,0xC0,0x48,0x81,0xC4,0xB0,0,0,0,0x5B,0x5F,0x5E,
 0xE9,0x8B,0xBA,0xEB,0xFF};
template<std::size_t N> inline bool bytes_equal(std::span<const std::uint8_t> actual,
 const std::array<std::uint8_t,N>& expected) {
 if(actual.size()!=N)return false;
 for(std::size_t i=0;i<N;++i)if(actual[i]!=expected[i])return false;
 return true;
}
inline std::int32_t signed32(std::span<const std::uint8_t> bytes,std::size_t at) {
 std::uint32_t value{};
 for(unsigned n=0;n<4;++n)value|=std::uint32_t(bytes[at+n])<<(8*n);
 return static_cast<std::int32_t>(value);
}
inline bool general_leaf_matches(std::span<const std::uint8_t> code,
 std::span<const std::uint8_t> asset) {
 if(!bytes_equal(code,kGeneralLeaf) || asset.size()!=sizeof(kAsset))return false;
 for(std::size_t n=0;n<asset.size();++n)if(asset[n]!=std::uint8_t(kAsset[n]))return false;
 return std::int64_t(kGeneral+7)+signed32(code,3)==std::int64_t(kAssetRva) &&
  std::int64_t(kGeneral+26)+signed32(code,22)==std::int64_t(kReset);
}
inline bool neighbors_match(std::span<const std::uintptr_t> rvas,bool owns_general=false) {
 if(rvas.size()!=kNeighbors.size())return false;
 for(std::size_t n=0;n<rvas.size();++n) {
  if(n==8 && owns_general && rvas[n]==kGeneral)continue;
  if(rvas[n]!=kNeighbors[n])return false;
 }
 return true;
}
} // namespace wnba_blacktop_roster_profile
