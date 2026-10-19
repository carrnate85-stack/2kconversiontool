// Offline test-executable debugger. Never attaches to an existing process.
// Usage: debug-test-crash.exe <target-qa054/debug/deps/test.exe> <report.txt> [test args]
#define NOMINMAX
#include <windows.h>
#include <dbghelp.h>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <map>
#include <vector>
#include <string>

std::wstring quoted(const std::wstring& text) {
    std::wstring out=L"\""; unsigned slashes=0;
    for(auto c:text) {
        if(c==L'\\'){++slashes;continue;}
        if(c==L'\"')out.append(slashes*2+1,L'\\');else out.append(slashes,L'\\');
        slashes=0;out.push_back(c);
    }
    out.append(slashes*2,L'\\');return out+L"\"";
}
std::string narrow(const std::wstring& w) {
    const int n=WideCharToMultiByte(CP_UTF8,0,w.data(),int(w.size()),nullptr,0,nullptr,nullptr);
    std::string out(n,0);WideCharToMultiByte(CP_UTF8,0,w.data(),int(w.size()),out.data(),n,nullptr,nullptr);return out;
}
std::wstring file_path(HANDLE file) {
    wchar_t buffer[32768]{};const auto n=GetFinalPathNameByHandleW(file,buffer,32768,FILE_NAME_NORMALIZED);
    auto path=n&&n<32768?std::wstring(buffer,n):L"<unknown>";
    if(path.starts_with(L"\\\\?\\"))path.erase(0,4);
    return path;
}
void address(std::ofstream& out,HANDLE process,DWORD64 pc) {
    alignas(SYMBOL_INFO) unsigned char storage[sizeof(SYMBOL_INFO)+2048]{};
    auto symbol=reinterpret_cast<SYMBOL_INFO*>(storage);symbol->SizeOfStruct=sizeof(SYMBOL_INFO);symbol->MaxNameLen=2047;
    DWORD64 displacement{};out<<"0x"<<std::hex<<pc;
    IMAGEHLP_MODULE64 module{};module.SizeOfStruct=sizeof(module);
    if(SymGetModuleInfo64(process,pc,&module))out<<" "<<module.ModuleName<<"+0x"<<(pc-module.BaseOfImage);
    if(SymFromAddr(process,pc,&displacement,symbol))out<<" "<<symbol->Name<<"+0x"<<displacement;
    IMAGEHLP_LINE64 line{};line.SizeOfStruct=sizeof(line);DWORD offset{};
    if(SymGetLineFromAddr64(process,pc,&offset,&line))out<<" "<<line.FileName<<":"<<std::dec<<line.LineNumber;
    out<<"\n";
}
int wmain(int argc,wchar_t** argv) {
    if(argc<3){std::cerr<<"Expected test executable and report path\n";return 64;}
    const auto exe=std::filesystem::canonical(argv[1]);
    const auto filename=exe.filename().wstring();
    // Fail closed: this tool can run only the offline Rust unit-test image.
    if(!filename.starts_with(L"nba2k27_launcher_tauri2-")||!filename.ends_with(L".exe")||
       exe.parent_path().filename()!=L"deps"||exe.parent_path().parent_path().filename()!=L"debug"||
       exe.parent_path().parent_path().parent_path().filename()!=L"target-qa054"){
        std::cerr<<"Refused: not the isolated QA054 test executable\n";return 65;
    }
    std::ofstream out{std::filesystem::path(argv[2])};
    if(!out)return 66;
    out<<std::unitbuf;
    out<<"Offline test-only debug child; no process attachment.\nEXE="<<narrow(exe.wstring())<<"\n";
    std::wstring cmd=quoted(exe.wstring());for(int i=3;i<argc;++i)cmd+=L" "+quoted(argv[i]);
    STARTUPINFOW startup{};startup.cb=sizeof(startup);PROCESS_INFORMATION child{};
    if(!CreateProcessW(exe.c_str(),cmd.data(),nullptr,nullptr,TRUE,DEBUG_ONLY_THIS_PROCESS,nullptr,nullptr,&startup,&child)){
        out<<"CreateProcessW failed "<<GetLastError()<<"\n";return 67;
    }
    out<<"PID="<<child.dwProcessId<<"\n";
    SymSetOptions(SYMOPT_UNDNAME|SYMOPT_DEFERRED_LOADS|SYMOPT_LOAD_LINES|SYMOPT_FAIL_CRITICAL_ERRORS);
    out<<"SYM_INITIALIZE_BEGIN\n";
    const bool symbols=SymInitializeW(child.hProcess,exe.parent_path().c_str(),FALSE)!=FALSE;
    out<<"SYM_INITIALIZE_END="<<symbols<<" ERROR="<<GetLastError()<<"\n";
    std::map<DWORD,HANDLE> threads;bool done=false;DWORD result=1;
    while(!done) {
        DEBUG_EVENT event{};
        if(!WaitForDebugEvent(&event,INFINITE)){out<<"WaitForDebugEvent failed "<<GetLastError()<<"\n";break;}
        DWORD status=DBG_CONTINUE;
        if(event.dwDebugEventCode==CREATE_PROCESS_DEBUG_EVENT){
            auto& c=event.u.CreateProcessInfo;threads[event.dwThreadId]=c.hThread;
            const auto base=reinterpret_cast<DWORD64>(c.lpBaseOfImage);
            const auto loaded=SymLoadModuleExW(child.hProcess,c.hFile,exe.c_str(),nullptr,base,0,nullptr,0);
            out<<"MAIN_SYMBOL_LOAD=0x"<<std::hex<<loaded<<" ERROR="<<std::dec<<GetLastError()<<"\n";
            out<<"MAIN_BASE=0x"<<std::hex<<base<<"\n";if(c.hFile)CloseHandle(c.hFile);
        } else if(event.dwDebugEventCode==CREATE_THREAD_DEBUG_EVENT)threads[event.dwThreadId]=event.u.CreateThread.hThread;
        else if(event.dwDebugEventCode==LOAD_DLL_DEBUG_EVENT){
            auto& d=event.u.LoadDll;const auto path=file_path(d.hFile);
            const auto loaded=SymLoadModuleExW(child.hProcess,d.hFile,path.c_str(),nullptr,reinterpret_cast<DWORD64>(d.lpBaseOfDll),0,nullptr,0);
            const auto error=GetLastError();
            if(!loaded&&error!=ERROR_SUCCESS)out<<"DLL_SYMBOL_LOAD_FAILED="<<narrow(path)<<" ERROR="<<error<<"\n";
            if(d.hFile)CloseHandle(d.hFile);
        } else if(event.dwDebugEventCode==EXCEPTION_DEBUG_EVENT){
            auto& e=event.u.Exception;const auto code=e.ExceptionRecord.ExceptionCode;
            if(code!=EXCEPTION_BREAKPOINT)status=DBG_EXCEPTION_NOT_HANDLED;
            if(!e.dwFirstChance||code==EXCEPTION_ACCESS_VIOLATION){
                out<<"EXCEPTION=0x"<<std::hex<<code<<" FIRST_CHANCE="<<e.dwFirstChance<<" THREAD="<<std::dec<<event.dwThreadId<<"\n";
                address(out,child.hProcess,reinterpret_cast<DWORD64>(e.ExceptionRecord.ExceptionAddress));
                if(code==EXCEPTION_ACCESS_VIOLATION)out<<"ACCESS_KIND="<<e.ExceptionRecord.ExceptionInformation[0]<<" BAD_ADDRESS=0x"<<std::hex<<e.ExceptionRecord.ExceptionInformation[1]<<"\n";
                CONTEXT context{};context.ContextFlags=CONTEXT_FULL;
                if(threads.contains(event.dwThreadId)&&GetThreadContext(threads[event.dwThreadId],&context)){
                    out<<std::hex<<"RAX="<<context.Rax<<" RBX="<<context.Rbx<<" RCX="<<context.Rcx<<" RDX="<<context.Rdx<<" RSP="<<context.Rsp<<" RBP="<<context.Rbp<<" RSI="<<context.Rsi<<" RDI="<<context.Rdi<<"\n";
                    out<<"R8="<<context.R8<<" R9="<<context.R9<<" R10="<<context.R10<<" R11="<<context.R11<<" R12="<<context.R12<<" R13="<<context.R13<<" R14="<<context.R14<<" R15="<<context.R15<<"\n";
                    DWORD64 raw[128]{};SIZE_T count{};
                    if(ReadProcessMemory(child.hProcess,reinterpret_cast<const void*>(context.Rsp),raw,sizeof(raw),&count)){
                        out<<"RAW_STACK_BEGIN (candidates, not unwound frames)\n";
                        for(unsigned n=0;n<count/sizeof(DWORD64);++n)if(raw[n]>0x10000&&raw[n]<0x800000000000){
                            out<<"SP+0x"<<std::hex<<n*8<<" ";address(out,child.hProcess,raw[n]);
                        }
                        out<<"RAW_STACK_END\n";
                    }
                    STACKFRAME64 frame{};frame.AddrPC={context.Rip,0,AddrModeFlat};frame.AddrStack={context.Rsp,0,AddrModeFlat};frame.AddrFrame={context.Rbp,0,AddrModeFlat};
                    out<<"DBGHELP_UNWIND_BEGIN (best effort; validate symbols independently)\n";
                    for(unsigned n=0;n<48&&frame.AddrPC.Offset;++n){
                        MEMORY_BASIC_INFORMATION region{};
                        if(!VirtualQueryEx(child.hProcess,reinterpret_cast<const void*>(frame.AddrPC.Offset),&region,sizeof(region))||
                           !(region.Protect&(PAGE_EXECUTE|PAGE_EXECUTE_READ|PAGE_EXECUTE_READWRITE|PAGE_EXECUTE_WRITECOPY)))break;
                        address(out,child.hProcess,frame.AddrPC.Offset);
                        if(!StackWalk64(IMAGE_FILE_MACHINE_AMD64,child.hProcess,threads[event.dwThreadId],&frame,&context,nullptr,SymFunctionTableAccess64,SymGetModuleBase64,nullptr))break;
                    }
                    out<<"DBGHELP_UNWIND_END\n";
                }
                out.flush();
            }
        } else if(event.dwDebugEventCode==EXIT_THREAD_DEBUG_EVENT){
            auto it=threads.find(event.dwThreadId);if(it!=threads.end()){CloseHandle(it->second);threads.erase(it);}
        } else if(event.dwDebugEventCode==EXIT_PROCESS_DEBUG_EVENT){result=event.u.ExitProcess.dwExitCode;out<<"EXIT_CODE=0x"<<std::hex<<result<<"\n";done=true;}
        ContinueDebugEvent(event.dwProcessId,event.dwThreadId,status);
    }
    SymCleanup(child.hProcess);for(auto [_,handle]:threads)CloseHandle(handle);
    CloseHandle(child.hThread);CloseHandle(child.hProcess);
    std::cout<<"Offline debugger child finished: 0x"<<std::hex<<result<<"\n";return result?1:0;
}
