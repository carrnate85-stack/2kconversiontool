#pragma once
// Dormant in-process native binding. No worker, config reader, export, or
// automatic activation is provided. Live invocation is an explicit test gate.
#include "outfit_match_v2_native.hpp"
#include "wnba_priority_v2_core.hpp"
#include "wnba_priority_v2_profile.hpp"
namespace wnba_priority_v2 {
class NativeHost {
    outfit_match_v2::Authority* authority_{};
    outfit_match_v2::PinnedDirectory directory_;
    outfit_match_v2::Handle index_, payload_;
    outfit_match_v2::StableId index_id_, payload_id_;
    std::uintptr_t base_{}, context_{}, manager_{}, mutex_{};
    std::string directory_ascii_;
    std::vector<unsigned char> verified_index_;
    mutable std::uintptr_t published_{};
    bool locked_{}, approved_{};
    std::uint32_t expected_entries_{1};
    using Sync = bool (*)(void*);
    using Register = std::uint32_t* (*)(void*,std::uint32_t*,const char*,const char*);
    bool chain() const {
        using namespace outfit_match_v2;
        return authority_ && authority_->safe() && context_ && manager_ && mutex_ &&
            q(base_+wnba_priority_v2::kWrapper+0x38)==context_ &&
            private_span(context_,0x108) && q(context_+0x100)==manager_ &&
            private_span(manager_,0x150,true) && q(manager_)==base_+kManagerTable &&
            q(manager_+0x70)==mutex_ && q(manager_+0xE0)!=0 &&
            private_span(mutex_,0x40,true) && read_mutex_enabled();
    }
    bool read_mutex_enabled() const {
        unsigned char enabled{};
        return outfit_match_v2::value(mutex_+0x3A,enabled) && enabled==1;
    }
    bool menu() const {
        outfit_match_v2::Context first{},second{};
        return outfit_match_v2::context(base_,first) &&
            inactive_frontend({first.mode,first.other,first.stage,first.active,first.kind}) &&
            outfit_match_v2::context(base_,second) && first==second;
    }
    bool pins() const {
        for(const auto& p:wnba_priority_v2::kMethods) {
            std::vector<unsigned char> bytes(p.bytes);
            if(!outfit_match_v2::read_memory(base_+p.rva,bytes.data(),bytes.size()) ||
                outfit_match_v2::digest(bytes)!=p.sha256) return false;
            MEMORY_BASIC_INFORMATION m{};
            if(VirtualQuery(reinterpret_cast<const void*>(base_+p.rva),&m,sizeof m)!=sizeof m ||
                m.Type!=MEM_IMAGE || reinterpret_cast<std::uintptr_t>(m.AllocationBase)!=base_ ||
                reinterpret_cast<std::uintptr_t>(m.BaseAddress)>base_+p.rva ||
                m.RegionSize < p.bytes || base_+p.rva-reinterpret_cast<std::uintptr_t>(m.BaseAddress)>m.RegionSize-p.bytes ||
                m.Protect&(PAGE_GUARD|PAGE_NOACCESS) || (m.Protect&0xFF)!=PAGE_EXECUTE_READ) return false;
        }
        return true;
    }
    struct CategorySource {
        std::array<std::uintptr_t,5> pointers{};
        std::array<unsigned char,48> raw{},header{},descriptor{};
        std::vector<unsigned char> prefix;
        bool operator==(const CategorySource&) const = default;
    };
    bool category_snapshot(CategorySource& out) const {
        using namespace outfit_match_v2;
        if(!published_ || verified_index_.size()!=144 || directory_ascii_.empty() ||
            directory_ascii_.size()>=4096 || !chain() || q(manager_+0x48)!=published_ ||
            !private_span(published_,0x40,true) || q(published_)!=base_+kWadTable ||
            q(published_+8)!=manager_ || d(published_+0x30)!=2 || d(published_+0x34)!=1)return false;
        out.pointers={published_,q(published_+0x10),q(published_+0x18),q(published_+0x20),q(published_+0x28)};
        for(unsigned i=1;i<4;++i)if(!private_span(out.pointers[i],48))return false;
        out.prefix.resize(directory_ascii_.size()+1);
        if(!private_span(out.pointers[4],out.prefix.size()) ||
            !read_memory(out.pointers[1],out.raw.data(),out.raw.size()) ||
            !read_memory(out.pointers[2],out.header.data(),out.header.size()) ||
            !read_memory(out.pointers[3],out.descriptor.data(),out.descriptor.size()) ||
            !read_memory(out.pointers[4],out.prefix.data(),out.prefix.size()))return false;
        for(unsigned i=0;i<48;++i)if(out.raw[i]!=verified_index_[96+i] || out.header[i]!=verified_index_[i])return false;
        for(unsigned i=0;i<8;++i)if(out.descriptor[i]!=verified_index_[48+i])return false;
        // Native constructor replaces descriptor handle at +8 and tag at +0x2c.
        for(unsigned i=0x10;i<0x2c;++i)if(out.descriptor[i]!=verified_index_[48+i])return false;
        for(unsigned i=8;i<0x10;++i)out.descriptor[i]=0;
        for(unsigned i=0x2c;i<0x30;++i)out.descriptor[i]=0;
        for(std::size_t i=0;i<directory_ascii_.size();++i)
            if(out.prefix[i]!=static_cast<unsigned char>(directory_ascii_[i]))return false;
        return out.prefix.back()==0 && q(manager_+0x48)==published_ && chain();
    }
public:
    NativeHost() = default;
    NativeHost(const NativeHost&)=delete;
    NativeHost& operator=(const NativeHost&)=delete;
    ~NativeHost() { if(locked_) unlock(); } // Never unregister the native WAD.

