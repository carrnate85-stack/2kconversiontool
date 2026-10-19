#include <Windows.h>
#include <TlHelp32.h>

#include <array>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <cwchar>

namespace {
constexpr std::uint32_t kTimestamp = 0x6aa2013d;
constexpr std::uint32_t kImageSize = 0x36558000;
constexpr std::uintptr_t kOperandInstructionRva = 0x3051647;
constexpr std::uintptr_t kOperandByteRva = 0x305164d;
constexpr std::uintptr_t kMenuTableRva = 0x0ac77660;
constexpr std::uintptr_t kStartupStateRva = 0x0a201970;
constexpr std::array<unsigned char, 7> kOriginal{0xc6,0x86,0xa0,0x0e,0x00,0x00,0x01};
constexpr std::array<unsigned char, 7> kArmed{0xc6,0x86,0xa0,0x0e,0x00,0x00,0x00};

struct Anchor { std::uintptr_t rva; const unsigned char *bytes; std::size_t size; };
constexpr unsigned char kAnchorMenu[]{0x4c,0x8d,0x25,0xc6,0x3c,0xc2,0x07};
constexpr unsigned char kAnchorMovie[]{0x48,0x8d,0x05,0x88,0x68,0xfe,0x03};
constexpr unsigned char kAnchorList[]{0x48,0x8d,0x05,0xdd,0x9d,0xb8,0x09};
constexpr std::array<Anchor,3> kAnchors{{
  {0x3053993,kAnchorMenu,sizeof(kAnchorMenu)},
  {0x38679cb,kAnchorMovie,sizeof(kAnchorMovie)},
  {0x0bda47c,kAnchorList,sizeof(kAnchorList)},
}};

struct Process { DWORD pid=0; HANDLE handle=nullptr; std::uintptr_t base=0; };

bool anti_cheat_running() {
  HANDLE snapshot=CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS,0);
  if(snapshot==INVALID_HANDLE_VALUE)return true;
  PROCESSENTRY32W entry{};entry.dwSize=sizeof(entry);bool found=false;
  if(Process32FirstW(snapshot,&entry))do{
    if(wcsstr(entry.szExeFile,L"EasyAntiCheat")||!_wcsicmp(entry.szExeFile,L"start_protected_game.exe")){found=true;break;}
  }while(Process32NextW(snapshot,&entry));
  CloseHandle(snapshot);return found;
}

DWORD find_game() {
  HANDLE snapshot=CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS,0);
  if(snapshot==INVALID_HANDLE_VALUE)return 0;
  PROCESSENTRY32W entry{};entry.dwSize=sizeof(entry);DWORD pid=0;
  if(Process32FirstW(snapshot,&entry))do{if(!_wcsicmp(entry.szExeFile,L"NBA2K27.exe")){pid=entry.th32ProcessID;break;}}while(Process32NextW(snapshot,&entry));
  CloseHandle(snapshot);return pid;
}

std::uintptr_t find_base(DWORD pid) {
  HANDLE snapshot=CreateToolhelp32Snapshot(TH32CS_SNAPMODULE|TH32CS_SNAPMODULE32,pid);
  if(snapshot==INVALID_HANDLE_VALUE)return 0;
  MODULEENTRY32W entry{};entry.dwSize=sizeof(entry);std::uintptr_t base=0;
  if(Module32FirstW(snapshot,&entry))do{if(!_wcsicmp(entry.szModule,L"NBA2K27.exe")){base=reinterpret_cast<std::uintptr_t>(entry.modBaseAddr);break;}}while(Module32NextW(snapshot,&entry));
  CloseHandle(snapshot);return base;
}

bool read_exact(HANDLE process,std::uintptr_t address,void *output,SIZE_T size){SIZE_T done=0;return ReadProcessMemory(process,reinterpret_cast<const void*>(address),output,size,&done)&&done==size;}

bool exact_build(const Process &process) {
  IMAGE_DOS_HEADER dos{};IMAGE_NT_HEADERS64 nt{};std::array<unsigned char,7> instruction{};
  if(!read_exact(process.handle,process.base,&dos,sizeof(dos))||dos.e_magic!=IMAGE_DOS_SIGNATURE||dos.e_lfanew<64||dos.e_lfanew>=4096||
     !read_exact(process.handle,process.base+dos.e_lfanew,&nt,sizeof(nt))||nt.Signature!=IMAGE_NT_SIGNATURE||
     nt.FileHeader.TimeDateStamp!=kTimestamp||nt.OptionalHeader.SizeOfImage!=kImageSize||
     !read_exact(process.handle,process.base+kOperandInstructionRva,instruction.data(),instruction.size())||instruction!=kOriginal)return false;
  for(const auto &anchor:kAnchors){std::array<unsigned char,8> value{};if(anchor.size>value.size()||!read_exact(process.handle,process.base+anchor.rva,value.data(),anchor.size))return false;for(std::size_t i=0;i<anchor.size;i++)if(value[i]!=anchor.bytes[i])return false;}
  return true;
}

bool menus_exist(const Process &process) {
  std::array<unsigned char,31*64> table{};
  if(!read_exact(process.handle,process.base+kMenuTableRva,table.data(),table.size()))return false;
  for(std::size_t i=0;i<31;i++){std::uint32_t kind=0;std::memcpy(&kind,table.data()+i*64,sizeof(kind));if(kind)return true;}
  return false;
}

