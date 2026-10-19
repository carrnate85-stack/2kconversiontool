// Synthetic debugger self-test only. Never accesses another process or file.
#include <windows.h>
__declspec(noinline) void controlled_exception() {
    const ULONG_PTR parameters[2]={0,0xBAD000};
    RaiseException(EXCEPTION_ACCESS_VIOLATION,0,2,parameters);
}
int main() {
    SetErrorMode(SEM_FAILCRITICALERRORS|SEM_NOGPFAULTERRORBOX);
    controlled_exception();
    return 99; // Unreachable unless a debugger incorrectly swallows the AV.
}
