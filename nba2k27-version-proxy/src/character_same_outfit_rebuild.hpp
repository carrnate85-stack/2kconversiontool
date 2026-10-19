// Included inside character_description_clone_audit after Snapshot helpers.
// Experimental existing-manager use of the STOCK create request. No callback,
// shared vtable replacement, or code patch. Queue ownership transfers once.
namespace same_outfit_rebuild {
struct Result {
  bool submitted{},witnessed{},drained{},replaced{},bindings{},clothing{};
  std::uintptr_t new_player{};
  const char *reason="Not submitted";
};
inline bool active_member(std::uintptr_t base,std::uintptr_t complete) {
  const auto sentinel=base+0xA37BFB0;
  auto node=mem::q(sentinel+0x10),previous=sentinel;
  unsigned matches=0;
  for(unsigned n=0;n<256;++n) {
    if(node==sentinel)return matches==1&&mem::q(sentinel+8)==previous;
    if(node<0x10000||mem::q(node+8)!=previous)return false;
    if(node==complete)++matches;
    previous=node;node=mem::q(node+0x10);
  }
  return false;
}
inline bool idle(std::uintptr_t base,std::uintptr_t manager,std::uintptr_t player,unsigned label) {
  if(mem::q(manager)!=base+0x681D088||mem::q(manager-0x78)!=base+0x681CCC8||
      mem::q(manager+0x98)!=player||mem::q(manager+0xA0)!=0||mem::i(manager+0x18)!=1||
      !active_member(base,manager-0x78))return false;
  unsigned char pending=1;
  if(!mem::value(manager+0xB8,&pending)||pending)return false;
  const auto sentinel=manager+0x80;
  auto node=mem::q(manager+0x90),previous=sentinel;
  unsigned matching=0;
  for(unsigned n=0;n<16;++n) {
    if(node==sentinel)return matching==1&&mem::q(manager+0x88)==previous;
    if(node<0x10000||mem::q(node)!=base+0x681CB68||mem::q(node+8)!=previous||mem::q(node+0x38))return false;
    if(mem::i(node+0x18)==label) {
      if(mem::q(node+0x20)!=player)return false;
      ++matching;
    }
    previous=node;node=mem::q(node+0x10);
  }
  return false;
}
inline bool queue_ready(std::uintptr_t base,bool require_empty) {
  const auto queue=base+0xA37C108,sentinel=queue+8;
  unsigned char initialized=0;
  return mem::q(queue)==base+0x68506A8&&mem::value(queue+0x5A,&initialized)&&initialized==1&&
    (!require_empty||(mem::q(queue+0x10)==sentinel&&mem::q(queue+0x18)==sentinel));
}
template<class Reporter>
Result run(std::uintptr_t base,std::uintptr_t record,std::uintptr_t manager,
           std::uintptr_t player,std::uintptr_t clone,const Snapshot &source,
           bool (*anti_cheat)(),Reporter report,const Snapshot *expected=nullptr) {
  Result result{};
  const auto label=mem::i(clone+0x88);
  const auto roster=mem::q(record+0x30),bound=mem::q(record+0x28),state=mem::q(record+0x18);
  const auto context=mem::q(manager+0x20);
  const auto main_thread=mem::i(base+0x89986D0);
  auto binding=[&](){return mem::in_match(base)&&!anti_cheat()&&mem::q(record)==base+0x75BD1F8&&
    mem::q(record+0x10)==manager&&mem::q(record+0x30)==roster&&mem::q(record+0x28)==bound&&
    mem::q(record+0x18)==state&&mem::q(roster+0x518)==manager&&mem::q(roster+0x520)==state&&
    mem::q(manager+0x20)==context&&mem::i(base+0x89986D0)==main_thread;};
  if(!label||!roster||!bound||!state||!context||!mem::q(context)||!mem::q(context+0x18)||
      !main_thread||!binding()||!idle(base,manager,player,label)||!queue_ready(base,true)) {
    result.reason="Existing manager, bindings or queue not in verified idle state";return result;
  }
  if(mem::q(base+0x67F8D48)!=base+0x800E10||mem::q(base+0x67F8D48+0x98)!=base+0x60B8E0||
      !mem::match_hex(base+0x60B8E0,"488b4108488b511048895010488b4108488942084889490848894910c3")||
      !mem::match_hex(base+0x5F9430,"555657534883ec38488d6c243048c74500feffffff4889ce4883c12048894df8e83b915404488b7e184883c6084839f70f94c04885ff0f94c108c175350f1f00488b07488b5f104889f9ff90980000004889f9e8c8000000488b074889f9ba01000000ff104839f374084889df4885db75ce488b4df8e8a5905404904883c4385b5f5e5dc3")||
      !mem::match_hex(base+0x5F9550,"56574883ec38488b79184885ff0f84ec0000004889ce488b41284885c00f84dc00000083b888000000007517c78088000000c5fe1bdc488b7e184885ff0f84c400000048397f08750648397f10740c488b074889f9ff9098000000488b05062ad80948894708488d0df329d80948894f1048897810488b471048897808c7879000000001000000488b4e184c8b46208b463c448b4e38488b56284c8b51784883c1788944242844894c24204531c941ff9250010000488b4e18488b4628448b8088000000488b41784883c178bae1985570ff9078010000488b4e18488b5630448b4e3c448b4638488b41784883c178488b80a00000004883c4385f5e48ffe0904883c4385f5ec331ffe96fffffff")||
      !mem::match_hex(base+0x800E10,"5556574883ec30488d6c243048c745f8feffffff89d74889ce488d05187fff05488901488b49284885c9740a488b01ba01000000ff10c5f857c0c5fc114618488d051286ff05488906c5f857c0c5f811460885ff740b4889f1c5f877e8cfe431044889f04883c4305f5e5dc5f877c3")||
      !mem::match_hex(base+0x60BB10,"56574883ec284885d274364889d64889cf48395208750648397610740c488b064889f1ff9098000000488b47084889460848897e1048897010488b4610488970084883c4285f5ec3")||
      mem::q(base+0x681D088+0x150)!=base+0x6F40F0) {
    result.reason="Native create queue fingerprint rejected";return result;
  }
  // Native allocation parameters match the stock producer at5F9A49..5F9A78.
  using Allocate=void *(*)(std::size_t,void *,unsigned,unsigned,unsigned,unsigned,bool,bool);
  const auto heap=mem::q(base+0xA37C1D8);
  auto packet=reinterpret_cast<std::uintptr_t>(reinterpret_cast<Allocate>(base+0x4B1F0D0)(
    0x40,reinterpret_cast<void *>(heap),0,0xAD7FD911,0x673,8,true,false));
  if(!packet){result.reason="Native request allocation failed";return result;}
  std::array<std::uintptr_t,8> fields{};
  fields[0]=base+0x67F8D48;fields[1]=packet;fields[2]=packet;
  fields[3]=manager-0x78;fields[4]=context+0x18;
  // clone pointer remains zero until transfer; caller owns it if enqueue fails.
  const auto tag=mem::i(manager+8),line=mem::i(manager+0xC);
  fields[7]=std::uintptr_t(tag)|(std::uintptr_t(line)<<32);
  std::memcpy(reinterpret_cast<void *>(packet),fields.data(),0x40);
  {
    mem::CriticalGuard guard(base+0xA37C128);
    Snapshot current{};
    if(guard.entered&&queue_ready(base,true)&&binding()&&idle(base,manager,player,label)&&
        capture(source.address,base,current)&&unchanged(source,current)) {
      // Temporary description only: preview refresh uses the same bit.
      *reinterpret_cast<unsigned char *>(clone+0x7C)|=2;
      *reinterpret_cast<std::uintptr_t *>(packet+0x28)=clone;
      using Insert=void (*)(void *,void *);
      reinterpret_cast<Insert>(base+0x60BB10)(reinterpret_cast<void *>(base+0xA37C110),reinterpret_cast<void *>(packet));
      result.submitted=true;
      const auto sentinel=base+0xA37C110;
      result.witnessed=mem::q(base+0xA37C118)==packet&&mem::q(base+0xA37C120)==packet&&
        mem::q(packet+8)==sentinel&&mem::q(packet+0x10)==sentinel&&mem::q(packet+0x28)==clone;
      // Native request/destructor now own clone and packet. Never free or inspect
      // packet after dropping this lock, including when observation fails.
    }
  }
  if(!result.submitted) {
    using Free=void (*)(void *);
    reinterpret_cast<Free>(base+0x4B1F340)(reinterpret_cast<void *>(packet));
    result.reason="Preconditions changed before transfer; clone still caller-owned";return result;
  }
  if(!result.witnessed){result.reason="Request transfer not witnessed; ownership uncertain, close game";return result;}
  report("REBUILD_PENDING","One stock request submitted; stay paused while native model loads");
  for(unsigned n=0;n<240;++n) {
    Sleep(250);
    if(!binding()||!queue_ready(base,false)){result.reason="Session changed after submission; close game, no manual rollback";return result;}
    {
      mem::CriticalGuard guard(base+0xA37C128);
      if(guard.entered&&queue_ready(base,true))result.drained=true;
    }
    const auto now=mem::q(manager+0x98);
    if(result.drained&&now&&now!=player&&mem::q(now)==base+0x6829528) {
      Snapshot rebuilt{};
      if(capture(now+0xF20,base,rebuilt)&&idle(base,manager,now,label)) {
        result.replaced=true;result.new_player=now;result.bindings=binding();
        const auto &desired=expected?*expected:source;
        result.clothing=desired.count==rebuilt.count&&desired.names==rebuilt.names;
        result.reason="Native replacement observed; user must confirm visibility and gameplay";return result;
      }
    }
  }
  result.reason="No verified completed replacement within 60 seconds; close game, no forced cleanup";
  return result;
}
} // namespace same_outfit_rebuild
