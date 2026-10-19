#include <Windows.h>
#include <TlHelp32.h>

#include <array>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <vector>

namespace {
HANDLE process_handle = nullptr;
bool read_memory(std::uintptr_t at, void *out, std::size_t size) {
  SIZE_T done = 0;
  return at >= 0x10000 && ReadProcessMemory(process_handle,
      reinterpret_cast<const void *>(at), out, size, &done) && done == size;
}
template <class T> bool read_value(std::uintptr_t at, T &out) {
  return read_memory(at, &out, sizeof(out));
}
template <class T> T value(std::uintptr_t at) { T out{}; read_value(at, out); return out; }
bool write_memory(std::uintptr_t at, const void *data, std::size_t size) {
  SIZE_T done = 0;
  return WriteProcessMemory(process_handle, reinterpret_cast<void *>(at), data, size, &done) &&
         done == size;
}
bool supported_build(std::uintptr_t base) {
  IMAGE_DOS_HEADER dos{}; IMAGE_NT_HEADERS64 nt{};
  return read_value(base, dos) && dos.e_magic == IMAGE_DOS_SIGNATURE && dos.e_lfanew > 0 &&
    dos.e_lfanew < 0x1000 && read_value(base + dos.e_lfanew, nt) &&
    nt.Signature == IMAGE_NT_SIGNATURE && nt.FileHeader.Machine == IMAGE_FILE_MACHINE_AMD64 &&
    nt.FileHeader.TimeDateStamp == 0x6AA2013D && nt.OptionalHeader.SizeOfImage == 0x36558000;
}
bool process_identity(std::uint64_t expected) {
  FILETIME created{}, exited{}, kernel{}, user{}; ULARGE_INTEGER stamp{};
  if (!GetProcessTimes(process_handle, &created, &exited, &kernel, &user)) return false;
  stamp.LowPart = created.dwLowDateTime; stamp.HighPart = created.dwHighDateTime;
  return stamp.QuadPart == expected && !exited.dwLowDateTime && !exited.dwHighDateTime;
}
bool anti_cheat_running() {
  const HANDLE snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
  if (snapshot == INVALID_HANDLE_VALUE) return true;
  PROCESSENTRY32W entry{}; entry.dwSize = sizeof(entry); bool found = false;
  if (Process32FirstW(snapshot, &entry)) do {
    if (_wcsnicmp(entry.szExeFile, L"EasyAntiCheat", 13) == 0) { found = true; break; }
  } while (Process32NextW(snapshot, &entry));
  CloseHandle(snapshot); return found;
}
std::uintptr_t description(std::uintptr_t actor) {
  const auto interface_address = actor + 0x78;
  const auto interface_vtable = value<std::uintptr_t>(interface_address);
  const auto getter = interface_vtable ? value<std::uintptr_t>(interface_vtable + 0x158) : 0;
  std::array<unsigned char, 8> code{}; std::int32_t displacement = 0;
  if (!getter || !read_memory(getter, code.data(), code.size()) || code[0] != 0x48 ||
      code[1] != 0x8D || code[2] != 0x81 || code[7] != 0xC3) return 0;
  std::memcpy(&displacement, code.data() + 3, sizeof(displacement));
  return interface_address + displacement;
}
struct ScalePair { float scale{}, inverse{}; };
bool valid(const ScalePair &pair) {
  return std::isfinite(pair.scale) && std::isfinite(pair.inverse) && pair.scale > 0.1f &&
         pair.scale < 10.0f && std::fabs(pair.scale * pair.inverse - 1.0f) < 0.001f;
}
bool exact(const ScalePair &a, const ScalePair &b) { return std::memcmp(&a, &b, sizeof(a)) == 0; }
struct Target {
  const char *kind{}; std::uintptr_t actor_vtable{}, description_vtable{}, manager{}, actor{}, desc{};
  unsigned char original_flag{}, target_flag{}; ScalePair original{}, target{}; bool flag_written{}, cache_written{};
};
bool active(std::uintptr_t base, const Target &t) {
  return t.manager && t.actor && t.desc && value<std::uintptr_t>(t.manager) == base + 0x6820088 &&
    value<std::uintptr_t>(t.manager + 0x98) == t.actor &&
    value<std::uintptr_t>(t.actor) == base + t.actor_vtable && description(t.actor) == t.desc &&
    value<std::uintptr_t>(t.desc) == base + t.description_vtable &&
    value<unsigned char>(t.manager + 0xB8) == 0;
}
bool collect(std::uintptr_t base, float factor, std::vector<Target> &targets,
             unsigned &generic_count, unsigned &cheerleader_count) {
  const auto sentinel = base + 0xA37FFB0; auto node = value<std::uintptr_t>(sentinel + 0x10);
  auto previous = sentinel;
  for (unsigned count = 0; count < 256 && node && node != sentinel; ++count) {
    if (value<std::uintptr_t>(node) != base + 0x681FCC8 ||
        value<std::uintptr_t>(node + 8) != previous) return false;
    const auto manager = node + 0x78, actor = value<std::uintptr_t>(manager + 0x98);
    const auto actor_vtable = actor ? value<std::uintptr_t>(actor) : 0;
    const char *kind = nullptr; std::uintptr_t actor_rva = 0, description_rva = 0;
    if (actor_vtable == base + 0x68180D8) { kind = "GENERIC"; actor_rva = 0x68180D8; description_rva = 0x6817E08; }
    else if (actor_vtable == base + 0x6801DE8) { kind = "CHEERLEADER"; actor_rva = 0x6801DE8; description_rva = 0x6855FD8; }
    if (kind) {
      const auto desc = description(actor); const auto flag = value<unsigned char>(desc + 0x7D);
      ScalePair original{}; read_value(actor + 0xBCC, original);
      if (!desc || value<std::uintptr_t>(desc) != base + description_rva || !valid(original)) return false;
      if (!(flag & 2)) {
        Target target{kind, actor_rva, description_rva, manager, actor, desc, flag,
                      static_cast<unsigned char>(flag | 2), original,
                      {original.scale * factor, 1.0f / (original.scale * factor)}};
        if (!valid(target.target)) return false;
        targets.push_back(target); if (actor_rva == 0x68180D8) ++generic_count; else ++cheerleader_count;
      }
    }
    previous = node; node = value<std::uintptr_t>(node + 0x10);
  }
  return node == sentinel && value<std::uintptr_t>(sentinel + 8) == previous &&
         generic_count > 0 && cheerleader_count > 0 && targets.size() <= 220;
}
} // namespace

