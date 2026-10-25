// Run the real native validators against a private image mapping. No loader,
// game entry point, imports, game process handle, or game-file writes.
#include "../native/roster_appearance_v2_win.hpp"
#include <iostream>
#include <stdexcept>

static void require(bool ok, const char* message) {
    if (!ok) throw std::runtime_error(message);
}
struct View {
    void* pointer{};
    ~View() { if (pointer) UnmapViewOfFile(pointer); }
};
static void change_byte(std::uintptr_t at, unsigned char byte) {
    DWORD protection{}, unused{};
    require(VirtualProtect(reinterpret_cast<void*>(at), 1, PAGE_WRITECOPY, &protection), "fixture protect");
    *reinterpret_cast<unsigned char*>(at) = byte;
    require(VirtualProtect(reinterpret_cast<void*>(at), 1, protection, &unused), "fixture restore protection");
}
int wmain(int argc, wchar_t** argv) try {
    using namespace outfit_match_v2;
    require(argc == 2, "Explicit game executable required");
    require(hash_executable(argv[1]), "Native executable identity/hash differs");
    Handle file(CreateFileW(argv[1], GENERIC_READ, FILE_SHARE_READ, nullptr, OPEN_EXISTING, 0, nullptr));
    require(bool(file), "Read-only fixture file open");
    Handle mapping(CreateFileMappingW(file.value, nullptr, PAGE_READONLY | SEC_IMAGE, 0, 0, nullptr));
    require(bool(mapping), "Image mapping");
    // An unconstrained mapping can land at the preferred base on some runs.
    // Request alternate addresses explicitly so ASLR coverage is deterministic.
    View view;
    for (const auto address : {0x20000000000ull, 0x30000000000ull}) {
        view.pointer = MapViewOfFileEx(mapping.value, FILE_MAP_READ, 0, 0, 0, reinterpret_cast<void*>(address));
        if (view.pointer) break;
    }
    require(view.pointer != nullptr, "Image view");
    const auto base = reinterpret_cast<std::uintptr_t>(view.pointer);
    const auto* dos = reinterpret_cast<const IMAGE_DOS_HEADER*>(base);
    const auto* nt = reinterpret_cast<const IMAGE_NT_HEADERS64*>(base + dos->e_lfanew);
    const auto preferred = nt->OptionalHeader.ImageBase;
    require(base != preferred, "Fixture must exercise ASLR");
    const auto size = nt->OptionalHeader.SizeOfImage;
    const auto& dir = nt->OptionalHeader.DataDirectory[IMAGE_DIRECTORY_ENTRY_BASERELOC];
    require(dir.Size && dir.VirtualAddress < size && dir.Size <= size - dir.VirtualAddress, "Relocation directory");
    // SEC_IMAGE mappings do not run the Windows loader. Accept either OS
    // relocation behavior, but never relocate a pointer twice.
    const auto root = q(base + kRootTag);
    require(root == preferred + 0x23120 || root == base + 0x23120, "Unrecognized mapping relocation state");
    std::size_t relocated = 0;
    if (root == preferred + 0x23120) {
        for (std::size_t cursor = 0; cursor < dir.Size;) {
            require(cursor + sizeof(IMAGE_BASE_RELOCATION) <= dir.Size, "Truncated relocation block");
            const auto* block = reinterpret_cast<const IMAGE_BASE_RELOCATION*>(base + dir.VirtualAddress + cursor);
            const auto length = block->SizeOfBlock;
            require(length >= 8 && length % 2 == 0 && length <= dir.Size - cursor, "Relocation block size");
            const auto* entries = reinterpret_cast<const WORD*>(block + 1);
            for (std::size_t i = 0; i < (length - 8) / 2; ++i) {
                const auto entry = entries[i];
                if (!(entry >> 12)) continue;
                require((entry >> 12) == IMAGE_REL_BASED_DIR64, "Unexpected relocation type");
                const auto rva = std::uint64_t(block->VirtualAddress) + (entry & 0xfff);
                require(rva <= size - 8, "Relocation outside image");
                auto* slot = reinterpret_cast<void*>(base + rva);
                DWORD protection{}, unused{};
                require(VirtualProtect(slot, 8, PAGE_WRITECOPY, &protection), "Relocation protection");
                std::uint64_t value{};
                std::memcpy(&value, slot, 8);
                value += base - preferred;
                std::memcpy(slot, &value, 8);
                require(VirtualProtect(slot, 8, protection, &unused), "Relocation protection restore");
                ++relocated;
            }
            cursor += length;
        }
    }
    const bool authority = outfit_match_v2::profile(base);
    const bool appearance = roster_appearance_v2::win_detail::profile(base);
    std::cout << "Native authority profile: " << authority << "\nNative measurement profile: " << appearance
              << "\nFixture base: " << std::hex << base << std::dec << "; relocated slots: " << relocated << '\n';
    require(authority && appearance, "Native runtime profile differs");
    // Both validators must still reject a changed method, then accept it after
    // restoring the fixture. These are copy-on-write test pages only.
    const auto a = base + kMethods[0].rva;
    const auto original_a = *reinterpret_cast<const unsigned char*>(a);
    change_byte(a, original_a ^ 1);
    const bool reject_a = !outfit_match_v2::profile(base);
    change_byte(a, original_a);
    const auto b = base + roster_appearance_v2::win_detail::methods[0].rva;
    const auto original_b = *reinterpret_cast<const unsigned char*>(b);
    change_byte(b, original_b ^ 1);
    const bool reject_b = !roster_appearance_v2::win_detail::profile(base);
    change_byte(b, original_b);
    require(reject_a && reject_b && outfit_match_v2::profile(base) && roster_appearance_v2::win_detail::profile(base),
            "Native fingerprint rejection/restoration failed");
    std::cout << "PASS: native authority, measurements, ASLR and changed-byte rejection; game processes/files unchanged\n";
    return 0;
} catch (const std::exception& e) {
    std::cerr << "FAIL: " << e.what() << " (Windows error " << GetLastError() << ")\n";
    return 1;
}
