#include "../native/roster_appearance_v2_win.hpp"
#include <cstdio>
int main(){
 using roster_appearance_v2::win_detail::cas32;
 auto check=[](bool value,const char*label){std::fprintf(stdout,"%s=%s\n",label,value?"PASS":"FAIL");std::fflush(stdout);return value;};
 alignas(4) std::uint32_t word=0x80018002,seen=0;
 std::fprintf(stdout,"ENTER\n");std::fflush(stdout);
 if(!check(cas32(reinterpret_cast<std::uintptr_t>(&word),word,0x80038004,seen),"cas-call"))return 1;
 if(!check(seen==0x80018002&&word==0x80038004,"cas-values"))return 2;
 if(!check(cas32(reinterpret_cast<std::uintptr_t>(&word),0,1,seen),"conflict-call"))return 3;
 if(!check(seen==word&&word==0x80038004,"conflict-values"))return 4;
 void*denied=VirtualAlloc(nullptr,4096,MEM_RESERVE|MEM_COMMIT,PAGE_NOACCESS);
 if(!check(denied!=nullptr,"protected-allocation"))return 5;
 if(!check(!cas32(reinterpret_cast<std::uintptr_t>(denied),0,1,seen),"caught-protected-cas"))return 6;
 if(!check(VirtualFree(denied,0,MEM_RELEASE)!=0,"release"))return 7;
 return 0;
}
