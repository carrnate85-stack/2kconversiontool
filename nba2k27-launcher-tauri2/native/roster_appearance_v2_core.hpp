#pragma once
// Narrow in-process appearance command policy. No code patching or game calls.
#include <array>
#include <cstdint>
#include <cstring>
#include <string>
#include <vector>
namespace roster_appearance_v2 {
using Id=std::array<char,48>;
using Sha=std::array<unsigned char,32>;
enum class Operation:std::uint32_t { apply=1,undo=2,commit=3,probe=4 };
enum class Outcome:std::uint32_t { refused=0,applied=1,applied_held=2,restored=3,committed=4,uncertain=5,conflict=6,ready=7 };
enum class Phase:std::uint32_t { apply_intent=1,applied=2,applied_held=3,undo_intent=4,restored=5,commit_intent=6,committed=7,no_effect=8,uncertain=9 };
#pragma pack(push,1)
struct Request {
 std::array<char,8> magic{};
 std::uint32_t version{},operation{},pid{},index{};
 std::uint64_t birth{};
 Id session{},id{};
 std::array<unsigned char,4> before{},after{},mask{};
 std::uint32_t reserved{};
 Sha row{},pointers{},metadata{},journal{};
 std::uint32_t reply_owner{},reply_reserved{};std::uint64_t reply_handle{},reply_volume{};std::array<unsigned char,16> reply_index{};
};
struct Reply {
 std::array<char,8> magic{};
 Id session{},id{};
 Sha request{};
 std::uint32_t outcome{},reserved{};
};
#pragma pack(pop)
static_assert(sizeof(Request)==312);
static_assert(sizeof(Reply)==144);
inline bool id_valid(const Id& id) {
 for(char c:id)if(!((c>='0'&&c<='9')||(c>='a'&&c<='f')||(c>='A'&&c<='F')))return false;
 return true;
}
inline std::uint32_t word(const std::array<unsigned char,4>& b) {std::uint32_t v{};std::memcpy(&v,b.data(),4);return v;}
inline bool valid(const Request& r) {
 if(r.magic!=std::array<char,8>{'C','2','7','A','P','R','0','1'}||r.version!=1||r.reserved||!r.pid||!r.birth||r.index>=8192||!id_valid(r.session)||!id_valid(r.id)||r.operation<1||r.operation>4||!r.reply_owner||r.reply_owner==r.pid||r.reply_reserved||r.reply_handle<4||!r.reply_volume)return false;
 bool file_id=false;for(auto b:r.reply_index)file_id|=b!=0;if(!file_id)return false;
 bool any=false;
 for(unsigned i=0;i<2;++i) {
  const auto at=i*2;const auto low=r.mask[at],high=r.mask[at+1];
  if(!((low==0&&high==0)||(low==255&&high==127)))return false;
  any|=low!=0;
  unsigned a=unsigned(r.before[at])|(unsigned(r.before[at+1])<<8);
  unsigned b=unsigned(r.after[at])|(unsigned(r.after[at+1])<<8);
  if(((a^b)&0x8000)||((a^b)&~(unsigned(low)|(unsigned(high)<<8))))return false;
  const unsigned max=i?32000:26000;
  if((a&32767)<12000||(a&32767)>max||(b&32767)<12000||(b&32767)>max)return false;
 }
 return any&&(r.operation==4||r.before!=r.after);
}
inline bool same_effect(const Request& a,const Request& b) {
 return a.pid==b.pid&&a.birth==b.birth&&a.index==b.index&&a.session==b.session&&a.id==b.id&&a.before==b.before&&a.after==b.after&&a.mask==b.mask&&a.row==b.row&&a.pointers==b.pointers&&a.metadata==b.metadata;
}
enum class Atomic { conflict,applied,uncertain };
struct Host {
 virtual ~Host()=default;
 // forward=false is retained proof, NEVER a new-operation permission.
 virtual bool authority(bool forward)=0;
 virtual bool journal(const Request&)=0;
 virtual bool scope(const Request&,bool after)=0;
 virtual bool record(const Request&,Phase)=0;
 // Must derive address from the exact checked native scope, then one actual
 // InterlockedCompareExchange32. It must not emulate this with remote writes.
 virtual Atomic compare(const Request&,bool undo)=0;
};
class Session {
 struct Owned {Request request;bool effect{},undo_attempted{},closed{};};
 std::vector<Owned> records_;
 bool held_{};
 static constexpr std::size_t kMax=64;
public:
 bool held()const{return held_;}
 std::size_t count()const{return records_.size();}
 Outcome execute(const Request& r,Host& h) {
  if(held_||!valid(r))return Outcome::refused;
  const auto op=Operation(r.operation);
  if(op==Operation::probe){if(records_.size()>=kMax)return Outcome::refused;for(const auto& prior:records_)if(!prior.closed)return Outcome::refused;return h.authority(true)&&h.scope(r,false)?Outcome::ready:Outcome::refused;}
  auto found=records_.end();for(auto i=records_.begin();i!=records_.end();++i)if(i->request.id==r.id){found=i;break;}
  if(op==Operation::apply) {
   if(found!=records_.end()||records_.size()>=kMax)return Outcome::refused;
   for(const auto& prior:records_)if(!prior.closed)return Outcome::refused;
   if(!h.authority(true)||!h.journal(r)||!h.scope(r,false))return Outcome::refused;
   // Consume before first possible effect. A failure never makes it replayable.
   records_.push_back({r});auto& owned=records_.back();
   if(!h.record(r,Phase::apply_intent)){held_=true;return Outcome::uncertain;}
   if(!h.authority(true)||!h.journal(r)||!h.scope(r,false)) {
    if(!h.record(r,Phase::no_effect))held_=true;
    owned.closed=true;return held_?Outcome::uncertain:Outcome::refused;
   }
   const auto atomic=h.compare(r,false);
   if(atomic==Atomic::conflict) {
    owned.closed=true;if(!h.record(r,Phase::no_effect))held_=true;
    return held_?Outcome::uncertain:Outcome::conflict;
   }
   if(atomic!=Atomic::applied){h.record(r,Phase::uncertain);held_=true;return Outcome::uncertain;}
   owned.effect=true;
   const bool checked=h.scope(r,true)&&h.authority(false);
   if(!h.record(r,checked?Phase::applied:Phase::applied_held)){held_=true;return Outcome::uncertain;}
   return checked?Outcome::applied:Outcome::applied_held;
  }
  if(found==records_.end()||!same_effect(found->request,r)||!found->effect||found->closed||found->undo_attempted)return Outcome::refused;
  if(!h.authority(false)||!h.journal(r)||!h.scope(r,true))return Outcome::refused;
  if(op==Operation::undo) {
   if(!h.record(r,Phase::undo_intent)){held_=true;return Outcome::uncertain;}
   if(!h.authority(false)||!h.journal(r)||!h.scope(r,true)){held_=true;return Outcome::uncertain;}
   // Even an uncertain reverse call consumes its receipt, before that call.
   found->undo_attempted=true;
   const auto atomic=h.compare(r,true);
   if(atomic!=Atomic::applied){h.record(r,Phase::uncertain);held_=true;return Outcome::uncertain;}
   found->effect=false;
   if(!h.scope(r,false)||!h.authority(false)||!h.record(r,Phase::restored)){held_=true;return Outcome::uncertain;}
   found->closed=true;return Outcome::restored;
  }
  if(!h.record(r,Phase::commit_intent)||!h.authority(false)||!h.journal(r)||!h.scope(r,true)||!h.record(r,Phase::committed)){held_=true;return Outcome::uncertain;}
  found->closed=true;found->effect=false;return Outcome::committed;
 }
};
} // namespace roster_appearance_v2
