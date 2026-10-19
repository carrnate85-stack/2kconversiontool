// Synthetic standalone fixture only; does not access another process.
#define QA073_NATURAL_CRASH_CRT
#include "qa073-natural-crash-channel.hpp"
#include <cstring>
#include <cwchar>
__declspec(noinline) void fault(){const ULONG_PTR p[2]={0,0xBAD000};RaiseException(EXCEPTION_ACCESS_VIOLATION,0,2,p);}
int main(int argc,char**argv){
 SetErrorMode(SEM_FAILCRITICALERRORS|SEM_NOGPFAULTERRORBOX);
 const char*mode=argc>1?argv[1]:"normal";
 if(std::strcmp(mode,"handled")==0){__try{fault();}__except(EXCEPTION_EXECUTE_HANDLER){return 0;}}
 if(std::strcmp(mode,"subprocess")==0){wchar_t self[32768]{};if(!GetModuleFileNameW(nullptr,self,32768))return 8;wchar_t command[32768]{};if(swprintf(command,32768,L"\"%s\" unhandled",self)<0)return 9;STARTUPINFOW si{};si.cb=sizeof si;PROCESS_INFORMATION child{};if(!CreateProcessW(self,command,nullptr,nullptr,TRUE,0,nullptr,nullptr,&si,&child))return 10;WaitForSingleObject(child.hProcess,10000);DWORD code{};GetExitCodeProcess(child.hProcess,&code);CloseHandle(child.hThread);CloseHandle(child.hProcess);return code==EXCEPTION_ACCESS_VIOLATION?0:11;}
 if(std::strcmp(mode,"hardware")==0){*reinterpret_cast<volatile unsigned char*>(std::uintptr_t(0xBAD000))=1;return 99;}
 if(std::strcmp(mode,"unhandled")==0||std::strcmp(mode,"noack")==0||std::strcmp(mode,"disabled")==0){fault();return 99;}
 return 0;
}
