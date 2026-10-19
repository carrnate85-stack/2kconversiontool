// Read-only exact-build startup observer. No game calls, injection or writes.
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <tlhelp32.h>
#include <array>
#include <cstring>
#include <fstream>
#include <iostream>
#include <string>
#include <filesystem>
#include <stdexcept>

struct Handle {HANDLE h=INVALID_HANDLE_VALUE;~Handle(){if(h&&h!=INVALID_HANDLE_VALUE)CloseHandle(h);}};
struct BuildProfile {DWORD timestamp,imageSize;uintptr_t anchor,manager;};
constexpr std::array<BuildProfile,2> kBuilds{{
  {0x6A974D13,0x368C3000,0x4B451E3,0xBCE8FC8},
  {0x6AA2013D,0x36558000,0x4B47AE3,0xBCED2C8}
}};
constexpr const BuildProfile* profile(DWORD timestamp,DWORD imageSize){
  for(const auto& build:kBuilds)if(build.timestamp==timestamp&&build.imageSize==imageSize)return &build;
  return nullptr;
}
static_assert(profile(0x6A974D13,0x368C3000)!=nullptr);
static_assert(profile(0x6AA2013D,0x36558000)!=nullptr);
static_assert(profile(0x6AA2013D,0x368C3000)==nullptr);
static_assert(profile(0x6A974D13,0x36558000)==nullptr);
static_assert(profile(0,0)==nullptr);
template<class T>T read(HANDLE h,uintptr_t p){T v{};SIZE_T n=0;if(!ReadProcessMemory(h,reinterpret_cast<void*>(p),&v,sizeof(v),&n)||n!=sizeof(v))throw std::runtime_error("Read unavailable");return v;}
struct Stable {uintptr_t pointer=0;ULONGLONG since=0;bool update(uintptr_t p,bool stock,ULONGLONG now,ULONGLONG stable_ms=1000){if(!p||!stock){pointer=0;since=0;return false;}if(pointer!=p){pointer=p;since=now;}return now-since>=stable_ms;}};
int wmain(int argc,wchar_t**argv){
  if(argc==2&&std::wstring(argv[1])==L"--self-test"){
    Stable s;if(s.update(12,true,0)||s.update(12,true,999)||!s.update(12,true,1000)||s.update(13,true,1001)||s.update(13,false,2002)||s.update(13,true,3003))return 1;
    Stable early;if(early.update(12,true,0,250)||early.update(12,true,249,250)||!early.update(12,true,250,250))return 1;
    std::cout<<"PASS: stable-manager readiness\n";return 0;
  }
  try{
    const bool check=argc==5&&std::wstring(argv[4])==L"--check-once";
    const bool custom_stability=argc==6&&std::wstring(argv[4])==L"--stable-ms";
    ULONGLONG stable_ms=1000;
    if(custom_stability){wchar_t*end=nullptr;const auto parsed=std::wcstoul(argv[5],&end,10);if(!end||*end||parsed<50||parsed>2000)throw std::runtime_error("Invalid stable readiness interval");stable_ms=parsed;}
    if((argc!=4&&!check&&!custom_stability)||std::wstring(argv[2])!=L"--armed-report")throw std::runtime_error("Expected game executable and --armed-report path");
    const std::filesystem::path exe=std::filesystem::absolute(argv[1]);
    std::ifstream input(exe,std::ios::binary);std::array<unsigned char,4096> disk{};
    if(!input.read(reinterpret_cast<char*>(disk.data()),disk.size()))throw std::runtime_error("Cannot read executable header");
    DWORD pe=0;std::memcpy(&pe,disk.data()+0x3c,4);
    if(pe>456||disk[0]!='M'||disk[1]!='Z')throw std::runtime_error("Invalid PE header");
    DWORD signature=0,timestamp=0,size=0;std::memcpy(&signature,disk.data()+pe,4);std::memcpy(&timestamp,disk.data()+pe+8,4);std::memcpy(&size,disk.data()+pe+80,4);
    const auto* build=profile(timestamp,size);
    if(signature!=0x4550||!build)throw std::runtime_error("Unsupported game build");
    const auto kAnchor=build->anchor,kManager=build->manager;
    const auto armedPath=std::filesystem::absolute(argv[3]);
    std::ifstream armedFile(armedPath);std::string line;DWORD pid=0;bool armed=false;
    while(std::getline(armedFile,line)){if(line.rfind("PID=",0)==0)pid=std::stoul(line.substr(4));if(line=="AUDIT_STATE=ARMED\r"||line=="AUDIT_STATE=ARMED")armed=true;}
    if(!pid||!armed)throw std::runtime_error("No armed session");
    Handle process{OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION|PROCESS_VM_READ,FALSE,pid)};
    if(!process.h)throw std::runtime_error("Cannot open game read-only");
    std::array<wchar_t,32768> target{};DWORD length=static_cast<DWORD>(target.size());
    if(!QueryFullProcessImageNameW(process.h,0,target.data(),&length)||_wcsicmp(target.data(),exe.c_str()))throw std::runtime_error("Armed PID executable mismatch");
    uintptr_t base=0;Stable stable;const auto started=GetTickCount64();
    while(GetTickCount64()-started<60000){
      DWORD exit=0;if(!GetExitCodeProcess(process.h,&exit)||exit!=STILL_ACTIVE)throw std::runtime_error("Game exited before readiness");
      if(!base){Handle snapshot{CreateToolhelp32Snapshot(TH32CS_SNAPMODULE|TH32CS_SNAPMODULE32,pid)};MODULEENTRY32W entry{};entry.dwSize=sizeof(entry);
        if(snapshot.h!=INVALID_HANDLE_VALUE&&Module32FirstW(snapshot.h,&entry))do{if(!_wcsicmp(entry.szExePath,exe.c_str())){base=reinterpret_cast<uintptr_t>(entry.modBaseAddr);break;}}while(Module32NextW(snapshot.h,&entry));
        if(!base){Sleep(50);continue;}}
      try{
        auto live=read<std::array<unsigned char,512>>(process.h,base);std::array<unsigned char,512> expected{};std::memcpy(expected.data(),disk.data(),512);uint64_t imageBase=0,preferred=0;
        std::memcpy(&imageBase,live.data()+pe+48,8);std::memcpy(&preferred,disk.data()+pe+48,8);
        if(imageBase!=base&&imageBase!=preferred)throw std::logic_error("Runtime image base mismatch");
        std::memcpy(expected.data()+pe+48,&imageBase,8);if(live!=expected)throw std::logic_error("Runtime header mismatch");
        const auto anchor=read<std::array<unsigned char,7>>(process.h,base+kAnchor);int32_t displacement=0;std::memcpy(&displacement,anchor.data()+3,4);
        if(anchor[0]!=0x48||anchor[1]!=0x8b||anchor[2]!=0x05||static_cast<int64_t>(kAnchor+7)+displacement!=static_cast<int64_t>(kManager))throw std::logic_error("Manager anchor mismatch");
        const auto manager=read<uintptr_t>(process.h,base+kManager);const auto stock=manager?read<uint32_t>(process.h,manager+0x1E0):0;
        if(check){std::cout<<"{\"verified\":true,\"pid\":"<<pid<<",\"stockDefault\":"<<(stock==0xC7E442DC?"true":"false")<<",\"memoryWrites\":0}\n";return 0;}
        if(manager!=read<uintptr_t>(process.h,base+kManager)){stable.update(0,false,GetTickCount64());continue;}
        if(stable.update(manager,stock==0xC7E442DC,GetTickCount64(),stable_ms)){std::cout<<"{\"ready\":true,\"pid\":"<<pid<<",\"waitMs\":"<<GetTickCount64()-started<<",\"stableMs\":"<<stable_ms<<",\"memoryWrites\":0,\"gameCalls\":0}\n";return 0;}
      }catch(const std::runtime_error&){stable.update(0,false,GetTickCount64());}
      Sleep(50);
    }
    throw std::runtime_error("Read-only readiness timed out; activation refused");
  }catch(const std::exception&e){std::cerr<<e.what()<<"\n";return 1;}
}
