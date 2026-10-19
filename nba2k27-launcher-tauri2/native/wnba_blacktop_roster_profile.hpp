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
inline constexpr std::uintptr_t kStock=0x236E510,kGeneral=0x236FB30,kReset=0x2F1E0B0;
inline constexpr std::uintptr_t kAssetRva=0x74E079A;
inline constexpr char kAsset[]="roster/learn2k_roster.iff";
// PlayNow_InitGameMode tail LEA proves STATIC image object, not a pointer slot.
inline constexpr std::uintptr_t kManager=0xAB160D8,kInstalledOffset=0x10;
inline constexpr std::uintptr_t kInstalledDelegateRva=kManager+kInstalledOffset;
inline constexpr std::uintptr_t kRequestedDelegateRva=0xAB16318,kPlayNowDelegateRva=0xAB3CE38;
inline constexpr std::uintptr_t kInstaller=0x222AAC0,kOldExitCall=0x222AC30,kNewEntryCall=0x222AC40;
inline constexpr std::uintptr_t kSameDelegateCompare=0x222AAE7,kInstalledStore=0x222AC36;
inline constexpr std::uintptr_t kCallsiteRva=0x222AC22,kManagerCallsiteRva=0x236F015;
inline constexpr std::size_t kNeighborOffset=0xCF0,kVtablePrefixBytes=0xD58;
// Little-endian uint64 RVAs, slots[0..0xD50], normalize only an exactly OWNED
// General callback at +D30 back to kStock before hashing. Never adopt foreign.
inline constexpr char kNormalizedVtableSha256[]="B786736A6D493C74B2083B78DA337CE8E8CC07661488A172AFFDC640BC72D983";
struct Method {std::uintptr_t rva;std::size_t bytes;const char* hash;};
inline constexpr Method kMethods[]={
 {0x222AAC0,616,"EF7EAF08DBBAAEA429A630617B50A09A6AB960B751D5460927919A8A31C34304"},
 {0x236EB30,1285,"F58D397178F61D7CBDE0E6059EEF9C5BBC8BBDE7D8799FB594F69A4F90C4627B"},
 {0x32FF5A0,208,"9FA9E2B36AFC4737DB89381A669FBC57E909619D1E7DFCA1D8AC2312196D1457"},
 {0x2F1E0B0,536,"00FEEDC69CBCB51B65D70047C84AD4B43C50E5DBDFD0E7E641F8B1A6E6075B45"},
 {0x2F1E360,432,"B36DEF76D979613A797A0ECED5451AAFCC9203674C1A3044DC94A507DE8D46BF"},
 {0x236E440,198,"BCE8FE831092C3F2157D837B81A5B12AF9B1C88F587A1EC8F19D7AB825EF2E80"},
 {kGeneral,26,"360DEB306FA92D1B7BA00C8E4057BE6F404742B84721B4506CBEC2C6F55820F6"},
 {kStock,1,"AE3F4619B0413D70D3004B9131C3752153074E45725BE13B9A148978895E359E"},
 {0x2350BF0,1,"AE3F4619B0413D70D3004B9131C3752153074E45725BE13B9A148978895E359E"},
};
// These stock RET leaves have no .pdata record; no replacement unwind is
// created because callback target is an existing, unchanged native image leaf.
inline constexpr bool kStockRetHasUnwind=false;
inline constexpr std::array<std::uintptr_t,13> kNeighbors={
 0x236DC70,0x236DD30,0x236DF70,0x2350860,0x23531A0,0x23531C0,0x23531B0,
 0x236E440,kStock,0x2350BF0,0x2350C00,0x2353C90,0x234C530};
inline constexpr std::array<std::uint8_t,26> kGeneralLeaf={
 0x48,0x8D,0x0D,0x63,0x0C,0x17,0x05,0xB2,0x01,0x41,0xB8,0xD2,0xB8,
 0x9C,0x50,0x41,0xB9,0x2F,0,0,0,0xE9,0x66,0xE5,0xBA,0};
inline constexpr std::array<std::uint8_t,36> kCallsite={
 0x48,0x8B,0x4E,0x10,0x48,0x85,0xC9,0x74,0x0B,0x48,0x8B,0x01,
 0x89,0xDA,0xFF,0x90,0x38,0x0D,0,0,0x48,0x89,0x7E,0x10,0x48,0x8B,
 0x07,0x48,0x89,0xF9,0xFF,0x90,0x30,0x0D,0,0};
inline constexpr std::array<std::uint8_t,32> kManagerCallsite={
 0x48,0x8B,0x15,0xFC,0x72,0x7A,0x08,0x48,0x8D,0x0D,0xB5,0x70,0x7A,
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
