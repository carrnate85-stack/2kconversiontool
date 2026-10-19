#include "../native/roster_appearance_v2_core.hpp"
#include <cassert>
#include <algorithm>
using namespace roster_appearance_v2;
struct Fake:Host {
 bool forward=true,retained=true,pinned=true,profile=true,after_fault=false,fail_record=false;
 unsigned records{},writes{},checks{},fault_at{};Atomic next=Atomic::applied;
 std::array<unsigned char,4> bytes{};std::vector<Phase> trace;
 bool authority(bool f)override{return f?forward:retained;}
 bool journal(const Request&)override{return pinned;}
 bool scope(const Request& r,bool after)override{++checks;return profile&&(!fault_at||checks!=fault_at)&&bytes==(after?r.after:r.before);}
 bool record(const Request&,Phase p)override{++records;trace.push_back(p);return !fail_record;}
 Atomic compare(const Request& r,bool undo)override {
  ++writes;const auto result=next;
  if(result==Atomic::applied)bytes=undo?r.before:r.after;
  if(after_fault)profile=false;
  return result;
 }
};
Request request() {
 Request r{};r.magic={'C','2','7','A','P','R','0','1'};r.version=1;r.operation=1;r.pid=7;r.birth=42;r.index=0;r.session.fill('a');r.id.fill('b');r.reply_owner=8;r.reply_handle=4;r.reply_volume=1;r.reply_index[0]=1;
 r.before={0xa0,0xce,0x08,0xd2};r.after={0xf0,0xc9,0x08,0xd2};r.mask={255,127,0,0};return r;
}
int main() {
 const auto r=request();assert(valid(r));{auto bad=r;bad.reply_index.fill(0);assert(!valid(bad));}
 for(unsigned i=0;i<10;++i) {auto bad=r;switch(i){case 0:bad.reserved=1;break;case 1:bad.index=8192;break;case 2:bad.mask[1]=255;break;case 3:bad.after[1]^=128;break;case 4:bad.id[0]='x';break;case 5:bad.before[0]=0;bad.before[1]=0;break;case 6:bad.operation=5;break;case 7:bad.after[2]^=1;break;case 8:bad.pid=0;break;default:bad.magic[0]='x';break;}assert(!valid(bad));}
 {Session s;Fake h;h.bytes=r.before;auto probe=r;probe.operation=4;probe.after=probe.before;assert(s.execute(probe,h)==Outcome::ready);assert(h.writes==0&&h.records==0&&s.count()==0);}
 {Session s;Fake h;h.bytes=r.before;assert(s.execute(r,h)==Outcome::applied);assert(h.trace[0]==Phase::apply_intent);assert(h.writes==1);assert(s.execute(r,h)==Outcome::refused);auto undo=r;undo.operation=2;h.forward=false;assert(s.execute(undo,h)==Outcome::restored);assert(h.bytes==r.before);assert(s.execute(undo,h)==Outcome::refused);assert(h.writes==2);}
 {Session s;Fake h;h.bytes=r.before;h.next=Atomic::conflict;assert(s.execute(r,h)==Outcome::conflict);h.bytes=r.after;auto undo=r;undo.operation=2;assert(s.execute(undo,h)==Outcome::refused);assert(h.writes==1);}
 {Session s;Fake h;h.bytes=r.before;h.next=Atomic::uncertain;assert(s.execute(r,h)==Outcome::uncertain);assert(s.held());h.bytes=r.after;auto undo=r;undo.operation=2;assert(s.execute(undo,h)==Outcome::refused);assert(h.writes==1);}
 {Session s;Fake h;h.bytes=r.before;h.fail_record=true;assert(s.execute(r,h)==Outcome::uncertain);assert(h.writes==0);assert(s.held());}
 {Session s;Fake h;h.bytes=r.before;h.fault_at=2;assert(s.execute(r,h)==Outcome::refused);assert(h.writes==0);assert(s.execute(r,h)==Outcome::refused);}
 {Session s;Fake h;h.bytes=r.before;h.after_fault=true;assert(s.execute(r,h)==Outcome::applied_held);h.profile=true;h.after_fault=false;auto undo=r;undo.operation=2;h.next=Atomic::uncertain;assert(s.execute(undo,h)==Outcome::uncertain);assert(h.writes==2);h.next=Atomic::applied;assert(s.execute(undo,h)==Outcome::refused);assert(h.writes==2);}
 {Session s;Fake h;h.bytes=r.before;assert(s.execute(r,h)==Outcome::applied);auto changed=r;changed.operation=2;changed.row[0]^=1;assert(s.execute(changed,h)==Outcome::refused);assert(h.writes==1);auto commit=r;commit.operation=3;assert(s.execute(commit,h)==Outcome::committed);assert(s.execute(changed,h)==Outcome::refused);assert(h.writes==1);}
 {Session s;Fake h;h.bytes=r.before;for(unsigned i=0;i<64;++i){auto next=r;next.id[0]="0123456789abcdef"[i&15];next.id[1]="0123456789abcdef"[i>>4];assert(s.execute(next,h)==Outcome::applied);next.operation=3;assert(s.execute(next,h)==Outcome::committed);h.bytes=r.before;}auto probe=r;probe.operation=4;assert(s.execute(probe,h)==Outcome::refused);auto over=r;over.id[0]='e';over.id[1]='e';assert(s.execute(over,h)==Outcome::refused);assert(h.writes==64);}
}
