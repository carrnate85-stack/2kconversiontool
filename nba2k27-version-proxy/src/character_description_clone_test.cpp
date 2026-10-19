#define NBA2K27_EXPAND_COPY
#include "character_description_clone_audit.hpp"
#include <cassert>
#include <cstdio>
namespace audit=character_description_clone_audit;
struct Fixture {
  std::array<unsigned char,0x2970> description{};
  std::array<std::uintptr_t,5> owner{};
  std::array<std::uintptr_t,2> array{};
  std::array<wchar_t,256> first{},second{};
  static constexpr std::uintptr_t base=0x10000000;
  void put(std::size_t at,std::uintptr_t value){std::memcpy(description.data()+at,&value,8);}
  Fixture() {
    std::wcscpy(first.data(),L"hoodie_test");std::wcscpy(second.data(),L"pants_test");
    array={reinterpret_cast<std::uintptr_t>(first.data()),reinterpret_cast<std::uintptr_t>(second.data())};
    owner={base+0x683A4C8,2,reinterpret_cast<std::uintptr_t>(array.data()),0,reinterpret_cast<std::uintptr_t>(array.data())};
    put(0,base+0x682AB58);put(0xB8,reinterpret_cast<std::uintptr_t>(description.data())+0x378);
    put(0xC8,reinterpret_cast<std::uintptr_t>(array.data()));put(0xD0,2);
    put(0x368,reinterpret_cast<std::uintptr_t>(owner.data()));
  }
  bool capture(audit::Snapshot &out){return audit::capture(reinterpret_cast<std::uintptr_t>(description.data()),base,out);}
};
int main() {
  Fixture a,b;audit::Snapshot first,second,again;
  assert(a.capture(first)&&b.capture(second));
  assert(audit::independent(first,second));assert(first.names==second.names);
  auto grown=second;grown.count=4;
  std::wcscpy(grown.names[2].data(),L"CODEX_PRIVATE_EXTRA_2");
  std::wcscpy(grown.names[3].data(),L"CODEX_PRIVATE_EXTRA_3");
  assert(audit::expanded_copy_matches(second,grown));
  const wchar_t *expected[]={second.names[0].data(),second.names[1].data(),L"CODEX_PRIVATE_EXTRA_2",L"CODEX_PRIVATE_EXTRA_3"};
  assert(audit::expanded_copy_matches(second,grown,expected));
  for(unsigned n=0;n<4;++n){auto wrong=grown;wrong.names[n][0]=L'!';assert(!audit::expanded_copy_matches(second,wrong,expected));}
  grown.bytes[0x7C]^=2;assert(!audit::expanded_copy_matches(second,grown));
  grown.bytes[0x7C]^=2;grown.names[1][0]=L'x';assert(!audit::expanded_copy_matches(second,grown));
  const wchar_t *all[]={L"top",L"bottom",L"sock",L"shoe",L"headband",L"glasses",L"backpack"};
  for(unsigned count=3;count<=7;++count){auto full=second;full.count=count;
    for(unsigned n=0;n<count;++n)std::wcscpy(full.names[n].data(),all[n]);
    assert(audit::expanded_copy_matches(second,full,all,count));
    for(unsigned n=0;n<count;++n){auto wrong=full;wrong.names[n][0]=L'!';assert(!audit::expanded_copy_matches(second,wrong,all,count));}
    full.bytes[0x7C]^=1;assert(!audit::expanded_copy_matches(second,full,all,count));
  }
  assert(!audit::expanded_copy_matches(second,grown,all,8));
  assert(!audit::expanded_copy_matches(second,grown,all,2));
  assert(!audit::expanded_copy_matches(second,grown,nullptr,3));
  auto edited=second;
  std::wcscpy(edited.names[0].data(),L"CODEX_TEMP_COPY_ONLY");
  assert(audit::first_item_changed_only(second,edited,L"CODEX_TEMP_COPY_ONLY"));
  edited.names[1][0]=L'x';assert(!audit::first_item_changed_only(second,edited,L"CODEX_TEMP_COPY_ONLY"));
  edited=second;edited.bytes[0x7C]^=1;assert(!audit::first_item_changed_only(second,edited,second.names[0].data()));
  edited=second;edited.owner++;assert(!audit::first_item_changed_only(second,edited,second.names[0].data()));
  assert(!audit::independent(first,first));
  assert(a.capture(again)&&audit::unchanged(first,again));
  second.items[1]=first.items[0];assert(!audit::independent(first,second));
  assert(b.capture(second));second.owner=first.owner;assert(!audit::independent(first,second));
  assert(b.capture(second));second.array=first.array;assert(!audit::independent(first,second));
  assert(b.capture(second));second.owned_array=first.owned_array;assert(!audit::independent(first,second));
  assert(b.capture(second));
  std::uintptr_t shared=0x55550000;
  std::memcpy(first.bytes.data()+0x2940,&shared,8);std::memcpy(second.bytes.data()+0x2958,&shared,8);
  assert(!audit::independent(first,second));assert(a.capture(first));
  a.description[0x7D]=1;assert(!a.capture(again));a.description[0x7D]=0;
  a.put(0xD0,8);assert(!a.capture(again));a.put(0xD0,2);
  a.put(0xB8,123);assert(!a.capture(again));a.put(0xB8,reinterpret_cast<std::uintptr_t>(a.description.data())+0x378);
  std::wcscpy(a.first.data(),L"VTXANI_JERSEY");assert(!a.capture(again));
  a.first.fill(L'x');assert(!a.capture(again));
  std::wcscpy(a.first.data(),L"different");assert(a.capture(again));assert(!audit::unchanged(first,again));
  a.array[0]=1;assert(!a.capture(again));
  std::puts("PASS: synthetic clone ownership, source changes, aliases, count/roster/override guards, jersey-reset case, invalid and unterminated strings. No native clone calls.");
}
