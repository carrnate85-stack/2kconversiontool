// Included inside coach_clone_audit. Experimental coach-only queue submission.
// Reuse read-only queue/idle policies, not the player-specific rebuild function.
inline bool unchanged(const Snapshot &a,const Snapshot &b){return a.address==b.address&&a.bytes==b.bytes&&a.names==b.names&&a.items==b.items&&a.resources==b.resources;}
#include "character_same_outfit_rebuild.hpp"
namespace coach_rebuild {
struct Result {bool submitted{},witnessed{},drained{},replaced{},clothing{},same_size{};float before{},after{},factor=1.f;const char *reason="Not submitted";};
inline bool same_size(float a,float b){return a>0.1f&&a<10.f&&b>0.1f&&b<10.f&&std::abs(a-b)<0.0001f;}
inline bool scale_input(float f){return std::isfinite(f)&&f>=0.9309f&&f<=0.98001f;}
inline bool fingerprints(std::uintptr_t base){
 return mem::q(base+0x67f8d48)==base+0x800e10&&mem::q(base+0x67f8d48+0x98)==base+0x60b8e0&&
 mem::q(base+0x681d088+0x150)==base+0x6f40f0&&
 mem::match_hex(base+0x60b8e0,"488b4108488b511048895010488b4108488942084889490848894910c3")&&
 mem::match_hex(base+0x5f9430,"555657534883ec38488d6c243048c74500feffffff4889ce4883c12048894df8e83b915404488b7e184883c6084839f70f94c04885ff0f94c108c175350f1f00488b07488b5f104889f9ff90980000004889f9e8c8000000488b074889f9ba01000000ff104839f374084889df4885db75ce488b4df8e8a5905404904883c4385b5f5e5dc3")&&
 mem::match_hex(base+0x5f9550,"56574883ec38488b79184885ff0f84ec0000004889ce488b41284885c00f84dc00000083b888000000007517c78088000000c5fe1bdc488b7e184885ff0f84c400000048397f08750648397f10740c488b074889f9ff9098000000488b05062ad80948894708488d0df329d80948894f1048897810488b471048897808c7879000000001000000488b4e184c8b46208b463c448b4e38488b56284c8b51784883c1788944242844894c24204531c941ff9250010000488b4e18488b4628448b8088000000488b41784883c178bae1985570ff9078010000488b4e18488b5630448b4e3c448b4638488b41784883c178488b80a00000004883c4385f5e48ffe0904883c4385f5ec331ffe96fffffff")&&
 mem::match_hex(base+0x800e10,"5556574883ec30488d6c243048c745f8feffffff89d74889ce488d05187fff05488901488b49284885c9740a488b01ba01000000ff10c5f857c0c5fc114618488d051286ff05488906c5f857c0c5f811460885ff740b4889f1c5f877e8cfe431044889f04883c4305f5e5dc5f877c3")&&
 mem::match_hex(base+0x60bb10,"56574883ec284885d274364889d64889cf48395208750648397610740c488b064889f1ff9098000000488b47084889460848897e1048897010488b4610488970084883c4285f5ec3");
}
template<class Reporter>Result run(std::uintptr_t base,std::uintptr_t manager,std::uintptr_t actor,std::uintptr_t clone,const Snapshot &source,bool(*anti_cheat)(),Reporter report){
 Result result;const auto context=mem::q(manager+0x20),root=mem::q(base+0xa3a69e8);
#ifdef NBA2K27_COACH_ALL_SCALE
 if((source.bytes[0x7d]&2)||!mem::value(base+0xa38f19c,&result.factor)||!scale_input(result.factor)||
 !mem::match_hex(base+0x6de34a,"c5fa10054a0ecb09")||!mem::match_hex(base+0x6de66e,"c5fa1005260bcb09")){
 result.reason="Shared scale input or original coach flag rejected";return result;}
#endif
 const auto main_thread=mem::i(base+0x89986d0),label=mem::i(clone+0x88);
 auto binding=[&](){
#ifdef NBA2K27_COACH_ALL_SCALE
 float current_factor=0;if(!mem::value(base+0xa38f19c,&current_factor)||current_factor!=result.factor)return false;
#endif
 return !anti_cheat()&&mem::exact_build(base)&&mem::i(mem::q(base+0xaac79a0)+0xa04)==1&&
  root&&mem::q(base+0xa3a69e8)==root&&mem::q(root)==base+0x683d918&&mem::q(manager)==base+0x681d088&&
  same_outfit_rebuild::active_member(base,manager-0x78)&&mem::q(manager+0x20)==context&&mem::i(base+0x89986d0)==main_thread;};
 if(!context||!mem::q(context)||!mem::q(context+0x18)||!main_thread||!label||!binding()||
  !same_outfit_rebuild::idle(base,manager,actor,label)||!same_outfit_rebuild::queue_ready(base,true)||
  !mem::value(actor+0xbcc,&result.before)||!same_size(result.before,result.before)){
  result.reason="Coach manager, context or queue not ready";return result;}
 if(!fingerprints(base)){result.reason="Native queue fingerprint rejected";return result;}
 using Allocate=void *(*)(std::size_t,void *,unsigned,unsigned,unsigned,unsigned,bool,bool);
 const auto heap=mem::q(base+0xa37c1d8);if(!heap||!mem::q(heap)){result.reason="Heap unavailable";return result;}
 auto packet=reinterpret_cast<std::uintptr_t>(reinterpret_cast<Allocate>(base+0x4b1f0d0)(0x40,reinterpret_cast<void *>(heap),0,0xad7fd911,0x673,8,true,false));
 if(!packet){result.reason="Request allocation failed";return result;}
 std::array<std::uintptr_t,8> fields{};fields[0]=base+0x67f8d48;fields[1]=fields[2]=packet;
 fields[3]=manager-0x78;fields[4]=context+0x18;fields[7]=std::uintptr_t(mem::i(manager+8))|(std::uintptr_t(mem::i(manager+0xc))<<32);
 std::memcpy(reinterpret_cast<void *>(packet),fields.data(),0x40);
 {mem::CriticalGuard lock(base+0xa37c128);Snapshot current{};
  if(lock.entered&&binding()&&same_outfit_rebuild::idle(base,manager,actor,label)&&same_outfit_rebuild::queue_ready(base,true)&&capture(source.address,base,current)&&unchanged(source,current)){
   // Request a fresh model from the private copy. Shared-scale flag +0x7D stays unchanged.
   *reinterpret_cast<unsigned char *>(clone+0x7c)|=2;
#ifdef NBA2K27_COACH_ALL_SCALE
   *reinterpret_cast<unsigned char *>(clone+0x7d)|=2;
#endif
   *reinterpret_cast<std::uintptr_t *>(packet+0x28)=clone;
   using Insert=void(*)(void *,void *);reinterpret_cast<Insert>(base+0x60bb10)(reinterpret_cast<void *>(base+0xa37c110),reinterpret_cast<void *>(packet));
   result.submitted=true;const auto sentinel=base+0xa37c110;
   result.witnessed=mem::q(sentinel+8)==packet&&mem::q(sentinel+0x10)==packet&&mem::q(packet+8)==sentinel&&mem::q(packet+0x10)==sentinel&&mem::q(packet+0x28)==clone;
  }
 }
 if(!result.submitted){using Free=void(*)(void *);reinterpret_cast<Free>(base+0x4b1f340)(reinterpret_cast<void *>(packet));result.reason="Preconditions changed; clone still caller-owned";return result;}
 // Native request owns clone and packet after transfer. Never free either here.
 if(!result.witnessed){result.reason="Transfer not witnessed; close game";return result;}
 report("PENDING","One coach rebuild queued; remain paused");
 for(unsigned n=0;n<240;++n){Sleep(250);
  if(!binding()||!same_outfit_rebuild::queue_ready(base,false)){result.reason="Session changed after submission; close game";return result;}
  {mem::CriticalGuard lock(base+0xa37c128);if(lock.entered&&same_outfit_rebuild::queue_ready(base,true))result.drained=true;}
  const auto now=mem::q(manager+0x98);
  if(result.drained&&now&&now!=actor&&mem::q(now)==base+0x6804e58&&same_outfit_rebuild::idle(base,manager,now,label)){
   Snapshot rebuilt{};if(!capture(now+0xf20,base,rebuilt))continue;
   auto expected_flag=source.bytes[0x7d];
#ifdef NBA2K27_COACH_ALL_SCALE
   expected_flag|=2;
#endif
   result.replaced=true;result.clothing=source.count==rebuilt.count&&source.names==rebuilt.names&&expected_flag==rebuilt.bytes[0x7d];
   if(mem::value(now+0xbcc,&result.after))result.same_size=same_size(result.before*result.factor,result.after);
   result.reason="Coach replacement observed; check appearance and gameplay";return result;
  }
 }
 result.reason="Replacement not verified within 60 seconds; close game";return result;
}
}
