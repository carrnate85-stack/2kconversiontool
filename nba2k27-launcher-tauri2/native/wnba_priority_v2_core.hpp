#pragma once
#include <cstdint>
#include <cstddef>
#include <cstring>
#include <string>
#include <vector>

// The native WAD priority API owns its allocations and all file handles after a
// successful registration. Fast-load locks retain raw manifest/entry pointers;
// the unregister API does not drain those locks. A published WNBA WAD must stay
// owned by the native manager until game exit, including launcher/pipe loss.
// This controller has no Windows APIs, game-call bindings, addresses, or writes.
namespace wnba_priority_v2 {
struct PackProfile {
    const wchar_t* folder;
    std::size_t index_bytes;
    const char* index_sha256;
    std::uint32_t entries;
};
inline constexpr PackProfile kFreestylePack{L"qa058-wpf-era",120,
    "6630BEDCBF708145FDFB55A48DE467265F3DE29CD76081E9C24A1C8EFD7B84BD",1};
inline constexpr PackProfile kBlacktopSourcePack{L"qa064-blacktop-source",144,
    "0C25D0839B11B4AF085F587C1BAF4E74F821CD34D2B902FC950DBC91E5123CC9",2};
inline constexpr const PackProfile& pack_profile(bool blacktop_source_trial) {
    return blacktop_source_trial?kBlacktopSourcePack:kFreestylePack;
}
struct MenuSnapshot {
    std::uint32_t current{},alternate{},stage{},active{},kind{};
};
// WPF WNBA arm boundary: Home stages0/1 or Quick Play stage1, with
// both mode slots matching and no active match/owner kind. The outfit
// carry menu predicate intentionally differs and must not be reused here.
inline bool inactive_frontend(const MenuSnapshot& c) {
    return c.current==c.alternate && c.active==0 && c.kind==0 &&
        ((c.current==0x6509837B && c.stage<=1) ||
         (c.current==0x079964C0 && c.stage==1));
}
inline bool approval_matches_profile(const std::vector<unsigned char>& data,std::uint32_t pid,
                             std::uint64_t birth,const std::string& session,bool blacktop_source_trial) {
    if(data.size()!=72 || session.size()!=48 || !pid || !birth ||
        std::memcmp(data.data(),blacktop_source_trial?"C27WNBT1":"C27WNPA1",8)!=0) return false;
    std::uint32_t p{},reserved{};std::uint64_t b{};
    std::memcpy(&p,data.data()+8,4);std::memcpy(&reserved,data.data()+12,4);
    std::memcpy(&b,data.data()+16,8);
    for(char c:session) if(!((c>='0'&&c<='9')||(c>='a'&&c<='f')||(c>='A'&&c<='F'))) return false;
    return p==pid && b==birth && !reserved && std::memcmp(data.data()+24,session.data(),48)==0;
}
inline bool approval_matches(const std::vector<unsigned char>& data,std::uint32_t pid,
                             std::uint64_t birth,const std::string& session) {
    return approval_matches_profile(data,pid,birth,session,false);
}
inline bool blacktop_approval_matches(const std::vector<unsigned char>& data,std::uint32_t pid,
                             std::uint64_t birth,const std::string& session) {
    return approval_matches_profile(data,pid,birth,session,true);
}
enum class Phase { idle, native_owned, ended, blocked };
enum class Result { published, already_published, waiting, conflict, invalid, failed, ended };

struct Admission {
    bool exact_build{};
    bool owned_offline_session{};
    bool inactive_frontend{};
    bool verified_private_pack{};
    bool manager_ready{};
    bool live{};
    std::uint64_t identity{};
};

class Session {
    Phase phase_{Phase::idle};
    std::uint64_t identity_{};
    std::uintptr_t published_{};
public:
    Phase phase() const noexcept { return phase_; }
    std::uintptr_t published() const noexcept { return published_; }

    // Host contract: lock/unlock acquire the native recursive priority mutex.
    // The final check AND native registration run under that same mutex. Host
    // must perform an exact retained-identity/profile/boundary check again in
    // checkpoint(); no cached "ready" observation authorizes publication.
    // native_register() must call the native API, never write manager fields.
    template<class Host> Result publish(const Admission& a, Host& h) {
        if (phase_ == Phase::ended) return Result::ended;
        if (!a.live) { phase_ = Phase::ended; return Result::ended; }
        if (phase_ == Phase::blocked) return Result::conflict;
        if (!a.exact_build || !a.owned_offline_session || !a.verified_private_pack || !a.identity) {
            phase_ = Phase::blocked; return Result::invalid;
        }
        if (identity_ && identity_ != a.identity) {
            phase_ = Phase::blocked; return Result::conflict;
        }
        if (!a.manager_ready || !a.inactive_frontend) return Result::waiting;
        if (!h.lock()) return Result::waiting;
        struct Unlock { Host& h; ~Unlock() { h.unlock(); } } unlock{h};
        if (!h.checkpoint(a.identity)) return Result::waiting;
        const auto current = h.priority();
        if (phase_ == Phase::native_owned) {
            if (current == published_) return Result::already_published;
            phase_ = Phase::blocked; return Result::conflict;
        }
        // Native register destroys an existing priority manifest, so this must
        // be checked while holding its mutex. Never replace a foreign backend.
        if (current) { phase_ = Phase::blocked; return Result::conflict; }
        identity_ = a.identity;
        if (!h.native_register()) {
            phase_ = Phase::blocked; return Result::failed;
        }
        published_ = h.priority();
        if (!published_ || !h.is_native_wad(published_)) {
            phase_ = Phase::blocked; return Result::failed;
        }
        phase_ = Phase::native_owned;
        return Result::published;
    }

    // Intentionally no unregister/destructor callback. Native manager owns the
    // pack until process exit. The caller must retain the immutable portable
    // pack files and may clean them only after this exact game identity exits.
    void game_ended(std::uint64_t identity) noexcept {
        if (identity_ && identity_ != identity) return;
        phase_ = Phase::ended;
    }
};
} // namespace wnba_priority_v2
