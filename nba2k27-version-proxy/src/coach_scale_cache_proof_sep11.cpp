#include <Windows.h>
#include <TlHelp32.h>

#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>

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

bool write_memory(std::uintptr_t address, const void *data, std::size_t size) {
  SIZE_T written = 0;
  return WriteProcessMemory(process_handle, reinterpret_cast<void *>(address), data, size, &written) &&
         written == size;
}

bool supported_build(std::uintptr_t base) {
  IMAGE_DOS_HEADER dos{};
  IMAGE_NT_HEADERS64 nt{};
  return read_value(base, dos) && dos.e_magic == IMAGE_DOS_SIGNATURE && dos.e_lfanew > 0 &&
         dos.e_lfanew < 0x1000 && read_value(base + dos.e_lfanew, nt) &&
         nt.Signature == IMAGE_NT_SIGNATURE && nt.FileHeader.Machine == IMAGE_FILE_MACHINE_AMD64 &&
         nt.FileHeader.TimeDateStamp == 0x6AA2013D && nt.OptionalHeader.SizeOfImage == 0x36558000;
}

bool process_identity(std::uint64_t expected_created) {
  FILETIME created{}, exited{}, kernel{}, user{};
  ULARGE_INTEGER stamp{};
  if (!GetProcessTimes(process_handle, &created, &exited, &kernel, &user)) return false;
  stamp.LowPart = created.dwLowDateTime;
  stamp.HighPart = created.dwHighDateTime;
  return stamp.QuadPart == expected_created && !exited.dwLowDateTime && !exited.dwHighDateTime;
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

struct ScalePair {
  float scale;
  float inverse;
};

bool valid_pair(const ScalePair &pair) {
  return std::isfinite(pair.scale) && std::isfinite(pair.inverse) && pair.scale > 0.1f &&
         pair.scale < 10.0f && std::fabs(pair.scale * pair.inverse - 1.0f) < 0.001f;
}

bool same_bits(const ScalePair &left, const ScalePair &right) {
  return std::memcmp(&left, &right, sizeof(left)) == 0;
}

struct Coach {
  std::uintptr_t manager{};
  std::uintptr_t actor{};
};

bool active_coach(std::uintptr_t base, const Coach &coach) {
  return coach.manager && coach.actor && value<std::uintptr_t>(coach.manager) == base + 0x6820088 &&
         value<std::uintptr_t>(coach.manager + 0x98) == coach.actor &&
         value<std::uintptr_t>(coach.actor) == base + 0x6807E58 &&
         value<std::uintptr_t>(coach.actor + 0xF20) == base + 0x6807818 &&
         value<unsigned char>(coach.manager + 0xB8) == 0;
}

Coach first_coach(std::uintptr_t base) {
  const auto sentinel = base + 0xA37FFB0;
  auto node = value<std::uintptr_t>(sentinel + 0x10);
  auto previous = sentinel;
  for (unsigned count = 0; count < 256 && node && node != sentinel; ++count) {
    if (value<std::uintptr_t>(node) != base + 0x681FCC8 ||
        value<std::uintptr_t>(node + 8) != previous)
      return {};
    const Coach coach{node + 0x78, value<std::uintptr_t>(node + 0x78 + 0x98)};
    if (active_coach(base, coach)) return coach;
    previous = node;
    node = value<std::uintptr_t>(node + 0x10);
  }
  return {};
}
} // namespace

// Temporary, one-coach proof only. It changes the two verified cache floats,
// waits for stdin or a 180-second timeout, and then restores the exact bits.
// It never changes descriptions, clothing, transforms, code, or other actors.
int main(int argc, char **argv) {
  std::setvbuf(stdout, nullptr, _IONBF, 0);
  if (argc != 5) return 2;
  const auto pid = static_cast<DWORD>(std::strtoul(argv[1], nullptr, 10));
  const auto expected_created = std::strtoull(argv[2], nullptr, 10);
  const auto base = static_cast<std::uintptr_t>(std::strtoull(argv[3], nullptr, 10));
  const float factor = std::strtof(argv[4], nullptr);
  if (!std::isfinite(factor) || factor < 0.85f || factor > 0.99f) return 3;
  if (anti_cheat_running()) return 4;

  process_handle = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ | PROCESS_VM_WRITE |
                                   PROCESS_VM_OPERATION,
                               FALSE, pid);
  if (!process_handle) return 5;
  if (!process_identity(expected_created) || !supported_build(base)) {
    CloseHandle(process_handle);
    return 6;
  }
  const auto settings = value<std::uintptr_t>(base + 0xAACB9A0);
  if (!settings || value<unsigned>(settings + 0xA04) != 1) {
    CloseHandle(process_handle);
    return 7;
  }
  const Coach coach = first_coach(base);
  const auto description_flags = value<unsigned char>(coach.actor + 0xF20 + 0x7D);
  ScalePair original{};
  if (!active_coach(base, coach) || !read_value(coach.actor + 0xBCC, original) ||
      !valid_pair(original) || (description_flags & 2)) {
    CloseHandle(process_handle);
    return 8;
  }
  const ScalePair target{original.scale * factor, 1.0f / (original.scale * factor)};
  if (!write_memory(coach.actor + 0xBCC, &target, sizeof(target))) {
    CloseHandle(process_handle);
    return 9;
  }
  ScalePair observed{};
  if (!read_value(coach.actor + 0xBCC, observed) || !same_bits(observed, target)) {
    CloseHandle(process_handle);
    return 10;
  }
  std::printf("STATE=APPLIED PID=%lu ACTOR=0x%llX FACTOR=%.9g BEFORE=%.9g AFTER=%.9g "
              "WRITES=1 AUTO_RESTORE_SECONDS=180\n",
              static_cast<unsigned long>(pid), static_cast<unsigned long long>(coach.actor), factor,
              original.scale, target.scale);
  std::printf("Press Enter to restore.\n");

  const HANDLE input = GetStdHandle(STD_INPUT_HANDLE);
  const auto started = GetTickCount64();
  bool process_ended = false;
  while (GetTickCount64() - started < 180000) {
    if (!process_identity(expected_created)) {
      process_ended = true;
      break;
    }
    if (WaitForSingleObject(input, 250) == WAIT_OBJECT_0) {
      char buffer[32]{};
      DWORD received = 0;
      ReadFile(input, buffer, sizeof(buffer), &received, nullptr);
      break;
    }
  }
  if (process_ended) {
    std::printf("STATE=GAME_ENDED RESTORE_WRITES=0\n");
    CloseHandle(process_handle);
    return 0;
  }
  if (!active_coach(base, coach) || !read_value(coach.actor + 0xBCC, observed) ||
      !same_bits(observed, target)) {
    std::printf("STATE=RESTORE_SKIPPED REASON=IDENTITY_OR_VALUE_CHANGED RESTORE_WRITES=0\n");
    CloseHandle(process_handle);
    return 11;
  }
  if (!write_memory(coach.actor + 0xBCC, &original, sizeof(original)) ||
      !read_value(coach.actor + 0xBCC, observed) || !same_bits(observed, original)) {
    std::printf("STATE=RESTORE_FAILED RESTORE_WRITES=1\n");
    CloseHandle(process_handle);
    return 12;
  }
  std::printf("STATE=RESTORED SCALE=%.9g INVERSE=%.9g RESTORE_WRITES=1\n", original.scale,
              original.inverse);
  CloseHandle(process_handle);
  return 0;
}
