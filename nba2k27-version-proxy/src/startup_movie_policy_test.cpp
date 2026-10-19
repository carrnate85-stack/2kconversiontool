#include "startup_movie_test.hpp"
#include <cassert>
int main() {
 namespace m=startup_movie_test;
 m::enable_movie=true;
 auto code=static_cast<unsigned char*>(VirtualAlloc(nullptr,4096,MEM_COMMIT|MEM_RESERVE,PAGE_READWRITE));assert(code);
 const unsigned char instruction[]{0xc6,0x86,0xa0,0x0e,0,0,1};memcpy(code,instruction,7);
 DWORD old=0;assert(VirtualProtect(code,4096,PAGE_EXECUTE_READ,&old));
 m::site=reinterpret_cast<std::uintptr_t>(code);m::site_protection=PAGE_EXECUTE_READ;
 std::array<unsigned char,0xea8> object{};std::uintptr_t vt=0x12345678;m::expected_vtable=vt;memcpy(object.data(),&vt,8);
 float timer=1;memcpy(object.data()+0xe94,&timer,4);object[0xea0]=1;
 CONTEXT ctx{};ctx.Rsi=reinterpret_cast<DWORD64>(object.data());ctx.Rip=m::site+1;
 EXCEPTION_RECORD record{};record.ExceptionCode=EXCEPTION_BREAKPOINT;record.ExceptionAddress=code;
 EXCEPTION_POINTERS e{&record,&ctx};
 assert(m::replace(0xc6,0xcc));assert(m::exception(&e)==EXCEPTION_CONTINUE_EXECUTION);
 assert(object[0xea0]==0&&ctx.Rip==m::site+7&&m::hits==1&&m::changed==1&&m::restored==1);
 assert(m::bytes(m::site,instruction,7));
 m::hits=0;m::changed=0;m::restored=0;object[0xea0]=1;m::expected_vtable=vt+1;
 assert(m::replace(0xc6,0xcc));assert(m::exception(&e)==EXCEPTION_CONTINUE_EXECUTION);
 assert(object[0xea0]==1&&ctx.Rip==m::site&&m::changed==0&&m::restored==1);
 record.ExceptionAddress=code+1;assert(m::exception(&e)==EXCEPTION_CONTINUE_SEARCH);
 // Exercise Windows exception dispatch and actual instruction continuation,
 // using only a generated local fixture (not game code or game memory).
 assert(VirtualProtect(code,4096,PAGE_READWRITE,&old));
 const unsigned char fixture[]{0x56,0x48,0x89,0xce,0xc6,0x86,0xa0,0x0e,0,0,1,0x5e,0xc3};
 memcpy(code,fixture,sizeof(fixture));assert(VirtualProtect(code,4096,PAGE_EXECUTE_READ,&old));
 FlushInstructionCache(GetCurrentProcess(),code,sizeof(fixture));
 m::site=reinterpret_cast<std::uintptr_t>(code+4);m::hits=0;m::changed=0;m::restored=0;m::expected_vtable=vt;
 auto veh=AddVectoredExceptionHandler(1,m::exception);assert(veh);assert(m::replace(0xc6,0xcc));
 reinterpret_cast<void(*)(void*)>(code)(object.data());
 assert(object[0xea0]==0&&m::hits==1&&m::changed==1&&m::restored==1);
 reinterpret_cast<void(*)(void*)>(code)(object.data());
 assert(object[0xea0]==1&&m::hits==1); // original instruction, no second hook
 m::hits=0;m::changed=0;m::restored=0;m::applied=0;m::enable_movie=false;
 assert(m::replace(0xc6,0xcc));reinterpret_cast<void(*)(void*)>(code)(object.data());
 assert(object[0xea0]==1&&m::hits==1&&m::changed==0&&m::applied==1&&m::restored==1);
 assert(RemoveVectoredExceptionHandler(veh));
 wchar_t tempdir[MAX_PATH]{},tempfile[MAX_PATH]{};
 assert(GetTempPathW(MAX_PATH,tempdir));assert(GetTempFileNameW(tempdir,L"smf",0,tempfile));
 m::fault_file=CreateFileW(tempfile,GENERIC_READ|GENERIC_WRITE,0,nullptr,OPEN_EXISTING,FILE_ATTRIBUTE_TEMPORARY|FILE_FLAG_DELETE_ON_CLOSE,nullptr);
 assert(m::fault_file!=INVALID_HANDLE_VALUE);m::faults=0;
 record.ExceptionCode=EXCEPTION_ACCESS_VIOLATION;record.NumberParameters=2;record.ExceptionInformation[0]=0;record.ExceptionInformation[1]=0x18;
 ctx.Rip=reinterpret_cast<DWORD64>(code);ctx.Rsp=0;ctx.Rcx=0;
 for(int i=0;i<12;++i)assert(m::capture_fault(&e)==EXCEPTION_CONTINUE_SEARCH);
 LARGE_INTEGER length{};assert(GetFileSizeEx(m::fault_file,&length));assert(length.QuadPart==8*sizeof(m::FaultRecord));
 SetFilePointer(m::fault_file,0,nullptr,FILE_BEGIN);m::FaultRecord captured{};DWORD got=0;
 assert(ReadFile(m::fault_file,&captured,sizeof(captured),&got,nullptr)&&got==sizeof(captured));
 assert(captured.magic==0x32464d53&&captured.code==EXCEPTION_ACCESS_VIOLATION&&captured.address==0x18&&captured.rip==ctx.Rip);
 assert(ctx.Rip==reinterpret_cast<DWORD64>(code)&&ctx.Rcx==0);
 CloseHandle(m::fault_file);m::fault_file=INVALID_HANDLE_VALUE;
 VirtualFree(code,0,MEM_RELEASE);
 puts("PASS: local-fixture exception dispatch, single-use interception, restoration, invalid-object refusal, bounded fault logging without fault suppression.");
}
