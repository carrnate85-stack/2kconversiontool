#include <Windows.h>
#include <TlHelp32.h>

#include <array>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>

namespace {
HANDLE process_handle = nullptr;
bool read_memory(std::uintptr_t at, void *out, std::size_t size) {
  SIZE_T done = 0;
  return at >= 0x10000 &&
         ReadProcessMemory(process_handle, reinterpret_cast<const void *>(at), out, size, &done) &&
         done == size;
}
template <class T> bool read_value(std::uintptr_t at, T &out) {
  return read_memory(at, &out, sizeof(out));
}
template <class T> T value(std::uintptr_t at) {
  T result{};
  read_value(at, result);
  return result;
}
bool write_byte(std::uintptr_t at, unsigned char byte) {
  SIZE_T done = 0;
  return WriteProcessMemory(process_handle, reinterpret_cast<void *>(at), &byte, 1, &done) && done == 1;
}
bool supported_build(std::uintptr_t base) {
  IMAGE_DOS_HEADER dos{};
  IMAGE_NT_HEADERS64 nt{};
  return read_value(base, dos) && dos.e_magic == IMAGE_DOS_SIGNATURE && dos.e_lfanew > 0 &&
         dos.e_lfanew < 0x1000 && read_value(base + dos.e_lfanew, nt) &&
         nt.Signature == IMAGE_NT_SIGNATURE && nt.FileHeader.Machine == IMAGE_FILE_MACHINE_AMD64 &&
         nt.FileHeader.TimeDateStamp == 0x6AA2013D && nt.OptionalHeader.SizeOfImage == 0x36558000;
}
bool process_identity(std::uint64_t expected) {
  FILETIME created{}, exited{}, kernel{}, user{};
  ULARGE_INTEGER stamp{};
  if (!GetProcessTimes(process_handle, &created, &exited, &kernel, &user)) return false;
  stamp.LowPart = created.dwLowDateTime;
  stamp.HighPart = created.dwHighDateTime;
  return stamp.QuadPart == expected && !exited.dwLowDateTime && !exited.dwHighDateTime;
}
bool anti_cheat_running() {
  const HANDLE snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
  if (snapshot == INVALID_HANDLE_VALUE) return true;
  PROCESSENTRY32W entry{};
  entry.dwSize = sizeof(entry);
  bool found = false;
  if (Process32FirstW(snapshot, &entry)) {
    do {
      if (_wcsnicmp(entry.szExeFile, L"EasyAntiCheat", 13) == 0) {
        found = true;
        break;
      }
    } while (Process32NextW(snapshot, &entry));
  }
  CloseHandle(snapshot);
  return found;
}
std::uintptr_t description(std::uintptr_t actor) {
  const auto interface_address = actor + 0x78;
  const auto getter = value<std::uintptr_t>(value<std::uintptr_t>(interface_address) + 0x158);
  std::array<unsigned char, 8> code{};
  std::int32_t displacement = 0;
  if (!getter || !read_memory(getter, code.data(), code.size()) || code[0] != 0x48 ||
      code[1] != 0x8D || code[2] != 0x81 || code[7] != 0xC3)
    return 0;
  std::memcpy(&displacement, code.data() + 3, sizeof(displacement));
  return interface_address + displacement;
}
struct Target {
  const char *kind{};
  std::uintptr_t actor_vtable{};
  std::uintptr_t description_vtable{};
  std::uintptr_t manager{};
  std::uintptr_t actor{};
  std::uintptr_t description{};
  unsigned char original_flag{};
  float original_scale{};
  bool changed{};
};
bool active(std::uintptr_t base, const Target &target) {
  return target.manager && target.actor && target.description &&
         value<std::uintptr_t>(target.manager) == base + 0x6820088 &&
         value<std::uintptr_t>(target.manager + 0x98) == target.actor &&
         value<std::uintptr_t>(target.actor) == base + target.actor_vtable &&
         description(target.actor) == target.description &&
         value<std::uintptr_t>(target.description) == base + target.description_vtable &&
         value<unsigned char>(target.manager + 0xB8) == 0;
}
bool find_targets(std::uintptr_t base, std::array<Target, 2> &targets) {
  const auto sentinel = base + 0xA37FFB0;
  auto node = value<std::uintptr_t>(sentinel + 0x10);
  auto previous = sentinel;
  for (unsigned count = 0; count < 256 && node && node != sentinel; ++count) {
    if (value<std::uintptr_t>(node) != base + 0x681FCC8 ||
        value<std::uintptr_t>(node + 8) != previous)
      return false;
    const auto manager = node + 0x78;
    const auto actor = value<std::uintptr_t>(manager + 0x98);
    const auto actor_vtable = actor ? value<std::uintptr_t>(actor) : 0;
    for (auto &target : targets) {
      if (target.actor || actor_vtable != base + target.actor_vtable) continue;
      const auto desc = description(actor);
      if (desc && value<std::uintptr_t>(desc) == base + target.description_vtable &&
          !(value<unsigned char>(desc + 0x7D) & 2)) {
        target.manager = manager;
        target.actor = actor;
        target.description = desc;
        target.original_flag = value<unsigned char>(desc + 0x7D);
        target.original_scale = value<float>(actor + 0xBCC);
      }
    }
    previous = node;
    node = value<std::uintptr_t>(node + 0x10);
  }
  return node == sentinel && value<std::uintptr_t>(sentinel + 8) == previous &&
         targets[0].actor && targets[1].actor;
}
} // namespace

