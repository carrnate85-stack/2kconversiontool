// Bounded transport used only by the isolated saved-outfit diagnostic.
inline bool saved_trigger_matches(const char *data,std::size_t size) {
  constexpr char token[]="SAVED_OUTFITS_V1";
  return size==sizeof(token)-1&&std::memcmp(data,token,sizeof(token)-1)==0;
}
constexpr unsigned kSavedMaxPieces=7;
constexpr unsigned kSavedV3Stride=16+kSavedMaxPieces*512;
constexpr unsigned kSavedMaxBytes=24+10*kSavedV3Stride;
struct SavedChoice {std::uint64_t roster{};unsigned pieces=4;std::array<std::array<wchar_t,256>,kSavedMaxPieces> names{};};
struct SavedPlan {unsigned count{};std::array<SavedChoice,10> choices{};};
inline bool parse_saved_plan(const unsigned char *data,std::size_t size,unsigned pid,std::uint64_t started,SavedPlan &out) {
  out={};if(size<24)return false;
  auto u32=[&](unsigned at){unsigned v;std::memcpy(&v,data+at,4);return v;};
  auto u64=[&](unsigned at){std::uint64_t v;std::memcpy(&v,data+at,8);return v;};
  const auto count=u32(8);
  const bool v3=u32(0)==0x33504F42,v2=u32(0)==0x32504F42,variable=v2||v3;
  const unsigned capacity=v3?kSavedMaxPieces:(v2?5:4),stride=v3?kSavedV3Stride:(v2?2576:2056);
  if((!variable&&u32(0)!=0x31504F42)||u32(4)!=pid||!count||count>10||u32(12)||u64(16)!=started||size!=24+count*stride)return false;
  out.count=count;
  for(unsigned n=0;n<count;++n) {
    const auto at=24+n*stride;auto &c=out.choices[n];c.roster=u64(at);
    c.pieces=variable?u32(at+8):4;
    // V3 permits top/bottom/socks with shoes retained from the roster copy.
    if(c.pieces<(v3?3u:4u)||c.pieces>capacity||(variable&&u32(at+12)))return false;
    if(c.roster<0x10000||c.roster>=0x800000000000ULL)return false;
    for(unsigned k=0;k<n;++k)if(out.choices[k].roster==c.roster)return false;
    std::memcpy(c.names.data(),data+at+(variable?16:8),capacity*512);
    for(unsigned piece=c.pieces;piece<capacity;++piece)for(const auto ch:c.names[piece])if(ch)return false;
    for(unsigned piece=0;piece<c.pieces;++piece) {const auto &name=c.names[piece];
      // Exact stock sentinel only; other whitespace-bearing names stay invalid.
      constexpr wchar_t shirtless[]=L"NO SHIRT";
      if(std::memcmp(name.data(),shirtless,sizeof(shirtless))==0) {
        for(unsigned j=sizeof(shirtless)/sizeof(wchar_t);j<256;++j)if(name[j])return false;
        continue;
      }
      bool ended=false;
      for(unsigned j=0;j<256;++j) {
        const auto ch=name[j];
        if(!ch){if(!j)return false;ended=true;continue;}
        if(ended||!((ch>=L'a'&&ch<=L'z')||(ch>=L'A'&&ch<=L'Z')||(ch>=L'0'&&ch<=L'9')||ch==L'_'))return false;
      }
      if(!ended)return false;
    }
  }
  return true;
}
