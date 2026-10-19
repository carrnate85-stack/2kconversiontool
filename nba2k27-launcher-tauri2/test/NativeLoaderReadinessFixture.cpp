// File-only diagnostic of the unchanged loader's readiness prerequisites.
// Never loads the DLL, opens a game process, or registers a native device.
#include "../../nba2k27-version-proxy/src/provider_resolver.hpp"
#include "../../nba2k27-version-proxy/src/session_diagnostics.hpp"
#include <cstdio>
#include <chrono>

int wmain(int argc, wchar_t** argv) {
  if (argc != 3) return 2;
  const std::wstring executable=argv[1], logs=argv[2];
  // session_portable_directory_for may create these two folders. Require
  // them to exist first so this invocation is strictly read-only.
  const auto slash=logs.find_last_of(L"\\/");
  if (slash==std::wstring::npos || !session_plain_directory(logs)
      || !session_plain_directory(logs.substr(0,slash))) return 3;
  bool configured=false;
  const auto directory=session_portable_directory_for(executable,&configured);
  std::printf("PORTABLE_CONFIGURED=%s\nPORTABLE_PATH_MATCH=%s\n",
    configured?"YES":"NO", directory==logs+L"\\"?"YES":"NO");
  nba2k27::ResolverReport report{};
  const auto start=std::chrono::steady_clock::now();
  const bool resolved=nba2k27::resolve_provider_layout(executable.c_str(),&report);
  const auto elapsed=std::chrono::duration_cast<std::chrono::milliseconds>(
    std::chrono::steady_clock::now()-start).count();
  std::printf("FILE_RESOLVED=%s\nCOMPLETE=%s\nFAILURE=%s\nELAPSED_MS=%lld\n",
    resolved?"YES":"NO",report.complete?"YES":"NO",report.failure,
    static_cast<long long>(elapsed));
  return configured && directory==logs+L"\\" && resolved && report.complete?0:1;
}
