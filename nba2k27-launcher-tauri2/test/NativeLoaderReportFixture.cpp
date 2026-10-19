// Synthetic host for the exact bundled DLL. Never uses a real game image or
// process: its PE build cannot satisfy the loader's runtime identity guards.
#include <Windows.h>
#include <cstdio>
#include <string>

int wmain(int argc,wchar_t** argv) {
  if(argc!=3)return 2;
  const std::wstring report=std::wstring(argv[2])+L"\\nba2k27_version_proxy.log";
  if(GetFileAttributesW(report.c_str())!=INVALID_FILE_ATTRIBUTES)return 3;
  const HMODULE module=LoadLibraryW(argv[1]);
  if(!module){std::printf("LOAD_ERROR=%lu\n",GetLastError());return 4;}
  for(unsigned attempt=0;attempt<50;++attempt){
    if(GetFileAttributesW(report.c_str())!=INVALID_FILE_ATTRIBUTES){
      std::printf("SYNTHETIC_REPORT=YES\nGAME_PROCESS_ACCESS=NONE\n");return 0;
    }
    Sleep(100);
  }
  std::printf("SYNTHETIC_REPORT=NO\nGAME_PROCESS_ACCESS=NONE\n");return 1;
}