// Two-character proof: one GENERIC arena character and one CHEERLEADER. Only
// their existing native shared-scale flag is toggled. The exact original flags
// are restored on Enter or after 90 seconds; props are never considered.
int main(int argc, char **argv) {
  std::setvbuf(stdout, nullptr, _IONBF, 0);
  if (argc != 4) return 2;
  const auto pid = static_cast<DWORD>(std::strtoul(argv[1], nullptr, 10));
  const auto expected_created = std::strtoull(argv[2], nullptr, 10);
  const auto base = static_cast<std::uintptr_t>(std::strtoull(argv[3], nullptr, 10));
  if (anti_cheat_running()) return 3;
  process_handle = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ | PROCESS_VM_WRITE |
                                   PROCESS_VM_OPERATION,
                               FALSE, pid);
  if (!process_handle || !process_identity(expected_created) || !supported_build(base)) return 4;
  const float factor = value<float>(base + 0xA39319C);
  if (!std::isfinite(factor) || factor < 0.9f || factor >= 1.0f) return 5;
  std::array<Target, 2> targets{{{"GENERIC", 0x68180D8, 0x6817E08},
                                  {"CHEERLEADER", 0x6801DE8, 0x6855FD8}}};
  if (!find_targets(base, targets)) return 6;
  unsigned applied = 0;
  for (auto &target : targets) {
    if (!active(base, target) || !std::isfinite(target.original_scale) ||
        !write_byte(target.description + 0x7D, target.original_flag | 2) ||
        value<unsigned char>(target.description + 0x7D) != (target.original_flag | 2))
      break;
    ++applied;
    std::printf("STATE=ARMED KIND=%s ACTOR=0x%llX SCALE_BEFORE=%.9g FLAG_BEFORE=0x%02X\n",
                target.kind, static_cast<unsigned long long>(target.actor), target.original_scale,
                target.original_flag);
  }
  if (applied != targets.size()) {
    for (unsigned index = 0; index < applied; ++index)
      if (active(base, targets[index]))
        write_byte(targets[index].description + 0x7D, targets[index].original_flag);
    return 7;
  }
  std::printf("SHARED_FACTOR=%.9g CACHE_WRITES=0 CODE_WRITES=0 AUTO_RESTORE_SECONDS=90\n", factor);
  const HANDLE input = GetStdHandle(STD_INPUT_HANDLE);
  const auto started = GetTickCount64();
  while (GetTickCount64() - started < 90000) {
    if (!process_identity(expected_created)) break;
    for (auto &target : targets) {
      if (!target.changed && active(base, target)) {
        const float scale = value<float>(target.actor + 0xBCC);
        if (std::isfinite(scale) && std::fabs(scale - target.original_scale) > 0.00001f) {
          target.changed = true;
          std::printf("STATE=NATIVE_CACHE_CHANGED KIND=%s SCALE=%.9g RATIO=%.9g EXPECTED=%.9g\n",
                      target.kind, scale, scale / target.original_scale, factor);
        }
      }
    }
    if (WaitForSingleObject(input, 100) == WAIT_OBJECT_0) {
      char buffer[32]{};
      DWORD received = 0;
      ReadFile(input, buffer, sizeof(buffer), &received, nullptr);
      break;
    }
  }
  unsigned restored = 0;
  for (auto &target : targets) {
    if (active(base, target) &&
        value<unsigned char>(target.description + 0x7D) == (target.original_flag | 2) &&
        write_byte(target.description + 0x7D, target.original_flag) &&
        value<unsigned char>(target.description + 0x7D) == target.original_flag)
      ++restored;
  }
  std::printf("STATE=RESTORED FLAGS=%u GENERIC_CHANGED=%s CHEERLEADER_CHANGED=%s\n", restored,
              targets[0].changed ? "YES" : "NO", targets[1].changed ? "YES" : "NO");
  CloseHandle(process_handle);
  return restored == targets.size() ? 0 : 8;
}