// Reversible arena-population proof. Only verified GENERIC and CHEERLEADER
// actors that do not already opt in are touched. Props and every other class
// are excluded. Exact flag/cache bytes are restored on Enter or after 90 sec.
int main(int argc, char **argv) {
  std::setvbuf(stdout, nullptr, _IONBF, 0); if (argc != 4) return 2;
  const auto pid = static_cast<DWORD>(std::strtoul(argv[1], nullptr, 10));
  const auto expected = std::strtoull(argv[2], nullptr, 10);
  const auto base = static_cast<std::uintptr_t>(std::strtoull(argv[3], nullptr, 10));
  if (anti_cheat_running()) return 3;
  process_handle = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ | PROCESS_VM_WRITE |
    PROCESS_VM_OPERATION, FALSE, pid);
  if (!process_handle || !process_identity(expected) || !supported_build(base)) return 4;
  const float factor = value<float>(base + 0xA39319C);
  if (!std::isfinite(factor) || factor < 0.9f || factor >= 1.0f) return 5;
  std::vector<Target> targets; unsigned generic_count = 0, cheerleader_count = 0;
  if (!collect(base, factor, targets, generic_count, cheerleader_count)) return 6;
  for (auto &target : targets) if (!active(base, target) ||
      value<unsigned char>(target.desc + 0x7D) != target.original_flag ||
      !exact(value<ScalePair>(target.actor + 0xBCC), target.original)) return 7;
  unsigned applied = 0;
  for (auto &target : targets) {
    if (!active(base, target) || !write_memory(target.desc + 0x7D, &target.target_flag, 1) ||
        value<unsigned char>(target.desc + 0x7D) != target.target_flag) break;
    target.flag_written = true;
    if (!write_memory(target.actor + 0xBCC, &target.target, sizeof(target.target)) ||
        !exact(value<ScalePair>(target.actor + 0xBCC), target.target)) break;
    target.cache_written = true; ++applied;
  }
  if (applied != targets.size()) std::printf("STATE=PARTIAL APPLIED=%u EXPECTED=%zu\n", applied, targets.size());
  else std::printf("STATE=APPLIED GENERIC=%u CHEERLEADER=%u FACTOR=%.9g CACHE_WRITES=%u FLAG_WRITES=%u CODE_WRITES=0 AUTO_RESTORE_SECONDS=90\n",
                   generic_count, cheerleader_count, factor, applied, applied);
  const HANDLE input = GetStdHandle(STD_INPUT_HANDLE); const auto started = GetTickCount64();
  while (applied == targets.size() && GetTickCount64() - started < 90000 && process_identity(expected)) {
    if (WaitForSingleObject(input, 100) == WAIT_OBJECT_0) { char b[32]{}; DWORD got = 0; ReadFile(input, b, sizeof(b), &got, nullptr); break; }
  }
  unsigned cache_restored = 0, flags_restored = 0, skipped = 0;
  for (auto &target : targets) {
    if (!active(base, target)) { ++skipped; continue; }
    if (target.cache_written && exact(value<ScalePair>(target.actor + 0xBCC), target.target) &&
        write_memory(target.actor + 0xBCC, &target.original, sizeof(target.original)) &&
        exact(value<ScalePair>(target.actor + 0xBCC), target.original)) ++cache_restored;
    if (target.flag_written && value<unsigned char>(target.desc + 0x7D) == target.target_flag &&
        write_memory(target.desc + 0x7D, &target.original_flag, 1) &&
        value<unsigned char>(target.desc + 0x7D) == target.original_flag) ++flags_restored;
  }
  std::printf("STATE=RESTORED CACHE=%u FLAGS=%u SKIPPED_DESTROYED=%u TOTAL=%zu\n",
              cache_restored, flags_restored, skipped, targets.size());
  CloseHandle(process_handle);
  return cache_restored + skipped == targets.size() && flags_restored + skipped == targets.size() ? 0 : 8;
}
