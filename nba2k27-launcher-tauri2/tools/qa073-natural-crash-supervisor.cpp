// TEST-ONLY natural-crash watcher: creates one allowlisted child, no debug flag,
// no OpenProcess, no attachment, no global registry/debug settings.
#include "qa073-natural-crash-channel.hpp"
#include <dbghelp.h>
#include <wincrypt.h>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <iomanip>
#include <sstream>
#include <vector>
#include <string>
namespace fs=std::filesystem;
struct Handle{HANDLE h{};~Handle(){if(h&&h!=INVALID_HANDLE_VALUE)CloseHandle(h);}operator HANDLE()const{return h;}};
std::wstring quote(const std::wstring&s){std::wstring o=L"\"";unsigned n=0;for(auto c:s){if(c==L'\\'){++n;continue;}o.append(c==L'"'?2*n+1:n,L'\\');n=0;o+=c;}o.append(2*n,L'\\');return o+L'"';}
struct ChildCleanup{HANDLE process{},report{};bool complete{};~ChildCleanup(){if(!complete&&process&&WaitForSingleObject(process,0)==WAIT_TIMEOUT){const bool stopped=TerminateProcess(process,0xE0730001)!=FALSE;const auto waited=WaitForSingleObject(process,5000);const std::string text="OWNED_CHILD_FAILURE_CLEANUP terminate="+std::to_string(stopped)+" wait="+std::to_string(waited)+"\r\n";DWORD n{};WriteFile(report,text.data(),DWORD(text.size()),&n,nullptr);FlushFileBuffers(report);}}};
std::wstring hexhandle(HANDLE h){std::wostringstream o;o<<std::hex<<reinterpret_cast<std::uintptr_t>(h);return o.str();}
std::string sha(const fs::path&p){Handle file{CreateFileW(p.c_str(),GENERIC_READ,FILE_SHARE_READ,nullptr,OPEN_EXISTING,FILE_ATTRIBUTE_NORMAL,nullptr)};if(file.h==INVALID_HANDLE_VALUE)throw std::runtime_error("hash open");HCRYPTPROV provider{};HCRYPTHASH hash{};
 if(!CryptAcquireContextW(&provider,nullptr,nullptr,PROV_RSA_AES,CRYPT_VERIFYCONTEXT)||!CryptCreateHash(provider,CALG_SHA_256,0,0,&hash))throw std::runtime_error("hash create");
 unsigned char b[65536];DWORD n{};bool ok=true;for(;;){if(!ReadFile(file,b,sizeof b,&n,nullptr)){ok=false;break;}if(!n)break;if(!CryptHashData(hash,b,n,0)){ok=false;break;}}
 unsigned char out[32]{};DWORD len=32;ok=ok&&CryptGetHashParam(hash,HP_HASHVAL,out,&len,0);CryptDestroyHash(hash);CryptReleaseContext(provider,0);if(!ok||len!=32)throw std::runtime_error("hash read");std::ostringstream o;for(auto c:out)o<<std::hex<<std::setw(2)<<std::setfill('0')<<unsigned(c);return o.str();}