bool set_operand(const Process &process,unsigned char expected,unsigned char value) {
  std::array<unsigned char,7> current{};
  if(!read_exact(process.handle,process.base+kOperandInstructionRva,current.data(),current.size()))return false;
  const auto &fingerprint=expected?kOriginal:kArmed;if(current!=fingerprint)return false;
  DWORD old=0;if(!VirtualProtectEx(process.handle,reinterpret_cast<void*>(process.base+kOperandByteRva),1,PAGE_EXECUTE_READWRITE,&old))return false;
  SIZE_T done=0;const bool wrote=WriteProcessMemory(process.handle,reinterpret_cast<void*>(process.base+kOperandByteRva),&value,1,&done)&&done==1&&FlushInstructionCache(process.handle,reinterpret_cast<void*>(process.base+kOperandInstructionRva),7);
  DWORD ignored=0;VirtualProtectEx(process.handle,reinterpret_cast<void*>(process.base+kOperandByteRva),1,old,&ignored);
  unsigned char verified=0;return wrote&&read_exact(process.handle,process.base+kOperandByteRva,&verified,1)&&verified==value;
}

bool operand_is(const Process &process,const std::array<unsigned char,7> &expected){std::array<unsigned char,7> current{};return read_exact(process.handle,process.base+kOperandInstructionRva,current.data(),current.size())&&current==expected;}

bool write_receipt(const wchar_t *path,const char *status,DWORD pid,bool armed,bool restored,const char *message){
  FILE *file=nullptr;if(_wfopen_s(&file,path,L"wb")||!file)return false;
  const int result=std::fprintf(file,"{\n  \"schema\": \"startup-presentation-v1\",\n  \"status\": \"%s\",\n  \"pid\": %lu,\n  \"armed\": %s,\n  \"restored\": %s,\n  \"message\": \"%s\"\n}\n",status,static_cast<unsigned long>(pid),armed?"true":"false",restored?"true":"false",message);
  return result>0&&std::fclose(file)==0;
}

bool write_phase(const wchar_t *path,int phase){if(!path)return true;FILE *file=nullptr;if(_wfopen_s(&file,path,L"wb")||!file)return false;const int result=std::fprintf(file,"%d\n",phase);return result>0&&std::fclose(file)==0;}

int run(const wchar_t *receipt,const wchar_t *phase_path){
  if(anti_cheat_running()){write_receipt(receipt,"rejected",0,false,false,"Anti-cheat process detected");return 2;}
  const ULONGLONG process_deadline=GetTickCount64()+120000;DWORD pid=0;while(!(pid=find_game())&&GetTickCount64()<process_deadline)Sleep(5);
  if(!pid){write_receipt(receipt,"timeout",0,false,false,"NBA 2K27 did not start");return 3;}
  Process process{};process.pid=pid;process.handle=OpenProcess(PROCESS_QUERY_INFORMATION|PROCESS_VM_READ|PROCESS_VM_WRITE|PROCESS_VM_OPERATION|SYNCHRONIZE,FALSE,pid);
  if(!process.handle){write_receipt(receipt,"rejected",pid,false,false,"Could not open the game process");return 4;}
  const ULONGLONG module_deadline=GetTickCount64()+30000;while(!(process.base=find_base(pid))&&GetTickCount64()<module_deadline&&WaitForSingleObject(process.handle,0)==WAIT_TIMEOUT)Sleep(2);
  if(!process.base||!exact_build(process)||anti_cheat_running()||menus_exist(process)){CloseHandle(process.handle);write_receipt(receipt,"rejected",pid,false,false,"Unsupported build, anti-cheat active, or startup already advanced");return 5;}
  bool pending=true; // Set before the write so every uncertain outcome attempts restoration.
  if(!set_operand(process,1,0)){
    const bool restored=operand_is(process,kOriginal)||(operand_is(process,kArmed)&&set_operand(process,0,1));pending=!restored;
    CloseHandle(process.handle);
    write_receipt(receipt,pending?"restore-failed":"rejected",pid,true,restored,pending?"Uncertain startup write could not be restored; close NBA 2K27":"Startup operand write failed and original bytes were verified");
    return pending?7:6;
  }
  write_receipt(receipt,"armed",pid,true,false,"Native startup movie trigger armed");
  const ULONGLONG deadline=GetTickCount64()+30000;
  int published_phase=0;
  while(GetTickCount64()<deadline&&WaitForSingleObject(process.handle,0)==WAIT_TIMEOUT&&!menus_exist(process)&&!anti_cheat_running()){
    int state=0;if(phase_path&&read_exact(process.handle,process.base+kStartupStateRva,&state,sizeof(state))){const int phase=state>=5?2:state>=3?1:0;if(phase>published_phase&&write_phase(phase_path,phase))published_phase=phase;}
    Sleep(2);
  }
  bool restored=false;
  if(WaitForSingleObject(process.handle,0)==WAIT_TIMEOUT&&pending){restored=set_operand(process,0,1);pending=!restored;}
  const bool alive=WaitForSingleObject(process.handle,0)==WAIT_TIMEOUT;CloseHandle(process.handle);
  if(pending&&alive){write_receipt(receipt,"restore-failed",pid,true,false,"Unexpected startup bytes retained; close NBA 2K27");return 7;}
  return write_receipt(receipt,restored?"restored":"process-exited",pid,true,restored,restored?"Original startup operand restored":"Game exited; process memory released")?0:8;
}
}

int wmain(int argc,wchar_t **argv){
  if(argc==2&&!wcscmp(argv[1],L"--self-test")){if(kOriginal[6]!=1||kArmed[6]!=0||kOperandByteRva!=kOperandInstructionRva+6||kMenuTableRva!=0x0ac77660)return 1;std::puts("PASS: exact build anchors, bounded trigger and restoration fingerprints configured");return 0;}
  if((argc!=3&&argc!=4)||wcscmp(argv[1],L"--wait-and-trigger")){std::fputs("Usage: NBA2K27StartupPresentation.exe --wait-and-trigger <receipt.json> [screen-phase.txt]\n",stderr);return 64;}
  return run(argv[2],argc==4?argv[3]:nullptr);
}
