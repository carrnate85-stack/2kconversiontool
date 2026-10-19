#include <Windows.h>
#include <TlHelp32.h>

#include <array>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cwchar>
#include <string>

namespace {
constexpr std::uint32_t kTimestamp = 0x6aa2013d;
constexpr std::uint32_t kImageSize = 0x36558000;
constexpr std::uintptr_t kStateRva = 0x0a201970;
constexpr std::uintptr_t kTimerRva = 0x0a201974;
constexpr std::uintptr_t kUpdateRva = 0x00074e00;
constexpr std::array<unsigned char, 16> kUpdateAnchor{
    0x8b, 0x05, 0x6a, 0xcb, 0x18, 0x0a, 0x8d, 0x48,
    0xff, 0x83, 0xf9, 0x04, 0x0f, 0x87, 0xf4, 0x00};
// Preserve the native state transition but leave no intentional dwell after
// the helper observes a startup page.  The game's next update still consumes
// the threshold and performs the normal state change.
constexpr float kRemainingSeconds = 0.0f;

struct Process {
  DWORD pid = 0;
  HANDLE handle = nullptr;
  std::uintptr_t base = 0;
};

bool anti_cheat_running() {
  HANDLE snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
  if (snapshot == INVALID_HANDLE_VALUE) return true;
  PROCESSENTRY32W entry{};
  entry.dwSize = sizeof(entry);
  bool found = false;
  if (Process32FirstW(snapshot, &entry)) {
    do {
      if (wcsstr(entry.szExeFile, L"EasyAntiCheat") ||
          !_wcsicmp(entry.szExeFile, L"start_protected_game.exe")) {
        found = true;
        break;
      }
    } while (Process32NextW(snapshot, &entry));
  }
  CloseHandle(snapshot);
  return found;
}

DWORD find_game() {
  HANDLE snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
  if (snapshot == INVALID_HANDLE_VALUE) return 0;
  PROCESSENTRY32W entry{};
  entry.dwSize = sizeof(entry);
  DWORD pid = 0;
  if (Process32FirstW(snapshot, &entry)) {
    do {
      if (!_wcsicmp(entry.szExeFile, L"NBA2K27.exe")) {
        pid = entry.th32ProcessID;
        break;
      }
    } while (Process32NextW(snapshot, &entry));
  }
  CloseHandle(snapshot);
  return pid;
}

std::uintptr_t find_base(DWORD pid) {
  HANDLE snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32, pid);
  if (snapshot == INVALID_HANDLE_VALUE) return 0;
  MODULEENTRY32W entry{};
  entry.dwSize = sizeof(entry);
  std::uintptr_t base = 0;
  if (Module32FirstW(snapshot, &entry)) {
    do {
      if (!_wcsicmp(entry.szModule, L"NBA2K27.exe")) {
        base = reinterpret_cast<std::uintptr_t>(entry.modBaseAddr);
        break;
      }
    } while (Module32NextW(snapshot, &entry));
  }
  CloseHandle(snapshot);
  return base;
}

bool read_exact(HANDLE process, std::uintptr_t address, void *output, SIZE_T size) {
  SIZE_T done = 0;
  return ReadProcessMemory(process, reinterpret_cast<const void *>(address), output,
                           size, &done) &&
         done == size;
}

bool exact_build(const Process &process) {
  IMAGE_DOS_HEADER dos{};
  IMAGE_NT_HEADERS64 nt{};
  std::array<unsigned char, kUpdateAnchor.size()> anchor{};
  return read_exact(process.handle, process.base, &dos, sizeof(dos)) &&
         dos.e_magic == IMAGE_DOS_SIGNATURE && dos.e_lfanew >= 64 &&
         dos.e_lfanew < 4096 &&
         read_exact(process.handle, process.base + dos.e_lfanew, &nt, sizeof(nt)) &&
         nt.Signature == IMAGE_NT_SIGNATURE &&
         nt.FileHeader.TimeDateStamp == kTimestamp &&
         nt.OptionalHeader.SizeOfImage == kImageSize &&
         read_exact(process.handle, process.base + kUpdateRva, anchor.data(), anchor.size()) &&
         anchor == kUpdateAnchor;
}

float threshold_for(int state) {
  switch (state) {
    case 1: return 5.0f;
    case 2: return 0.5f;
    case 3: return 4.0f;
    case 4: return 0.5f;
    case 5: return 4.0f;
    default: return 0.0f;
  }
}

bool write_timer(const Process &process, int expected_state, float target) {
  int state_before = 0;
  float timer = 0.0f;
  int state_after = 0;
  if (!read_exact(process.handle, process.base + kStateRva, &state_before, sizeof(state_before)) ||
      !read_exact(process.handle, process.base + kTimerRva, &timer, sizeof(timer)) ||
      !read_exact(process.handle, process.base + kStateRva, &state_after, sizeof(state_after)) ||
      state_before != expected_state || state_after != expected_state ||
      !std::isfinite(timer) || timer < 0.0f || timer >= target) {
    return false;
  }
  SIZE_T done = 0;
  if (!WriteProcessMemory(process.handle,
                          reinterpret_cast<void *>(process.base + kTimerRva),
                          &target, sizeof(target), &done) || done != sizeof(target)) {
    return false;
  }
  float verified = 0.0f;
  int verified_state = 0;
  if (!read_exact(process.handle, process.base + kTimerRva, &verified, sizeof(verified)) ||
      !read_exact(process.handle, process.base + kStateRva, &verified_state,
                  sizeof(verified_state)) ||
      !std::isfinite(verified)) {
    return false;
  }
  // The game can naturally advance between the atomic write and readback. A
  // changed state proves the threshold was consumed; otherwise require the
  // requested timer value to still be present.
  return verified_state != expected_state || verified >= target;
}

