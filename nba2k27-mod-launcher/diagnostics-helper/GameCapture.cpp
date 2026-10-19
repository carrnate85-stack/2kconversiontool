#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <tlhelp32.h>
#include <dbghelp.h>
#include <cstdio>
#include <string>
#include <vector>

// External read-only handle: never inject, debug-attach, suspend, terminate, or
// write game memory. DbgHelp may briefly pause threads while taking a dump.
static constexpr DWORD processAccess = PROCESS_QUERY_INFORMATION | PROCESS_VM_READ | SYNCHRONIZE;
static constexpr LONGLONG maxDump = 64LL * 1024 * 1024;
static constexpr ULONGLONG maxDumpMs = 15000;
struct DumpBudget { HANDLE file; ULONGLONG start; };
static BOOL CALLBACK budget(PVOID raw, const PMINIDUMP_CALLBACK_INPUT input, PMINIDUMP_CALLBACK_OUTPUT output) {
    if (input->CallbackType == CancelCallback) {
        auto* state = static_cast<DumpBudget*>(raw); LARGE_INTEGER size{};
        output->CheckCancel = TRUE;
        output->Cancel = GetTickCount64() - state->start > maxDumpMs ||
            !GetFileSizeEx(state->file, &size) || size.QuadPart > maxDump;
    }
    return TRUE;
}
static void event(const char* name, DWORD pid, DWORD value = 0) {
    printf("{\"event\":\"%s\",\"pid\":%lu,\"value\":%lu,\"tick\":%llu}\n", name, pid, value, GetTickCount64()); fflush(stdout);
}
static std::wstring absolute(const wchar_t* input) {
    std::vector<wchar_t> buffer(32768); DWORD size = GetFullPathNameW(input, static_cast<DWORD>(buffer.size()), buffer.data(), nullptr);
    if (!size || size >= buffer.size()) return L"";
    return std::wstring(buffer.data());
}
static bool plain(const std::wstring& input) {
    std::wstring current = input;
    while (current.size() > 3) {
        DWORD attrs = GetFileAttributesW(current.c_str());
        if (attrs == INVALID_FILE_ATTRIBUTES || (attrs & FILE_ATTRIBUTE_REPARSE_POINT)) return false;
        size_t at = current.find_last_of(L"\\/"); if (at == std::wstring::npos) return false;
        current.resize(at);
    }
    return true;
}
static HANDLE findGame(const std::wstring& expected, DWORD& pid) {
    HANDLE snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0); if (snapshot == INVALID_HANDLE_VALUE) return nullptr;
    PROCESSENTRY32W entry{}; entry.dwSize = sizeof(entry); HANDLE result = nullptr;
    if (Process32FirstW(snapshot, &entry)) do {
        if (_wcsicmp(entry.szExeFile, L"NBA2K27.exe")) continue;
        HANDLE handle = OpenProcess(processAccess, FALSE, entry.th32ProcessID); if (!handle) continue;
        wchar_t image[32768]; DWORD count = 32768;
        if (QueryFullProcessImageNameW(handle, 0, image, &count) && !_wcsicmp(expected.c_str(), image)) { result = handle; pid = entry.th32ProcessID; break; }
        CloseHandle(handle);
    } while (Process32NextW(snapshot, &entry));
    CloseHandle(snapshot); return result;
}
struct WindowState { DWORD pid; bool hung; };
static BOOL CALLBACK windows(HWND window, LPARAM parameter) {
    auto* state = reinterpret_cast<WindowState*>(parameter); DWORD pid = 0; GetWindowThreadProcessId(window, &pid);
    if (pid == state->pid && IsWindowVisible(window) && IsHungAppWindow(window)) state->hung = true;
    return TRUE;
}
static bool dump(HANDLE game, DWORD pid, const std::wstring& directory, unsigned index, bool automatic) {
    const std::wstring partial = directory + L"\\hang-" + std::to_wstring(index) + L".partial";
    const std::wstring target = directory + L"\\hang-" + std::to_wstring(index) + L".dmp";
    HANDLE file = CreateFileW(partial.c_str(), GENERIC_WRITE | GENERIC_READ, 0, nullptr, CREATE_NEW, FILE_ATTRIBUTE_NORMAL, nullptr);
    if (file == INVALID_HANDLE_VALUE) { event(automatic ? "auto-dump-error" : "dump-error", pid, GetLastError()); return false; }
    DumpBudget state{file, GetTickCount64()}; MINIDUMP_CALLBACK_INFORMATION callback{budget, &state};
    event("dump-start", pid, index);
    BOOL ok = MiniDumpWriteDump(game, pid, file, static_cast<MINIDUMP_TYPE>(MiniDumpNormal | MiniDumpWithThreadInfo | MiniDumpWithUnloadedModules), nullptr, nullptr, &callback);
    DWORD error = ok ? 0 : GetLastError(); LARGE_INTEGER size{};
    if (!GetFileSizeEx(file, &size) || size.QuadPart > maxDump) { ok = FALSE; error = ERROR_FILE_TOO_LARGE; }
    FlushFileBuffers(file); CloseHandle(file);
    if (ok && !MoveFileW(partial.c_str(), target.c_str())) { ok = FALSE; error = GetLastError(); }
    if (!ok) DeleteFileW(partial.c_str());
    event(automatic ? (ok ? "auto-dump-complete" : "auto-dump-error") : (ok ? "dump-complete" : "dump-error"), pid, ok ? index : error);
    return ok != FALSE;
}
int wmain(int argc, wchar_t** argv) {
    if (argc == 2 && !wcscmp(argv[1], L"--fixture")) { Sleep(4000); return 0x1234; }
    if (argc == 3 && !wcscmp(argv[1], L"--self-test")) {
        std::wstring directory = absolute(argv[2]); if (directory.empty() || !plain(directory)) return 3;
        wchar_t executable[32768]; if (!GetModuleFileNameW(nullptr, executable, 32768)) return 4;
        std::wstring command = L"\"" + std::wstring(executable) + L"\" --fixture";
        STARTUPINFOW startup{}; startup.cb = sizeof(startup); PROCESS_INFORMATION child{};
        if (!CreateProcessW(executable, command.data(), nullptr, nullptr, FALSE, CREATE_NO_WINDOW, nullptr, nullptr, &startup, &child)) return 5;
        HANDLE handle = OpenProcess(processAccess, FALSE, child.dwProcessId);
        Sleep(200); bool ok = handle && dump(handle, child.dwProcessId, directory, 1, false);
        if (handle) CloseHandle(handle);
        DWORD code = 0; ok = WaitForSingleObject(child.hProcess, 10000) == WAIT_OBJECT_0 && GetExitCodeProcess(child.hProcess, &code) && code == 0x1234 && ok;
        CloseHandle(child.hThread); CloseHandle(child.hProcess); event("self-test-exit", child.dwProcessId, code);
        return ok ? 0 : 6;
    }
    if (argc != 4 || wcscmp(argv[1], L"--watch")) return 2;
    const std::wstring expected = absolute(argv[2]), directory = absolute(argv[3]);
    if (expected.empty() || directory.empty() || !plain(directory) ||
        _wcsicmp(expected.substr(expected.find_last_of(L"\\/") + 1).c_str(), L"NBA2K27.exe")) return 3;
    DWORD pid = 0; HANDLE game = nullptr; const ULONGLONG started = GetTickCount64();
    ULONGLONG lastSample = 0, hungSince = 0; unsigned dumps = 0; bool autoTaken = false; std::string commands;
    event("waiting", 0);
    while (GetTickCount64() - started < 6ULL * 60 * 60 * 1000) {
        DWORD available = 0; HANDLE input = GetStdHandle(STD_INPUT_HANDLE);
        if (!PeekNamedPipe(input, nullptr, 0, nullptr, &available, nullptr)) break;
        if (available) { char bytes[256]; DWORD read = 0; if (!ReadFile(input, bytes, (available < sizeof(bytes) ? available : sizeof(bytes)), &read, nullptr)) break; commands.append(bytes, read); }
        if (commands.size() > 1024) break;
        if (!game) { game = findGame(expected, pid); if (game) event("attached-read-only", pid); else if (GetTickCount64() - started > 90000) { event("game-not-found", 0); break; } }
        if (game && WaitForSingleObject(game, 0) == WAIT_OBJECT_0) {
            DWORD code = 0; if (GetExitCodeProcess(game, &code)) event("game-exit", pid, code); else event("exit-code-error", pid, GetLastError());
            break;
        }
        size_t newline;
        while ((newline = commands.find('\n')) != std::string::npos) {
            std::string command = commands.substr(0, newline); commands.erase(0, newline + 1);
            if (command == "stop") { if (game) CloseHandle(game); event("stopped", pid); return 0; }
            if (command == "capture") {
                if (!game || dumps >= 3) event("dump-error", pid, game ? ERROR_TOO_MANY_CMDS : ERROR_NOT_READY);
                else dump(game, pid, directory, ++dumps, false);
            }
        }
        if (game && GetTickCount64() - lastSample >= 1000) {
            lastSample = GetTickCount64(); WindowState state{pid, false}; EnumWindows(windows, reinterpret_cast<LPARAM>(&state));
            event("sample", pid, state.hung ? 1 : 0);
            if (!state.hung) hungSince = 0; else if (!hungSince) hungSince = lastSample;
            if (hungSince && lastSample - hungSince >= 10000 && !autoTaken && dumps < 3) { autoTaken = true; dump(game, pid, directory, ++dumps, true); }
        }
        Sleep(100);
    }
    if (game) CloseHandle(game);
    event("watch-ended", pid); return 0;
}