bool same(const fs::path&a,const fs::path&b){const auto x=a.lexically_normal().make_preferred().wstring(),y=b.lexically_normal().make_preferred().wstring();return CompareStringOrdinal(x.c_str(),int(x.size()),y.c_str(),int(y.size()),TRUE)==CSTR_EQUAL;}
BOOL CALLBACK cancel(PVOID context,const PMINIDUMP_CALLBACK_INPUT input,PMINIDUMP_CALLBACK_OUTPUT output){if(input->CallbackType==CancelCallback){output->CheckCancel=TRUE;output->Cancel=GetTickCount64()>*static_cast<ULONGLONG*>(context);}return TRUE;}
int wmain(int argc,wchar_t**argv){try{
 if(argc<5){std::cerr<<"target prefix firstArg enable|disable|noack [more args]\n";return 64;}
 const auto root=fs::canonical(L"C:\\Users\\carrn\\CodexProjects\\2kconversiontool\\nba2k27-launcher-tauri2");
 const auto exe=fs::canonical(argv[1]);const auto fixture=root/L"artifacts/qa073-natural-crash-fixture.exe";
 const auto test=root/L"target-qa054/debug/deps/nba2k27_launcher_tauri2-281ce2d259e8f3fd.exe";
 if(!same(exe,fixture)&&!same(exe,test))throw std::runtime_error("target outside exact allowlist");
 const fs::path prefix=fs::absolute(argv[2]);if(!same(fs::canonical(prefix.parent_path()),fs::canonical(root/L"artifacts"))||(prefix.filename().wstring().find(L"qa073-natural-")!=0&&prefix.filename().wstring().find(L"qa074-natural-")!=0))throw std::runtime_error("output outside private workspace");
 const auto report=fs::path(prefix.wstring()+L".txt"),dump=fs::path(prefix.wstring()+L".dmp");
 Handle reportFile{CreateFileW(report.c_str(),GENERIC_WRITE,0,nullptr,CREATE_NEW,FILE_ATTRIBUTE_NORMAL,nullptr)};if(reportFile.h==INVALID_HANDLE_VALUE)throw std::runtime_error("report CREATE_NEW");
 auto log=[&](const std::string&s){const auto line=s+"\r\n";DWORD n{};if(!WriteFile(reportFile,line.data(),DWORD(line.size()),&n,nullptr)||n!=line.size()||!FlushFileBuffers(reportFile))throw std::runtime_error("report durable write");std::cout<<s<<"\n";};
 const auto hash=sha(exe);log("TEST_ONLY_NO_DEBUGGER target="+exe.string()+" sha256="+hash);const auto pdb=fs::path(exe).replace_extension(L".pdb");if(fs::exists(pdb))log("PDB="+pdb.string()+" sha256="+sha(pdb));
 HMODULE dll=LoadLibraryExW(L"dbghelp.dll",nullptr,LOAD_LIBRARY_SEARCH_SYSTEM32);if(!dll)throw std::runtime_error("system32 dbghelp missing");
 const auto writeDump=reinterpret_cast<decltype(&MiniDumpWriteDump)>(GetProcAddress(dll,"MiniDumpWriteDump"));const auto readStream=reinterpret_cast<decltype(&MiniDumpReadDumpStream)>(GetProcAddress(dll,"MiniDumpReadDumpStream"));if(!writeDump||!readStream)throw std::runtime_error("dump API missing");
 wchar_t dllPath[32768]{};GetModuleFileNameW(dll,dllPath,32768);log("DBGHELP="+fs::path(dllPath).string()+" sha256="+sha(dllPath));
 SECURITY_ATTRIBUTES sa{sizeof(sa),nullptr,TRUE};Handle mapping{CreateFileMappingW(INVALID_HANDLE_VALUE,&sa,PAGE_READWRITE,0,sizeof(natural_crash::Channel),nullptr)},ready{CreateEventW(&sa,TRUE,FALSE,nullptr)},request{CreateEventW(&sa,TRUE,FALSE,nullptr)},ack{CreateEventW(&sa,TRUE,FALSE,nullptr)};
 if(!mapping||!ready||!request||!ack)throw std::runtime_error("channel handles");auto*c=static_cast<natural_crash::Channel*>(MapViewOfFile(mapping,FILE_MAP_READ|FILE_MAP_WRITE,0,0,sizeof(natural_crash::Channel)));if(!c)throw std::runtime_error("channel map");c->signature=natural_crash::magic;c->version=1;
 const std::wstring policy=argv[4];if(policy!=L"enable"&&policy!=L"disable"&&policy!=L"noack")throw std::runtime_error("invalid policy");
 const wchar_t*names[]={L"QA073_CRASH_MAPPING",L"QA073_CRASH_READY",L"QA073_CRASH_REQUEST",L"QA073_CRASH_ACK"};HANDLE inherited[]={mapping.h,ready.h,request.h,ack.h};
 for(unsigned n=0;n<4;++n)if(!SetEnvironmentVariableW(names[n],policy==L"disable"?nullptr:hexhandle(inherited[n]).c_str()))throw std::runtime_error("env channel");
 SIZE_T attrSize{};InitializeProcThreadAttributeList(nullptr,1,0,&attrSize);std::vector<unsigned char> attrs(attrSize);auto*list=reinterpret_cast<LPPROC_THREAD_ATTRIBUTE_LIST>(attrs.data());if(!InitializeProcThreadAttributeList(list,1,0,&attrSize)||!UpdateProcThreadAttribute(list,0,PROC_THREAD_ATTRIBUTE_HANDLE_LIST,inherited,sizeof inherited,nullptr,nullptr))throw std::runtime_error("handle allowlist");
 STARTUPINFOEXW startup{};startup.StartupInfo.cb=sizeof startup;startup.lpAttributeList=list;PROCESS_INFORMATION child{};auto command=quote(exe.wstring())+L" "+quote(argv[3]);if(same(exe,fixture)&&argc!=5)throw std::runtime_error("fixture takes one exact mode");for(int n=5;n<argc;++n)command+=L" "+quote(argv[n]);
 const bool launched=CreateProcessW(exe.c_str(),command.data(),nullptr,nullptr,TRUE,EXTENDED_STARTUPINFO_PRESENT,nullptr,root.c_str(),&startup.StartupInfo,&child)!=FALSE;
 for(auto name:names)SetEnvironmentVariableW(name,nullptr);DeleteProcThreadAttributeList(list);if(!launched)throw std::runtime_error("CreateProcess child");Handle process{child.hProcess},thread{child.hThread};ChildCleanup cleanup{process.h,reportFile.h,false};
 FILETIME birth{},exit{},kernel{},user{};if(!GetProcessTimes(process,&birth,&exit,&kernel,&user))throw std::runtime_error("retained child birth");c->pid=child.dwProcessId;c->birth=(std::uint64_t(birth.dwHighDateTime)<<32)|birth.dwLowDateTime;MemoryBarrier();SetEvent(ready);log("CHILD_PID="+std::to_string(c->pid)+" BIRTH="+std::to_string(c->birth));if(sha(exe)!=hash)throw std::runtime_error("target changed during launch");
 HANDLE waits[]={process.h,request.h};auto wait=WaitForMultipleObjects(2,waits,FALSE,180000);bool dumped=false;
 if(wait==WAIT_OBJECT_0+1){MemoryBarrier();if(c->claimed!=1||!c->tid||!c->pointers||GetProcessId(process)!=c->pid)throw std::runtime_error("invalid fatal signal");log("UNHANDLED_SIGNAL_TID="+std::to_string(c->tid));
 if(policy==L"noack"){log("NOACK_FIXTURE_NO_DUMP");}else{
 Handle out{CreateFileW(dump.c_str(),GENERIC_READ|GENERIC_WRITE,0,nullptr,CREATE_NEW,FILE_ATTRIBUTE_NORMAL,nullptr)};if(out.h==INVALID_HANDLE_VALUE)throw std::runtime_error("dump CREATE_NEW");
 MINIDUMP_EXCEPTION_INFORMATION exception{c->tid,reinterpret_cast<EXCEPTION_POINTERS*>(c->pointers),TRUE};ULONGLONG deadline=GetTickCount64()+15000;MINIDUMP_CALLBACK_INFORMATION callback{cancel,&deadline};
 const auto type=static_cast<MINIDUMP_TYPE>(MiniDumpNormal|MiniDumpWithThreadInfo|MiniDumpWithUnloadedModules);
 dumped=writeDump(process,c->pid,out,type,&exception,nullptr,&callback)!=FALSE;const auto err=dumped?0:GetLastError();if(!FlushFileBuffers(out))throw std::runtime_error("dump flush failed");log("DUMP_OK="+std::to_string(dumped)+" ERROR="+std::to_string(err));if(!dumped)throw std::runtime_error("dump failed");
 Handle dm{CreateFileMappingW(out,nullptr,PAGE_READONLY,0,0,nullptr)};auto*view=MapViewOfFile(dm,FILE_MAP_READ,0,0,0);if(!view)throw std::runtime_error("dump map");PMINIDUMP_DIRECTORY directory{};PVOID stream{};ULONG bytes{};
 const bool valid=readStream(view,ExceptionStream,&directory,&stream,&bytes)&&bytes>=sizeof(MINIDUMP_EXCEPTION_STREAM);if(!valid)throw std::runtime_error("no exception stream");auto*e=static_cast<MINIDUMP_EXCEPTION_STREAM*>(stream);log("DUMP_EXCEPTION="+std::to_string(e->ExceptionRecord.ExceptionCode)+" THREAD="+std::to_string(e->ThreadId)+" CONTEXT_BYTES="+std::to_string(e->ThreadContext.DataSize));if(same(exe,fixture)&&e->ExceptionRecord.NumberParameters>=2)log("AV_ACCESS_KIND="+std::to_string(e->ExceptionRecord.ExceptionInformation[0])+" FAULT_ADDRESS="+std::to_string(e->ExceptionRecord.ExceptionInformation[1]));if(e->ThreadId!=c->tid||!e->ThreadContext.DataSize)throw std::runtime_error("dump wrong context");if(same(exe,fixture)&&e->ExceptionRecord.ExceptionCode!=EXCEPTION_ACCESS_VIOLATION)throw std::runtime_error("fixture wrong exception");UnmapViewOfFile(view);SetEvent(ack);
 }}
 else if(wait!=WAIT_OBJECT_0)throw std::runtime_error("child watchdog timeout");
 if(WaitForSingleObject(process,30000)!=WAIT_OBJECT_0)throw std::runtime_error("fatal child failed bounded exit");DWORD code{};GetExitCodeProcess(process,&code);log("EXIT_CODE="+std::to_string(code)+" CLAIMED="+std::to_string(c->claimed)+" DUMP="+std::to_string(dumped));
 const std::wstring mode=argv[3];if(same(exe,fixture)){const bool fatal=mode==L"unhandled"||mode==L"hardware"||mode==L"noack"||mode==L"disabled";if((fatal&&code!=EXCEPTION_ACCESS_VIOLATION)||(!fatal&&code!=0))throw std::runtime_error("fixture exit wrong");const bool expected=fatal&&policy==L"enable";if(dumped!=expected||(!fatal&&c->claimed))throw std::runtime_error("fixture dump behavior wrong");}
 cleanup.complete=true;UnmapViewOfFile(c);FreeLibrary(dll);return 0;
 }catch(const std::exception&e){std::cerr<<"REFUSED/FAILED: "<<e.what()<<"\n";return 1;}}