bool write_receipt(const wchar_t *path, const char *status, DWORD pid,
                   unsigned writes, unsigned seen_mask, const char *message) {
  FILE *file = nullptr;
  if (_wfopen_s(&file, path, L"wb") || !file) return false;
  const int result = std::fprintf(
      file,
      "{\n  \"schema\": \"startup-blank-accelerator-v1\",\n"
      "  \"status\": \"%s\",\n  \"pid\": %lu,\n  \"writes\": %u,\n"
      "  \"seenStateMask\": %u,\n  \"remainingSecondsPerStage\": %.2f,\n"
      "  \"message\": \"%s\"\n}\n",
      status, static_cast<unsigned long>(pid), writes, seen_mask,
      static_cast<double>(kRemainingSeconds), message);
  const bool ok = result > 0 && std::fclose(file) == 0;
  return ok;
}

int run(const wchar_t *receipt) {
  if (anti_cheat_running()) {
    write_receipt(receipt, "rejected", 0, 0, 0, "Anti-cheat process detected");
    return 2;
  }
  const ULONGLONG process_deadline = GetTickCount64() + 120000;
  DWORD pid = 0;
  while (!(pid = find_game()) && GetTickCount64() < process_deadline) Sleep(5);
  if (!pid) {
    write_receipt(receipt, "timeout", 0, 0, 0, "NBA 2K27 did not start");
    return 3;
  }
  Process process{};
  process.pid = pid;
  process.handle = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ |
                                   PROCESS_VM_WRITE | PROCESS_VM_OPERATION | SYNCHRONIZE,
                               FALSE, pid);
  if (!process.handle) {
    write_receipt(receipt, "rejected", pid, 0, 0, "Could not open the game process");
    return 4;
  }
  const ULONGLONG module_deadline = GetTickCount64() + 30000;
  while (!(process.base = find_base(pid)) && GetTickCount64() < module_deadline &&
         WaitForSingleObject(process.handle, 0) == WAIT_TIMEOUT) {
    Sleep(2);
  }
  if (!process.base || !exact_build(process) || anti_cheat_running()) {
    CloseHandle(process.handle);
    write_receipt(receipt, "rejected", pid, 0, 0,
                  "Unsupported build or anti-cheat became active");
    return 5;
  }

  unsigned writes = 0;
  unsigned seen_mask = 0;
  const ULONGLONG deadline = GetTickCount64() + 60000;
  while (GetTickCount64() < deadline &&
         WaitForSingleObject(process.handle, 0) == WAIT_TIMEOUT) {
    if (anti_cheat_running()) {
      CloseHandle(process.handle);
      write_receipt(receipt, "stopped", pid, writes, seen_mask,
                    "Anti-cheat became active; no further writes attempted");
      return 6;
    }
    int state = 0;
    float timer = 0.0f;
    if (!read_exact(process.handle, process.base + kStateRva, &state, sizeof(state)) ||
        !read_exact(process.handle, process.base + kTimerRva, &timer, sizeof(timer))) {
      Sleep(1);
      continue;
    }
    if (state == 6) break;
    const float threshold = threshold_for(state);
    if (threshold > 0.0f) {
      const unsigned bit = 1u << static_cast<unsigned>(state);
      if (!(seen_mask & bit)) {
        const float target = threshold - kRemainingSeconds;
        if (timer >= target || write_timer(process, state, target)) {
          seen_mask |= bit;
          if (timer < target) ++writes;
        }
      }
    }
    Sleep(1);
  }
  const bool alive = WaitForSingleObject(process.handle, 0) == WAIT_TIMEOUT;
  CloseHandle(process.handle);
  const char *status = writes ? "accelerated" : "unchanged";
  const char *message = writes ? "Native blank stages shortened" :
                                 (alive ? "Startup stages were not observed" : "Game exited");
  return write_receipt(receipt, status, pid, writes, seen_mask, message) ? 0 : 7;
}
}  // namespace

int wmain(int argc, wchar_t **argv) {
  if (argc == 2 && !wcscmp(argv[1], L"--self-test")) {
    if (threshold_for(1) != 5.0f || threshold_for(2) != 0.5f ||
        threshold_for(3) != 4.0f || threshold_for(4) != 0.5f ||
        threshold_for(5) != 4.0f || threshold_for(6) != 0.0f) return 1;
    std::puts("PASS: exact state thresholds and fail-closed build anchors configured");
    return 0;
  }
  if (argc != 3 || wcscmp(argv[1], L"--wait-and-accelerate")) {
    std::fputs("Usage: NBA2K27StartupBlankAccelerator.exe --wait-and-accelerate <receipt.json>\n",
               stderr);
    return 64;
  }
  return run(argv[2]);
}
