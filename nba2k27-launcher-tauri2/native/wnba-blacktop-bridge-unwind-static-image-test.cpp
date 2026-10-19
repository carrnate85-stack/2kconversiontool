// Offline own-EXE static .pdata + nearby private RX bridge validation.
// No game handles, game code, native game calls, or bridge execution.
#include <windows.h>
#include <array>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <exception>
#include <stdexcept>
#include "wnba_blacktop_bridge_unwind.hpp"

extern "C" void unwind_stock_reference();
// This reference is NEVER executed. The assembler emits real static PE
// .pdata/.xdata for the exact stock push RSI/RDI/RBX; sub RSP,20h frame.
__asm__(
    ".text\n"
    ".globl unwind_stock_reference\n"
    ".def unwind_stock_reference; .scl 2; .type 32; .endef\n"
    ".seh_proc unwind_stock_reference\n"
    "unwind_stock_reference:\n"
    "pushq %rsi\n.seh_pushreg %rsi\n"
    "pushq %rdi\n.seh_pushreg %rdi\n"
    "pushq %rbx\n.seh_pushreg %rbx\n"
    "subq $32,%rsp\n.seh_stackalloc 32\n.seh_endprologue\n"
    ".rept 64\nnop\n.endr\n"
    "addq $32,%rsp\npopq %rbx\npopq %rdi\npopq %rsi\nretq\n"
    ".seh_endproc\n"
);
static void require(bool value,const char* message) {if(!value)throw std::runtime_error(message);}
struct Allocation {
    unsigned char* bytes{};
    RUNTIME_FUNCTION* runtime{};
    bool registered{};
    ~Allocation() {
        if(registered && !RtlDeleteFunctionTable(runtime))std::terminate();
        if(bytes)VirtualFree(bytes,0,MEM_RELEASE);
    }
};
struct Sample {
    alignas(16) std::array<std::uint64_t,64> stack{};
    CONTEXT context{};
    static constexpr std::uint64_t bx=0x1234567812345678,di=0x2345678923456789,si=0x3456789A3456789A;
    static constexpr std::uint64_t return_pc=0x0000012345678000;
    explicit Sample(DWORD64 pc) {
        stack.fill(0xBADBADBADBADBADB);
        stack[20]=bx;stack[21]=di;stack[22]=si;stack[23]=return_pc;
        context.ContextFlags=CONTEXT_ALL;context.Rip=pc;context.Rsp=reinterpret_cast<DWORD64>(stack.data()+16);
        context.Rbx=0xBAD;context.Rdi=0xBAD;context.Rsi=0x20000000;
        context.Rbp=0x111;context.R12=0x222;context.R13=0x333;context.R14=0x444;context.R15=0x555;
    }
    bool restored()const {
        return context.Rip==return_pc && context.Rsp==reinterpret_cast<DWORD64>(stack.data()+24) &&
            context.Rbx==bx && context.Rdi==di && context.Rsi==si && context.Rbp==0x111 &&
            context.R12==0x222 && context.R13==0x333 && context.R14==0x444 && context.R15==0x555;
    }
};
static bool unwind(DWORD64 pc,DWORD64 expected_base,RUNTIME_FUNCTION* expected,bool slots) {
    DWORD64 base{};auto* function=RtlLookupFunctionEntry(pc,&base,nullptr);
    require(base==expected_base && function==expected,"RtlLookupFunctionEntry base/entry differs");
    Sample sample(pc);PVOID handler{};DWORD64 frame{};KNONVOLATILE_CONTEXT_POINTERS pointers{};
    require(RtlVirtualUnwind(0,base,pc,function,&sample.context,&handler,&frame,&pointers)==nullptr,"unexpected language handler");
    const bool valid=sample.restored();
    if(valid && slots)require(frame==reinterpret_cast<DWORD64>(sample.stack.data()+16) &&
        pointers.IntegerContext[3]==&sample.stack[20] && pointers.IntegerContext[7]==&sample.stack[21] &&
        pointers.IntegerContext[6]==&sample.stack[22],"unwind frame/register slots differ");
    return valid;
}
int main() {
    try {
        const auto base=reinterpret_cast<DWORD64>(GetModuleHandleW(nullptr));
        const auto continuation=reinterpret_cast<DWORD64>(&unwind_stock_reference)+16;
        DWORD64 original_base{};auto* original=RtlLookupFunctionEntry(continuation,&original_base,nullptr);
        require(original!=nullptr && original_base==base,"static own-EXE primary lookup failed");
        require(std::memcmp(reinterpret_cast<const void*>(base+original->UnwindData),
            wnba_blacktop_bridge_unwind::kStockFrame.data(),12)==0,"assembler static prologue metadata does not match stock frame");
        require(unwind(continuation,base,original,true),"static primary body unwind failed");
        Allocation allocation;
        for(std::uintptr_t delta=0x40000000;delta<0x50000000;delta+=0x10000) {
            allocation.bytes=static_cast<unsigned char*>(VirtualAlloc(reinterpret_cast<void*>(base+delta),8192,MEM_RESERVE|MEM_COMMIT,PAGE_READWRITE));
            if(allocation.bytes)break;
        }
        require(allocation.bytes!=nullptr,"no nearby private allocation in production RVA window");
        const auto bridge=reinterpret_cast<DWORD64>(allocation.bytes);
        require(bridge>base && bridge-base>=0x40000000 && bridge-base<0x50000000,"allocation left production window");
        const auto put=[&](unsigned at) {
            unsigned char bytes[]{0xBE,0,0,0,0x20,0xE9,0,0,0,0};
            const auto distance=static_cast<std::int64_t>(continuation)-static_cast<std::int64_t>(bridge+at+10);
            require(distance>=INT32_MIN && distance<=INT32_MAX,"fixture JMP does not fit rel32");
            const auto rel=static_cast<std::int32_t>(distance);std::memcpy(bytes+6,&rel,4);
            std::memcpy(allocation.bytes+at,bytes,10);
        };
        put(0);put(0x100);
        std::memcpy(allocation.bytes+0x1000,wnba_blacktop_bridge_unwind::kChainInfo.data(),16);
        std::memcpy(allocation.bytes+0x1004,original,sizeof *original);
        std::memcpy(allocation.bytes+0x1020,wnba_blacktop_bridge_unwind::kZeroPrologueNegativeControl.data(),12);
        allocation.runtime=reinterpret_cast<RUNTIME_FUNCTION*>(allocation.bytes+0x1100);
        const auto rva=static_cast<DWORD>(bridge-base);
        allocation.runtime[0]={rva,rva+10,rva+0x1000};
        allocation.runtime[1]={rva+0x100,rva+0x10A,rva+0x1020};
        DWORD prior{};
        require(VirtualProtect(allocation.bytes,4096,PAGE_EXECUTE_READ,&prior)!=0 && prior==PAGE_READWRITE,"RX seal failed");
        require(VirtualProtect(allocation.bytes+4096,4096,PAGE_READONLY,&prior)!=0 && prior==PAGE_READWRITE,"metadata seal failed");
        require(FlushInstructionCache(GetCurrentProcess(),allocation.bytes,4096)!=0,"instruction-cache flush failed");
        allocation.registered=RtlAddFunctionTable(allocation.runtime,2,base)!=0;
        require(allocation.registered,"private RtlAddFunctionTable with actual static image base failed");
        DWORD64 after_base{};auto* after=RtlLookupFunctionEntry(continuation,&after_base,nullptr);
        require(after==original && after_base==base,"private registration shadowed static primary entry");
        require(unwind(continuation,base,original,true),"static primary changed after private registration");
        for(unsigned offset=0;offset<10;++offset)
            require(unwind(bridge+offset,base,&allocation.runtime[0],true),"CHAININFO private bridge byte failed with real static primary");
        require(!unwind(bridge+0x105,base,&allocation.runtime[1],false),"negative zero-prologue clone did not reject E9");
        DWORD64 outside{};
        require(RtlLookupFunctionEntry(bridge+10,&outside,nullptr)==nullptr,"private EndAddress is not exclusive");
        require(RtlLookupFunctionEntry(continuation,&after_base,nullptr)==original && after_base==base,"primary entry changed after unwind probes");
        std::printf("PASS actual own-EXE static PE .pdata primary; exact stock UNWIND_INFO; private bridge at imageBase+%lX; RtlAddFunctionTable BaseAddress=imageBase; original static continuation entry unchanged before/after; CHAININFO PC bytes10/10 restore RIP/RSP/RBX/RDI/RSI and unchanged nonvolatiles/stack-slot pointers; zero-prologue E9 negative control rejected; RX/R metadata and exclusive end verified. No game/process access or code execution.\n",static_cast<unsigned long>(rva));
        return 0;
    } catch(const std::exception& error) {std::fprintf(stderr,"FAIL: %s\n",error.what());return 1;}
}
