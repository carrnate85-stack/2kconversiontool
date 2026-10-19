// POD-only Windows CAS. No C++ objects, allocation, game calls or remote writes.
// GNU clang requires -Xclang -fasync-exceptions so the actual atomic instruction
// has a real C_specific_handler range. Keep the rest of the DLL non-async.
#include <windows.h>
#include <cstdint>
extern "C" __declspec(noinline) bool c27_roster_appearance_cas32(std::uintptr_t at,std::uint32_t before,std::uint32_t after,std::uint32_t& observed) noexcept {
#if defined(_MSC_VER) || (defined(__clang__) && defined(_WIN32) && defined(C27_APPEARANCE_SEH))
 __try {observed=static_cast<std::uint32_t>(InterlockedCompareExchange(reinterpret_cast<volatile LONG*>(at),static_cast<LONG>(after),static_cast<LONG>(before)));return true;} __except(EXCEPTION_EXECUTE_HANDLER){return false;}
#else
 (void)at;(void)before;(void)after;(void)observed;return false;
#endif
}
