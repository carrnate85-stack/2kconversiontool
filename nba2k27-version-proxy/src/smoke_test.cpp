#include <Windows.h>

#include <cstdio>

int wmain(int argc, wchar_t **argv) {
  if (argc != 2) return 2;
  const HMODULE module = LoadLibraryW(argv[1]);
  if (module == nullptr) return 3;
  // The read-only mapper writes its bounded report from a one-shot worker so no
  // file I/O occurs under the Windows loader lock. Let that worker finish
  // before this artificial smoke-test process unloads the module.
  Sleep(500);
  using SizeProc = DWORD(WINAPI *)(LPCSTR, LPDWORD);
  using InfoProc = BOOL(WINAPI *)(LPCSTR, DWORD, DWORD, LPVOID);
  using QueryProc = BOOL(WINAPI *)(LPCVOID, LPCSTR, LPVOID *, PUINT);
  const auto size_proc = reinterpret_cast<SizeProc>(
      GetProcAddress(module, "GetFileVersionInfoSizeA"));
  const auto info_proc = reinterpret_cast<InfoProc>(
      GetProcAddress(module, "GetFileVersionInfoA"));
  const auto query_proc = reinterpret_cast<QueryProc>(
      GetProcAddress(module, "VerQueryValueA"));
  if (size_proc == nullptr || info_proc == nullptr || query_proc == nullptr) {
    FreeLibrary(module);
    return 4;
  }
  char system_path[MAX_PATH] = {};
  const UINT system_length = GetSystemDirectoryA(system_path, MAX_PATH);
  if (system_length == 0 || system_length >= MAX_PATH - 13) return 5;
  strcat_s(system_path, "\\kernel32.dll");
  DWORD ignored = 0;
  const DWORD bytes = size_proc(system_path, &ignored);
  if (bytes == 0) return 6;
  auto data = new BYTE[bytes];
  const BOOL read = info_proc(system_path, 0, bytes, data);
  LPVOID fixed_info = nullptr;
  UINT fixed_bytes = 0;
  const BOOL queried =
      read != FALSE && query_proc(data, "\\", &fixed_info, &fixed_bytes);
  delete[] data;
  FreeLibrary(module);
  if (queried == FALSE || fixed_info == nullptr || fixed_bytes == 0) return 7;
  std::puts("PASS: version proxy loaded and forwarded version APIs.");
  return 0;
}
