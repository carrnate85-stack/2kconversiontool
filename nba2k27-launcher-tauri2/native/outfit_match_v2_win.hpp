#pragma once
#ifndef _WIN32_WINNT
#define _WIN32_WINNT 0x0602
#endif
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#include <bcrypt.h>
#include <algorithm>
#include <array>
#include <cstring>
#include <limits>
#include <string>
#include <vector>
#include "outfit_match_v2_core.hpp"
#include "outfit_match_v2_profile.hpp"
namespace outfit_match_v2 {
struct Handle {
    HANDLE value{INVALID_HANDLE_VALUE};
    Handle() = default;
    explicit Handle(HANDLE v) : value(v) {}
    Handle(const Handle&) = delete;
    Handle& operator=(const Handle&) = delete;
    Handle(Handle&& o) noexcept : value(std::exchange(o.value, INVALID_HANDLE_VALUE)) {}
    Handle& operator=(Handle&& o) noexcept {
        if (this != &o) { reset(); value = std::exchange(o.value,INVALID_HANDLE_VALUE); } return *this;
    }
    ~Handle() { reset(); }
    void reset() { if (value && value != INVALID_HANDLE_VALUE) CloseHandle(value); value=INVALID_HANDLE_VALUE; }
    explicit operator bool() const { return value && value != INVALID_HANDLE_VALUE; }
};
inline std::uint64_t ticks(FILETIME f) { return (std::uint64_t(f.dwHighDateTime)<<32)|f.dwLowDateTime; }
inline bool live(HANDLE h) { return h && h != INVALID_HANDLE_VALUE && WaitForSingleObject(h,0)==WAIT_TIMEOUT; }
// GetCurrentProcess() is the -1 pseudo-handle, numerically identical to
// INVALID_HANDLE_VALUE. Retain a real query/synchronize handle for identity
// checks rather than weakening live()'s invalid-handle rejection.
inline Handle retain_current_process() {
    return Handle(OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION|SYNCHRONIZE,FALSE,GetCurrentProcessId()));
}
inline bool process_identity(HANDLE h, std::uint64_t birth, const std::wstring& path) {
    FILETIME b{},e{},k{},u{}; std::array<wchar_t,32768> p{}; DWORD size=DWORD(p.size());
    return live(h) && GetProcessTimes(h,&b,&e,&k,&u) && ticks(b)==birth &&
        QueryFullProcessImageNameW(h,0,p.data(),&size) && size==path.size() &&
        CompareStringOrdinal(p.data(),int(size),path.data(),int(path.size()),TRUE)==CSTR_EQUAL;
}
struct StableId {
    ULONGLONG volume{}; std::array<unsigned char,16> index{};
    bool operator==(const StableId&) const = default;
};
inline bool file_id(HANDLE h, bool directory, StableId& id) {
    FILE_ATTRIBUTE_TAG_INFO tag{}; FILE_STANDARD_INFO info{}; FILE_ID_INFO identity{};
    if (!GetFileInformationByHandleEx(h,FileAttributeTagInfo,&tag,sizeof tag) ||
        (tag.FileAttributes & FILE_ATTRIBUTE_REPARSE_POINT) ||
        bool(tag.FileAttributes & FILE_ATTRIBUTE_DIRECTORY)!=directory ||
        !GetFileInformationByHandleEx(h,FileStandardInfo,&info,sizeof info) || info.DeletePending ||
        !GetFileInformationByHandleEx(h,FileIdInfo,&identity,sizeof identity)) return false;
    // Ordinary file streams only. A hard-linked intent/report is not ours.
    if (!directory && (info.NumberOfLinks!=1 || info.EndOfFile.QuadPart<0)) return false;
    id.volume=identity.VolumeSerialNumber;
    std::memcpy(id.index.data(),identity.FileId.Identifier,16);
    return id.volume && std::any_of(id.index.begin(),id.index.end(),[](auto b){return b!=0;});
}
inline bool canonical_path(const std::wstring& path) {
    if (path.size()<3 || path.size()>240 || path[1]!=L':' || path[2]!=L'\\' ||
        !((path[0]>=L'A'&&path[0]<=L'Z')||(path[0]>=L'a'&&path[0]<=L'z')) ||
        path.back()==L'\\') return false;
    for (std::size_t i=3;i<path.size();++i) if (path[i]==L'/' || path[i]==L':' || path[i]<32) return false;
    std::size_t begin=3;
    for (std::size_t i=3;i<=path.size();++i) if (i==path.size() || path[i]==L'\\') {
        const auto part=path.substr(begin,i-begin);
        if (part.empty() || part==L"." || part==L".." || part.back()==L'.' || part.back()==L' ') return false;
        begin=i+1;
    }
    return true;
}
inline bool equal_path(const std::wstring& a,const std::wstring& b) {
    return a.size()==b.size() && CompareStringOrdinal(a.data(),int(a.size()),b.data(),int(b.size()),TRUE)==CSTR_EQUAL;
}
// All ancestors stay pinned without DELETE sharing, not just the leaf. No
// create-directory fallback: the trusted launcher must prepare its own layout.
class PinnedDirectory {
    std::wstring path_;
    std::vector<Handle> handles_;
    std::vector<StableId> identities_;
public:
    bool open(const std::wstring& path) {
        if (!handles_.empty() || !canonical_path(path)) return false;
        for (std::size_t n=3;n<=path.size();++n) if (n==3 || n==path.size() || path[n]==L'\\') {
            const auto name=path.substr(0,n);
            Handle h(CreateFileW(name.c_str(),FILE_LIST_DIRECTORY|FILE_READ_ATTRIBUTES,FILE_SHARE_READ|FILE_SHARE_WRITE,nullptr,
                OPEN_EXISTING,FILE_FLAG_BACKUP_SEMANTICS|FILE_FLAG_OPEN_REPARSE_POINT,nullptr));
            StableId id{};
            if (!h || !file_id(h.value,true,id)) return false;
            handles_.push_back(std::move(h)); identities_.push_back(id);
        }
        path_=path; return !handles_.empty();
    }
    bool unchanged() const {
        if (handles_.empty() || identities_.size()!=handles_.size()) return false;
        for (std::size_t n=0;n<handles_.size();++n) {
            StableId now{}; if (!file_id(handles_[n].value,true,now) || now!=identities_[n]) return false;
        }
        return true;
    }
    const std::wstring& path() const { return path_; }
};
class Hash {
    BCRYPT_ALG_HANDLE algorithm_{}; BCRYPT_HASH_HANDLE hash_{}; std::vector<unsigned char> object_;
public:
    Hash(const Hash&)=delete; Hash& operator=(const Hash&)=delete;
    Hash() {
        DWORD size{},got{};
        if (BCryptOpenAlgorithmProvider(&algorithm_,BCRYPT_SHA256_ALGORITHM,nullptr,0)<0 ||
            BCryptGetProperty(algorithm_,BCRYPT_OBJECT_LENGTH,reinterpret_cast<PUCHAR>(&size),sizeof size,&got,0)<0 ||
            got!=sizeof size || size>65536) return;
        object_.resize(size);
        if (BCryptCreateHash(algorithm_,&hash_,object_.data(),size,nullptr,0,0)<0) hash_=nullptr;
    }
    ~Hash() { if(hash_) BCryptDestroyHash(hash_); if(algorithm_) BCryptCloseAlgorithmProvider(algorithm_,0); }
    bool update(const void* p,std::size_t n) {
        return hash_ && n<=0x100000 && BCryptHashData(hash_,reinterpret_cast<PUCHAR>(const_cast<void*>(p)),ULONG(n),0)>=0;
    }
    std::string finish() {
        std::array<unsigned char,32> b{};
        if (!hash_ || BCryptFinishHash(hash_,b.data(),DWORD(b.size()),0)<0) return {};
        std::string s; for(auto v:b) { s.push_back("0123456789ABCDEF"[v>>4]);s.push_back("0123456789ABCDEF"[v&15]); } return s;
    }
};
inline bool read_file(HANDLE h,std::size_t max,std::vector<unsigned char>& bytes) {
    FILE_STANDARD_INFO st{}; LARGE_INTEGER zero{};
    if (!GetFileInformationByHandleEx(h,FileStandardInfo,&st,sizeof st) || st.EndOfFile.QuadPart<0 ||
        std::uint64_t(st.EndOfFile.QuadPart)>max || !SetFilePointerEx(h,zero,nullptr,FILE_BEGIN)) return false;
    bytes.resize(std::size_t(st.EndOfFile.QuadPart)); DWORD got{};
    return bytes.size()<=MAXDWORD && ReadFile(h,bytes.data(),DWORD(bytes.size()),&got,nullptr) && got==bytes.size();
}
inline std::string digest(const std::vector<unsigned char>& b) {
    Hash h; for(std::size_t i=0;i<b.size();i+=65536) if(!h.update(b.data()+i,std::min<std::size_t>(65536,b.size()-i))) return {};
    return h.finish();
}
inline bool hash_executable(const std::wstring& path) {
    Handle file(CreateFileW(path.c_str(),GENERIC_READ,FILE_SHARE_READ,nullptr,OPEN_EXISTING,
        FILE_FLAG_OPEN_REPARSE_POINT|FILE_FLAG_SEQUENTIAL_SCAN,nullptr));
    StableId id{}; FILE_STANDARD_INFO st{};
    if(!file || !file_id(file.value,false,id) || !GetFileInformationByHandleEx(file.value,FileStandardInfo,&st,sizeof st) ||
        st.EndOfFile.QuadPart!=870548760) return false;
    Hash h; std::array<unsigned char,65536> buffer{}; DWORD got{};
    while(true) { if(!ReadFile(file.value,buffer.data(),DWORD(buffer.size()),&got,nullptr)) return false;
        if(!got) break; if(!h.update(buffer.data(),got)) return false; }
    StableId after{};
    return file_id(file.value,false,after) && after==id &&
        h.finish()=="29A96471C55909FC2BCEA6C699C59238656A501C19B50C74ED64B0D842E576A1";
}
inline bool read_memory(std::uintptr_t at,void* out,std::size_t count) {
    if (at<0x10000 || !count || count>65536 || at>0x7FFF00000000ull-count) return false;
    SIZE_T got{};
    return ReadProcessMemory(GetCurrentProcess(),reinterpret_cast<const void*>(at),out,count,&got) && got==count;
}
template<class T> inline bool value(std::uintptr_t at,T& out) { return read_memory(at,&out,sizeof out); }
inline std::uintptr_t q(std::uintptr_t at) { std::uintptr_t v{}; return value(at,v)?v:0; }
inline unsigned d(std::uintptr_t at) { unsigned v=~0u; return value(at,v)?v:~0u; }
inline bool private_span(std::uintptr_t at,std::size_t bytes,bool write=false) {
    if(at<0x10000 || !bytes || bytes>65536 || at>0x7FFF00000000ull-bytes) return false;
    const auto end=at+bytes;
    for(unsigned regions=0;at<end&&regions<32;++regions) {
        MEMORY_BASIC_INFORMATION m{};
        if(VirtualQuery(reinterpret_cast<const void*>(at),&m,sizeof m)!=sizeof m || m.State!=MEM_COMMIT ||
            m.Type!=MEM_PRIVATE || m.Protect&(PAGE_GUARD|PAGE_NOACCESS)) return false;
        const auto p=m.Protect&0xFF;
        if(p!=PAGE_READONLY && p!=PAGE_READWRITE && p!=PAGE_WRITECOPY) return false;
        if(write && p!=PAGE_READWRITE && p!=PAGE_WRITECOPY) return false;
        const auto start=reinterpret_cast<std::uintptr_t>(m.BaseAddress);
        if(!m.RegionSize || start>at || start>std::numeric_limits<std::uintptr_t>::max()-m.RegionSize || start+m.RegionSize<=at) return false;
        at=std::min(end,start+m.RegionSize);
    }
    return at==end;
}
inline bool profile(std::uintptr_t base) {
    std::array<unsigned char,4096> header{};
    if(!read_memory(base,header.data(),header.size()) || header[0]!='M' || header[1]!='Z') return false;
    unsigned pe{};std::memcpy(&pe,header.data()+60,4);
    if(pe<64 || pe>3500 || std::memcmp(header.data()+pe,"PE\0\0",4) ||
        *reinterpret_cast<const unsigned short*>(header.data()+pe+4)!=0x8664 ||
        *reinterpret_cast<const unsigned short*>(header.data()+pe+24)!=0x20B ||
        *reinterpret_cast<const unsigned*>(header.data()+pe+8)!=0x6AC53765 ||
        *reinterpret_cast<const unsigned*>(header.data()+pe+80)!=0x35BDE000) return false;
    for(const auto& proof:kMethods) {
        std::vector<unsigned char> b(proof.bytes);
        if(!read_memory(base+proof.rva,b.data(),b.size()) || digest(b)!=proof.sha256) return false;
    }
    for(const auto& proof:kExtraMethods) {
        std::vector<unsigned char> b(proof.bytes);
        if(!read_memory(base+proof.rva,b.data(),b.size()) || digest(b)!=proof.sha256) return false;
    }
    for(const auto& proof:kVirtuals) if(q(base+proof.table+proof.slot)!=base+proof.method) return false;
    if(q(base+kSecondary+0xA0)!=base+0x6FD8C0) return false;
    struct Anchor {std::uintptr_t at,target;std::array<unsigned char,3> prefix;};
    constexpr Anchor anchors[]={{0x3457D80,kRootTag,{0x48,0x8D,0x05}},
        {0x3457D87,kRoot,{0x48,0x89,0x05}},{0x25AA3D0,kOwner,{0x48,0x8B,0x05}},
        {0x308C649,kModes,{0x48,0x8D,0x05}},{0x308C650,kAlternate,{0x48,0x63,0x0D}},
        {0x300D703,kCurrent,{0x48,0x63,0x05}}};
    for(const auto& a:anchors) {std::array<unsigned char,7> b{};std::int32_t disp{};
        if(!read_memory(base+a.at,b.data(),b.size()) || !std::equal(a.prefix.begin(),a.prefix.end(),b.begin())) return false;
        std::memcpy(&disp,b.data()+3,4); if(std::int64_t(a.at)+7+disp!=std::int64_t(a.target)) return false; }
    for(const auto& [at,target]:{std::pair<std::uintptr_t,std::uintptr_t>{0x32F799A,kLineup+4},{0x32F79A0,kLineup+8}}) {
        std::array<unsigned char,6> code{};std::int32_t displacement{};
        if(!read_memory(base+at,code.data(),code.size()) || code[0]!=0x89 || code[1]!=0x05) return false;
        std::memcpy(&displacement,code.data()+2,4);
        if(std::int64_t(at)+6+displacement!=std::int64_t(target)) return false;
    }
    const unsigned char stage[]={0x8B,0x81,0xA8,0x0E,0,0};
    const unsigned char active[]={0x80,0x3D,0xC4,0x58,0x66,0x08,0};
    const unsigned char mode[]={0x48,0x8B,0x05,0xC9,0x79,0x66,0x08,0x8B,0x80,0x04,0x0A,0,0,0xC3};
    std::array<unsigned char,14> b{};
    return read_memory(base+0x32FB9B6,b.data(),sizeof stage) && !std::memcmp(b.data(),stage,sizeof stage) &&
        read_memory(base+0x24F7EC5,b.data(),sizeof active) && !std::memcmp(b.data(),active,sizeof active) &&
        read_memory(base+0x24F5450,b.data(),sizeof mode) && !std::memcmp(b.data(),mode,sizeof mode) &&
        q(base+kRootTag)==base+0x23120 && q(base+kRootTag+8)==base+0x346B350;
}
struct TrustedBinding {
    DWORD game_pid{},owner_pid{};
    std::uint64_t game_birth{},owner_birth{};
    std::wstring game_executable,owner_executable,portable_root;
    std::string session;
    HANDLE cancellation{}; // Borrowed internal event; duplicated at open.
    // Mandatory provider-owned callbacks, never browser-controlled addresses.
    bool (*session_owned)(void*) noexcept{};
    bool (*protected_present)(void*) noexcept{};
    void* context{};
};
class Authority {
    TrustedBinding binding_;
    Handle owner_,cancel_,game_;
    std::uintptr_t base_{};
    std::string reason_="Invalid native authority binding";
public:
    const std::string& reason() const { return reason_; }
    bool open(const TrustedBinding& b) {
        if(owner_ || b.game_pid!=GetCurrentProcessId() || !b.game_birth || !b.owner_pid || b.owner_pid==b.game_pid ||
            !b.owner_birth || !b.session_owned || !b.protected_present || !b.cancellation ||
            !canonical_path(b.game_executable) || !canonical_path(b.owner_executable) || !canonical_path(b.portable_root) ||
            b.session.size()<16 || b.session.size()>96) return false;
        for(char c:b.session) if(!((c>='a'&&c<='z')||(c>='A'&&c<='Z')||(c>='0'&&c<='9')||c=='-')) return false;
        const auto expected=b.portable_root+L"\\App\\";
        if(b.owner_executable.size()<=expected.size() || !equal_path(b.owner_executable.substr(0,expected.size()),expected)) return false;
        Handle owner(OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION|SYNCHRONIZE,FALSE,b.owner_pid));
        HANDLE cancel{};
        if(!owner || !process_identity(owner.value,b.owner_birth,b.owner_executable) ||
            !DuplicateHandle(GetCurrentProcess(),b.cancellation,GetCurrentProcess(),&cancel,SYNCHRONIZE,FALSE,0)) return false;
        Handle held(cancel);
        auto game=retain_current_process();
        if(!game || !process_identity(game.value,b.game_birth,b.game_executable)) return false;
        binding_=b;owner_=std::move(owner);cancel_=std::move(held);game_=std::move(game);
        base_=reinterpret_cast<std::uintptr_t>(GetModuleHandleW(nullptr));
        if(!safe()){reason_="Native lease/process/permission guard rejected before ARMED";return false;}
        if(!hash_executable(b.game_executable)){reason_="Native executable file identity/hash rejected before ARMED";return false;}
        if(!profile(base_)){reason_="Native in-memory Version2 method/table proof rejected before ARMED";return false;}
        reason_.clear();return true;
    }
    bool safe() const {
        return owner_ && cancel_ && WaitForSingleObject(cancel_.value,0)==WAIT_TIMEOUT &&
            process_identity(owner_.value,binding_.owner_birth,binding_.owner_executable) &&
            process_identity(game_.value,binding_.game_birth,binding_.game_executable) &&
            binding_.session_owned(binding_.context) && !binding_.protected_present(binding_.context);
    }
    std::uintptr_t base() const { return base_; }
    const TrustedBinding& binding() const { return binding_; }
};
} // namespace outfit_match_v2
