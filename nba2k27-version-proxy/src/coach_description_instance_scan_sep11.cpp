#include <Windows.h>

#include <algorithm>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <vector>

namespace {
HANDLE process_handle = nullptr;

bool read_memory(std::uintptr_t address, void *out, std::size_t size) {
  SIZE_T read = 0;
  return address >= 0x10000 &&
         ReadProcessMemory(process_handle, reinterpret_cast<const void *>(address), out, size, &read) &&
         read == size;
}

template <class T> bool read_value(std::uintptr_t address, T &out) {
  return read_memory(address, &out, sizeof(out));
}

template <class T> T value(std::uintptr_t address) {
  T result{};
  read_value(address, result);
  return result;
}

bool supported_build(std::uintptr_t base) {
  IMAGE_DOS_HEADER dos{};
  IMAGE_NT_HEADERS64 nt{};
  return read_value(base, dos) && dos.e_magic == IMAGE_DOS_SIGNATURE && dos.e_lfanew > 0 &&
         dos.e_lfanew < 0x1000 && read_value(base + dos.e_lfanew, nt) &&
         nt.Signature == IMAGE_NT_SIGNATURE && nt.FileHeader.Machine == IMAGE_FILE_MACHINE_AMD64 &&
         nt.FileHeader.TimeDateStamp == 0x6AA2013D && nt.OptionalHeader.SizeOfImage == 0x36558000;
}

bool readable(DWORD protect) {
  if (protect & (PAGE_GUARD | PAGE_NOACCESS)) return false;
  const DWORD base = protect & 0xff;
  return base == PAGE_READONLY || base == PAGE_READWRITE || base == PAGE_WRITECOPY ||
         base == PAGE_EXECUTE_READ || base == PAGE_EXECUTE_READWRITE || base == PAGE_EXECUTE_WRITECOPY;
}
} // namespace

// Read-only search for live coach-description objects. The purpose is to
// distinguish actor-embedded copies from earlier roster/source descriptions
// that could be opted into native scaling before actor construction.
int main(int argc, char **argv) {
  if (argc != 4) return 2;
  const auto pid = static_cast<DWORD>(std::strtoul(argv[1], nullptr, 10));
  const auto expected_created = std::strtoull(argv[2], nullptr, 10);
  const auto base = static_cast<std::uintptr_t>(std::strtoull(argv[3], nullptr, 10));
  process_handle = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, FALSE, pid);
  if (!process_handle) return 3;

  FILETIME created{}, exited{}, kernel{}, user{};
  ULARGE_INTEGER stamp{};
  if (!GetProcessTimes(process_handle, &created, &exited, &kernel, &user)) return 4;
  stamp.LowPart = created.dwLowDateTime;
  stamp.HighPart = created.dwHighDateTime;
  if (stamp.QuadPart != expected_created || exited.dwLowDateTime || exited.dwHighDateTime ||
      !supported_build(base))
    return 5;

  constexpr std::size_t chunk_size = 1u << 20;
  const std::uintptr_t wanted = base + 0x6807818;
  std::vector<unsigned char> bytes(chunk_size);
  std::vector<std::uintptr_t> matches;
  std::uintptr_t cursor = 0x10000;
  unsigned regions = 0;
  std::uint64_t scanned = 0;
  while (cursor < 0x7FFFFFFF0000ULL) {
    MEMORY_BASIC_INFORMATION region{};
    if (!VirtualQueryEx(process_handle, reinterpret_cast<const void *>(cursor), &region,
                        sizeof(region)))
      break;
    const auto start = reinterpret_cast<std::uintptr_t>(region.BaseAddress);
    const auto size = static_cast<std::uint64_t>(region.RegionSize);
    const auto next = start + size;
    if (next <= cursor) break;
    if (region.State == MEM_COMMIT && readable(region.Protect) && region.Type != MEM_IMAGE &&
        size <= (1ULL << 34)) {
      ++regions;
      for (std::uint64_t offset = 0; offset < size; offset += chunk_size) {
        const auto wanted_size = static_cast<std::size_t>(
            std::min<std::uint64_t>(chunk_size, size - offset));
        SIZE_T received = 0;
        if (!ReadProcessMemory(process_handle, reinterpret_cast<const void *>(start + offset),
                               bytes.data(), wanted_size, &received) || received < sizeof(wanted))
          continue;
        scanned += received;
        for (std::size_t at = 0; at + sizeof(wanted) <= received; at += alignof(std::uintptr_t)) {
          std::uintptr_t candidate = 0;
          std::memcpy(&candidate, bytes.data() + at, sizeof(candidate));
          if (candidate == wanted) matches.push_back(start + offset + at);
        }
      }
    }
    cursor = next;
  }

  std::printf("BUILD=6AA2013D:36558000 PID=%lu READ_ONLY=YES REGIONS=%u BYTES=%llu MATCHES=%zu\n",
              static_cast<unsigned long>(pid), regions,
              static_cast<unsigned long long>(scanned), matches.size());
  unsigned embedded = 0, standalone = 0;
  for (const auto address : matches) {
    unsigned char flag_7c = 0, flag_7d = 0;
    read_value(address + 0x7C, flag_7c);
    read_value(address + 0x7D, flag_7d);
    const auto actor = address >= 0xF20 ? address - 0xF20 : 0;
    const bool actor_embedded = actor && value<std::uintptr_t>(actor) == base + 0x6807E58;
    const bool list_shape = value<std::uintptr_t>(address + 0xE8) == address + 0x128;
    if (actor_embedded)
      ++embedded;
    else
      ++standalone;
    std::printf("DESCRIPTION=0x%llX KIND=%s ACTOR=0x%llX FLAG_7C=0x%02X FLAG_7D=0x%02X "
                "SHARED_OPT_IN=%s LIST_SHAPE=%s\n",
                static_cast<unsigned long long>(address),
                actor_embedded ? "ACTOR_EMBEDDED" : "STANDALONE",
                static_cast<unsigned long long>(actor_embedded ? actor : 0), flag_7c, flag_7d,
                (flag_7d & 2) ? "YES" : "NO", list_shape ? "YES" : "NO");
  }
  std::printf("EMBEDDED=%u STANDALONE=%u PROCESS_WRITES=0 NATIVE_CALLS=0\n", embedded,
              standalone);
  CloseHandle(process_handle);
  return 0;
}
