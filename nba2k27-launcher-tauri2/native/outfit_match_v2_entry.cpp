#include "outfit_match_v2_native.hpp"
// Isolated link target / production callable helper. Loading this DLL does not
// start a thread or touch the game. Only the trusted native provider may call
// this blocking worker API after retaining its launch identity and callbacks.
extern "C" __declspec(dllexport) unsigned OutfitMatchV2Run(
    const outfit_match_v2::TrustedBinding* binding,
    outfit_match_v2::Outcome* result) noexcept {
    if(!binding || !result) return 2;
    try {
        *result=outfit_match_v2::run_session(*binding);
        return result->state==outfit_match_v2::Completion::rebuilt?0:1;
    } catch(...) {
        // No retry and no cleanup through possibly uncertain native pointers.
        return 3;
    }
}
