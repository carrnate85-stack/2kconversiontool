#include <Windows.h>
#include <TlHelp32.h>

#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstdlib>

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

bool write_byte(std::uintptr_t address, unsigned char byte) {
  SIZE_T written = 0;
  return WriteProcessMemory(process_handle, reinterpret_cast<void *>(address), &byte, 1, &written) &&
         written == 1;
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

// Temporary one-coach flag proof. This changes only the existing native
// shared-scale opt-in bit, observes the game's own scale cache, and restores
// the exact original flag on Enter or after 120 seconds. It makes no code,
// transform, cache-float, clothing, or global-scale writes.
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
  if (!process_handle) return 4;
  if (!process_identity(expected_created) || !supported_build(base)) return 5;
  const auto settings = value<std::uintptr_t>(base + 0xAACB9A0);
  const float factor = value<float>(base + 0xA39319C);
  if (!settings || value<unsigned>(settings + 0xA04) != 1 || !std::isfinite(factor) ||
      factor < 0.90f || factor >= 1.0f)
    return 6;

  const Coach coach = first_coach(base);
  if (!active_coach(base, coach)) return 7;
  const auto flag_address = coach.actor + 0xF20 + 0x7D;
  const auto original_flag = value<unsigned char>(flag_address);
  const auto original_scale = value<float>(coach.actor + 0xBCC);
  const auto target_flag = static_cast<unsigned char>(original_flag | 2);
  if ((original_flag & 2) || !std::isfinite(original_scale) || original_scale < 0.1f ||
      original_scale > 10.0f)
    return 8;
  if (!write_byte(flag_address, target_flag) || value<unsigned char>(flag_address) != target_flag)
    return 9;

  std::printf("STATE=ARMED PID=%lu ACTOR=0x%llX FLAG_BEFORE=0x%02X FLAG_AFTER=0x%02X "
              "SCALE_BEFORE=%.9g SHARED_FACTOR=%.9g CACHE_WRITES=0 CODE_WRITES=0 "
              "AUTO_RESTORE_SECONDS=120\n",
              static_cast<unsigned long>(pid), static_cast<unsigned long long>(coach.actor),
              original_flag, target_flag, original_scale, factor);
  std::printf("Unpause briefly if paused. Press Enter to restore the flag.\n");

  const HANDLE input = GetStdHandle(STD_INPUT_HANDLE);
  const auto started = GetTickCount64();
  float last_scale = original_scale;
  while (GetTickCount64() - started < 120000) {
    if (!process_identity(expected_created) || !active_coach(base, coach)) {
      std::printf("STATE=IDENTITY_CHANGED RESTORE_WRITES=0\n");
      CloseHandle(process_handle);
      return 0;
    }
    const float scale = value<float>(coach.actor + 0xBCC);
    if (std::isfinite(scale) && std::fabs(scale - last_scale) > 0.00001f) {
      std::printf("STATE=NATIVE_CACHE_CHANGED SCALE=%.9g RATIO=%.9g EXPECTED_FACTOR=%.9g\n", scale,
                  scale / original_scale, factor);
      last_scale = scale;
    }
    if (WaitForSingleObject(input, 100) == WAIT_OBJECT_0) {
      char buffer[32]{};
      DWORD received = 0;
      ReadFile(input, buffer, sizeof(buffer), &received, nullptr);
      break;
    }
  }
  if (!active_coach(base, coach) || value<unsigned char>(flag_address) != target_flag) {
    std::printf("STATE=RESTORE_SKIPPED REASON=IDENTITY_OR_FLAG_CHANGED RESTORE_WRITES=0\n");
    CloseHandle(process_handle);
    return 10;
  }
  if (!write_byte(flag_address, original_flag) || value<unsigned char>(flag_address) != original_flag) {
    std::printf("STATE=RESTORE_FAILED RESTORE_WRITES=1\n");
    CloseHandle(process_handle);
    return 11;
  }
  std::printf("STATE=RESTORED FLAG=0x%02X FINAL_SCALE=%.9g RESTORE_WRITES=1\n", original_flag,
              value<float>(coach.actor + 0xBCC));
  CloseHandle(process_handle);
  return 0;
}
