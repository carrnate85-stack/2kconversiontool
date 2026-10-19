// Deterministic file-only reproduction of the unchanged header's create race.
#include <Windows.h>
#include <atomic>
#include <thread>
#include <cstdio>
static std::atomic<unsigned> arrivals{0};
static BOOL synchronized_create(LPCWSTR path,LPSECURITY_ATTRIBUTES attributes) {
  arrivals.fetch_add(1);
  while(arrivals.load()<2)Sleep(1);
  return CreateDirectoryW(path,attributes);
}
#define CreateDirectoryW synchronized_create
#include "../../nba2k27-version-proxy/src/session_diagnostics.hpp"
#undef CreateDirectoryW
int wmain(int argc,wchar_t** argv) {
  if(argc!=2)return 2;
  const std::wstring race=std::wstring(argv[1])+L"\\race";
  const std::wstring prepared=std::wstring(argv[1])+L"\\prepared";
  if(GetFileAttributesW(race.c_str())!=INVALID_FILE_ATTRIBUTES)return 3;
  bool first=false,second=false;
  std::thread a([&]{first=session_make_plain_directory(race);});
  std::thread b([&]{second=session_make_plain_directory(race);});
  a.join();b.join();
  const unsigned failures=static_cast<unsigned>(!first)+static_cast<unsigned>(!second);
  if(!CreateDirectoryW(prepared.c_str(),nullptr))return 4;
  arrivals.store(0);
  std::thread c([&]{first=session_make_plain_directory(prepared);});
  std::thread d([&]{second=session_make_plain_directory(prepared);});
  c.join();d.join();
  std::printf("UNPREPARED_FALSE_FAILURES=%u\nPREPARED_BOTH_PASS=%s\n"
    "PREPARED_CREATE_CALLS=%u\nGAME_ACCESS=NONE\n",failures,
    first&&second?"YES":"NO",arrivals.load());
  return failures==1&&session_plain_directory(race)&&first&&second&&arrivals.load()==0?0:1;
}
