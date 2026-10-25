#include "../native/wnba_blacktop_roster_win.hpp"
#include <cassert>
#include <iostream>
struct RefusedCategory {
 wnba_blacktop_category::Binding value{};
 unsigned checkpoint_calls{};
 const auto& binding()const{return value;}
 std::uintptr_t game_base()const{return 0;}
 bool roster_checkpoint(const wnba_blacktop_category::Binding&,bool){++checkpoint_calls;return false;}
 const char* reason()const{return "fixture retained source differs";}
};
int main(){
 using namespace wnba_blacktop_roster;
 assert(format_telemetry({})=="ret=0 ctx=? d=?/?/? uid=?");
 constexpr std::uintptr_t base=0x140000000;
 Telemetry detail{};detail.retained=detail.context_read=detail.stable=detail.uid_read=true;
 detail.current=detail.other=0x92DF06DE;detail.stage=3;detail.uid=29;
 detail.installed=ImageMetadata::from(true,base+0x9B31BB0,base);
 detail.requested=ImageMetadata::from(true,base+0x8AE67D8,base);
 detail.playnow=ImageMetadata::from(true,base+0x9B31BB0,base);
 assert(format_telemetry(detail)=="ret=1 ctx=stable:92df06de/92df06de,s=3,a=0,k=0 d=9b31bb0/8ae67d8/9b31bb0 uid=29");
 detail.stable=false;detail.other=0x6509837B;detail.uid_read=false;
 detail.installed=ImageMetadata::from(true,0,base);
 detail.requested=ImageMetadata::from(true,base-8,base);
 detail.playnow=ImageMetadata::from(false,base+0x9B31BB0,base);
 assert(format_telemetry(detail)=="ret=1 ctx=unstable:92df06de/6509837b,s=3,a=0,k=0 d=null/foreign/? uid=?");
 assert(image_metadata(ImageMetadata::from(true,base+0x36532000,base))=="foreign");

 // Exact observed QA067 metadata: image data, loader copy-on-write, same image.
 // Pure values only: these assertions invoke no process or native game APIs.
 namespace p=wnba_blacktop_roster_profile;
 for(auto protection:{0x02u,0x04u,0x08u}) {
  assert(p::delegate_image_metadata(0x1000000,protection,true,true));
  assert(!p::delegate_image_metadata(0x20000,protection,true,true));
  assert(!p::delegate_image_metadata(0x1000000,protection,true,false));
  assert(!p::delegate_image_metadata(0x1000000,protection,false,true));
 }
 for(auto protection:{0u,0x01u,0x10u,0x20u,0x40u,0x80u,0x102u,0x104u,0x108u})
  assert(!p::delegate_image_metadata(0x1000000,protection,true,true));

 RefusedCategory unbound;
 wnba_blacktop_roster::NativeHost<RefusedCategory> first;
 assert(!first.open(unbound));
 assert(std::string(first.reason())=="Blacktop roster admission invalid session binding");
 assert(unbound.checkpoint_calls==0);
 RefusedCategory source;
 source.value={1,2,std::string(48,'a')};
 wnba_blacktop_roster::NativeHost<RefusedCategory> second;
 assert(!second.open(source));
 assert(std::string(second.reason())=="fixture retained source differs");
 assert(source.checkpoint_calls==1);
 // No native target resolution or diagnostics reads for a zero image base.
 std::cout<<"Blacktop roster distinct no-call admission reasons passed\n";
}