    // Fixed private-pack format and exact prepared WPF-era payload. Neither
    // filename nor game-call address is configurable. A future worker must pin
    // this host for the entire game lifetime after publish, even on pipe loss.
    bool open(outfit_match_v2::Authority& authority,const std::wstring& directory,
              bool live_call_approved=false,bool blacktop_source_trial=false) {
        using namespace outfit_match_v2;
        if(authority_ || !live_call_approved || !authority.safe()) return false;
        const auto& profile=pack_profile(blacktop_source_trial);
        const auto prefix=authority.binding().portable_root+L"\\Data\\wnba-priority-v2\\";
        if(!equal_path(directory,prefix+profile.folder) || !canonical_path(directory)) return false;
        for(auto c:directory) {
            if(c<32 || c>126) return false; // Native WAD API consumes narrow paths.
            directory_ascii_.push_back(char(c));
        }
        // Native formatter at RVA 0x6886C34 is UTF16 "{0}{1}"; it does
        // not insert a separator before index name or descriptor filename.
        directory_ascii_.push_back('/');
        if(!directory_.open(directory)) return false;
        index_=Handle(CreateFileW((directory+L"\\0A").c_str(),GENERIC_READ,FILE_SHARE_READ,nullptr,
            OPEN_EXISTING,FILE_FLAG_OPEN_REPARSE_POINT,nullptr));
        payload_=Handle(CreateFileW((directory+L"\\0B").c_str(),GENERIC_READ,FILE_SHARE_READ,nullptr,
            OPEN_EXISTING,FILE_FLAG_OPEN_REPARSE_POINT,nullptr));
        std::vector<unsigned char> index,payload;
        if(!index_ || !payload_ || !file_id(index_.value,false,index_id_) ||
            !file_id(payload_.value,false,payload_id_) ||
            !read_file(index_.value,profile.index_bytes,index) || index.size()!=profile.index_bytes ||
            !read_file(payload_.value,1762144,payload) || payload.size()!=1762144 ||
            digest(index)!=profile.index_sha256 ||
            digest(payload)!="D42B4E4DDFA03A83DF799A49A9BAEF1526A74393F4CFC44A5CE01AA2BF7CE7EE") return false;
        expected_entries_=profile.entries;
        verified_index_=std::move(index);
        authority_=&authority; base_=authority.base();
        context_=q(base_+wnba_priority_v2::kWrapper+0x38);
        if(!private_span(context_,0x108)) return false;
        manager_=q(context_+0x100);
        if(!private_span(manager_,0x150,true)) return false;
        mutex_=q(manager_+0x70);
        approved_=chain() && menu() && pins();
        return approved_;
    }
    bool lock() {
        if(!approved_ || locked_ || !chain() || !menu() || !pins()) return false;
        if(!reinterpret_cast<Sync>(base_+kLock)(reinterpret_cast<void*>(mutex_))) return false;
        locked_=true; return true;
    }
    void unlock() {
        if(!locked_) return;
        reinterpret_cast<Sync>(base_+kUnlock)(reinterpret_cast<void*>(mutex_));
        locked_=false;
    }
    bool checkpoint(std::uint64_t identity) const {
        using namespace outfit_match_v2;
        StableId index,payload;
        return approved_ && locked_ && identity==authority_->binding().game_birth &&
            chain() && menu() && directory_.unchanged() &&
            file_id(index_.value,false,index) && index==index_id_ &&
            file_id(payload_.value,false,payload) && payload==payload_id_;
    }
    std::uintptr_t priority() const {
        return locked_ ? outfit_match_v2::q(manager_+0x48) : 0;
    }
    bool native_register() {
        if(!approved_ || !authority_ || !locked_) return false;
        if(!checkpoint(authority_->binding().game_birth) || priority()) return false;
        std::uint32_t status{};
        auto* returned=reinterpret_cast<Register>(base_+kRegister)(reinterpret_cast<void*>(manager_),
            &status,"0A",directory_ascii_.c_str());
        return returned==&status && status==0x504521A8;
    }
    bool is_native_wad(std::uintptr_t p) const {
        const bool valid=locked_ && chain() && outfit_match_v2::q(manager_+0x48)==p &&
            outfit_match_v2::private_span(p,0x40,true) &&
            outfit_match_v2::q(p)==base_+kWadTable && outfit_match_v2::q(p+8)==manager_ &&
            outfit_match_v2::d(p+0x30)==expected_entries_ && outfit_match_v2::d(p+0x34)==1;
        if(!valid || (published_ && published_!=p))return false;
        if(!published_)published_=p; // First locked post-publication proof only; never adopt another WAD.
        return true;
    }
    // Read-only category admission proof. Deliberately independent of Home;
    // it performs no native calls, lock acquisition, or game-memory writes.
    // Restoration must use retained process/profile ownership rather than this
    // consent-dependent accessor after Authority.safe() becomes false.
    bool category_source_ready() const {
        using namespace outfit_match_v2;
        if(!approved_ || expected_entries_!=2 || !published_ || !chain() || !directory_.unchanged())return false;
        StableId index,payload;
        if(!file_id(index_.value,false,index) || index!=index_id_ ||
            !file_id(payload_.value,false,payload) || payload!=payload_id_)return false;
        CategorySource first,second;
        return category_snapshot(first) && category_snapshot(second) && first==second &&
            directory_.unchanged() && file_id(index_.value,false,index) && index==index_id_ &&
            file_id(payload_.value,false,payload) && payload==payload_id_ && chain();
    }
};
} // namespace wnba_priority_v2
