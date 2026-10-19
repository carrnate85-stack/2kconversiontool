// Offline self-process fixture. No game handles, game calls, or code execution.
// RtlVirtualUnwind interprets a synthetic CONTEXT/stack and newly allocated
// inert bridge bytes. Dynamic function registration is removed only after all
// fixture lookups/unwinds finish. Production must retain it until game exit.
#include <windows.h>
#include <array>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <exception>
#include <stdexcept>
#include "wnba_blacktop_bridge_unwind.hpp"

static void require(bool condition,const char* message) {
    if(!condition)throw std::runtime_error(message);
}
struct Fixture {
    static constexpr std::uint32_t base_delta=0x40000000;
    unsigned char* memory{};
    RUNTIME_FUNCTION* table{};
    DWORD64 image_base{};
    bool registered{};
    bool primary_registered{};
    Fixture() {
        memory=static_cast<unsigned char*>(VirtualAlloc(nullptr,8192,MEM_RESERVE|MEM_COMMIT,PAGE_READWRITE));
        require(memory!=nullptr,"fixture VirtualAlloc failed");
        require(reinterpret_cast<DWORD64>(memory)>base_delta+0x10000ULL,"fixture too low for synthetic game base");
        image_base=reinterpret_cast<DWORD64>(memory)-base_delta;
        std::memset(memory,0x90,4096);
        const unsigned char prologue[]{0x56,0x57,0x53,0x48,0x83,0xEC,0x20};
        std::memcpy(memory+0x100,prologue,sizeof prologue);
        const auto bridge=[this](std::uint32_t at) {
            unsigned char code[]{0xBE,0,0,0,0x20,0xE9,0,0,0,0};
            const std::int32_t delta=0x15B-static_cast<std::int32_t>(at+10);
            std::memcpy(code+6,&delta,4);std::memcpy(memory+at,code,sizeof code);
        };
        bridge(0x300);bridge(0x400);bridge(0x500);bridge(0x600);
        std::memcpy(memory+0x1000,wnba_blacktop_bridge_unwind::kStockFrame.data(),12);
        std::memcpy(memory+0x1020,wnba_blacktop_bridge_unwind::kZeroPrologueNegativeControl.data(),12);
        std::memcpy(memory+0x1040,wnba_blacktop_bridge_unwind::kChainInfo.data(),16);
        const RUNTIME_FUNCTION primary{base_delta+0x100,base_delta+0x1FB,base_delta+0x1000};
        std::memcpy(memory+0x1044,&primary,sizeof primary);
        auto guarded=wnba_blacktop_bridge_unwind::kZeroPrologueNegativeControl;guarded[1]=10;
        std::memcpy(memory+0x1060,guarded.data(),guarded.size());
        table=reinterpret_cast<RUNTIME_FUNCTION*>(memory+0x1100);
        table[0]={base_delta+0x100,base_delta+0x1FB,base_delta+0x1000};
        table[1]={base_delta+0x300,base_delta+0x30A,base_delta+0x1020};
        table[2]={base_delta+0x400,base_delta+0x40A,base_delta+0x1000}; // Negative control: wrong original prologue offsets.
        table[3]={base_delta+0x500,base_delta+0x50A,base_delta+0x1040}; // Chained noncontiguous segment.
        table[4]={base_delta+0x600,base_delta+0x60A,base_delta+0x1060}; // Full bridge treated as established prologue.
        DWORD old{};
        require(VirtualProtect(memory,4096,PAGE_EXECUTE_READ,&old)!=0 && old==PAGE_READWRITE,"fixture RX transition failed");
        require(VirtualProtect(memory+4096,4096,PAGE_READONLY,&old)!=0 && old==PAGE_READWRITE,"fixture metadata R transition failed");
        require(FlushInstructionCache(GetCurrentProcess(),memory,4096)!=0,"fixture instruction flush failed");
        // The real game already registers the primary through its static PE
        // exception directory. Model that with a SEPARATE primary table here;
        // do not smuggle the primary into the private fragment's table.
        primary_registered=RtlAddFunctionTable(table,1,image_base)!=0;
        require(primary_registered,"primary reference function table registration failed");
        registered=RtlAddFunctionTable(table+1,4,image_base)!=0;
        require(registered,"RtlAddFunctionTable failed");
    }
    ~Fixture() {
        if(registered && !RtlDeleteFunctionTable(table+1))std::terminate();
        if(primary_registered && !RtlDeleteFunctionTable(table))std::terminate();
        if(memory)VirtualFree(memory,0,MEM_RELEASE);
    }
};
struct Sample {
    CONTEXT context{};
    alignas(16) std::array<std::uint64_t,64> stack{};
    static constexpr std::uint64_t saved_bx=0x1111222233334444ULL;
    static constexpr std::uint64_t saved_di=0x5555666677778888ULL;
    static constexpr std::uint64_t saved_si=0x9999AAAABBBBCCCCULL;
    static constexpr std::uint64_t return_pc=0x0000012345678000ULL;
    explicit Sample(std::uint64_t control_pc) {
        stack.fill(0xABCDABCDABCDABCDULL);
        // bottom+0..31: stock shadow space; +32 RBX; +40 RDI; +48 RSI;
        // +56 original return PC. The post-prologue RSP is16byte aligned.
        stack[20]=saved_bx;stack[21]=saved_di;stack[22]=saved_si;stack[23]=return_pc;
        context.ContextFlags=CONTEXT_ALL;context.Rip=control_pc;
        context.Rsp=reinterpret_cast<DWORD64>(stack.data()+16);
        context.Rbx=0xBAD;context.Rdi=0xBAD;context.Rsi=0x20000000;
        context.Rbp=0x1234;context.R12=0x2345;context.R13=0x3456;context.R14=0x4567;context.R15=0x5678;
        require((context.Rsp&15)==0,"synthetic stack alignment wrong");
    }
    bool restored() const {
        return context.Rip==return_pc && context.Rsp==reinterpret_cast<DWORD64>(stack.data()+24) &&
            context.Rbx==saved_bx && context.Rdi==saved_di && context.Rsi==saved_si &&
            context.Rbp==0x1234 && context.R12==0x2345 && context.R13==0x3456 && context.R14==0x4567 && context.R15==0x5678;
    }
};
static bool unwind(Fixture& fixture,std::uint32_t rva,bool check_pointers) {
    const auto pc=reinterpret_cast<DWORD64>(fixture.memory)+rva;
    DWORD64 image{};
    auto* entry=RtlLookupFunctionEntry(pc,&image,nullptr);
    require(entry!=nullptr && image==fixture.image_base,"dynamic lookup did not find retained exact fixture entry/base");
    Sample sample(pc);
    PVOID handler{};DWORD64 frame{};KNONVOLATILE_CONTEXT_POINTERS pointers{};
    const auto function=RtlVirtualUnwind(UNW_FLAG_NHANDLER,image,pc,entry,&sample.context,&handler,&frame,&pointers);
    require(function==nullptr,"unexpected handler for flags0");
    const bool valid=sample.restored();
    if(!valid)std::printf("unwind mismatch rva=%X RIP=%llX RSPdelta=%lld RBX=%llX RDI=%llX RSI=%llX\n",rva,
        static_cast<unsigned long long>(sample.context.Rip),
        static_cast<long long>(sample.context.Rsp-reinterpret_cast<DWORD64>(sample.stack.data()+16)),
        static_cast<unsigned long long>(sample.context.Rbx),static_cast<unsigned long long>(sample.context.Rdi),static_cast<unsigned long long>(sample.context.Rsi));
    if(valid && check_pointers) {
        require(frame==reinterpret_cast<DWORD64>(sample.stack.data()+16),"establisher frame differs");
        require(pointers.IntegerContext[3]==&sample.stack[20] && pointers.IntegerContext[7]==&sample.stack[21] && pointers.IntegerContext[6]==&sample.stack[22],"nonvolatile register slot locations differ");
    }
    return valid;
}
int main() {
    try {
        Fixture fixture;
        require(unwind(fixture,0x150,true),"original stock-frame body reference failed");
        unsigned correct{},chained{},guarded{};
        for(unsigned offset=0;offset<10;++offset) {
            if(unwind(fixture,0x300+offset,true))++correct;
            if(unwind(fixture,0x500+offset,true))++chained;
            if(unwind(fixture,0x600+offset,true))++guarded;
        }
        std::printf("probe zero=%u/10 chained=%u/10 guarded-prologue=%u/10\n",correct,chained,guarded);
        require(chained==10,"CHAININFO metadata did not restore all bridge PCs");
        require(correct==9 && guarded==9,"negative clone controls did not expose the E9 epilogue failure");
        require(RtlDeleteFunctionTable(fixture.table)!=0,"could not remove primary reference for negative control");
        fixture.primary_registered=false;
        require(!unwind(fixture,0x505,false),"missing primary lookup did not expose the E9 failure");
        fixture.primary_registered=RtlAddFunctionTable(fixture.table,1,fixture.image_base)!=0;
        require(fixture.primary_registered,"could not restore retained primary reference registration");
        for(unsigned offset=0;offset<10;++offset)
            require(unwind(fixture,0x500+offset,true),"chain failed after restoring exact primary lookup");
        unsigned rejected{};
        for(unsigned offset=0;offset<7;++offset) {
            if(!unwind(fixture,0x400+offset,false))++rejected;
        }
        require(rejected>0,"negative original-offset metadata unexpectedly restored every early PC");
        DWORD64 image{};
        require(RtlLookupFunctionEntry(reinterpret_cast<DWORD64>(fixture.memory)+0x30A,&image,nullptr)==nullptr,"EndAddress is not exclusive");
        std::printf("PASS original stock-frame body; CHAININFO bridge PCs=%u/10 and10/10 after primary re-registration; zero-offset clone=%u/10 and guarded-prologue clone=%u/10 reject E9; missing-primary lookup rejected E9; wrong original-offset PCs rejected=%u/7; dynamic lookup with virtualGameBase=allocation-40000000 and primary registered through SEPARATE reference table, exclusive ranges, RIP/RSP/RBX/RDI/RSI, unchanged nonvolatiles, stack-slot pointers verified. No game/process access or bridge execution.\n",chained,correct,guarded,rejected);
        return 0;
    } catch(const std::exception& error) {
        std::fprintf(stderr,"FAIL: %s\n",error.what());return 1;
    }
}
