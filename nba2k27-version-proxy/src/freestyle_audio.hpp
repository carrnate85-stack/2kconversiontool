#pragma once
#include "audio_index_override.hpp"
#include <array>
#include <cstdio>

namespace nba2k27 {
// Exact-build event 992 (0xFAB3211A), captured twice from the live resource table.
inline constexpr std::array<unsigned,58> kFreestyleTracks={
  4400,4405,4408,4412,4417,4421,4433,4438,4447,4451,4456,4461,4463,4465,
  4475,4477,4481,4488,4494,4498,4502,4506,4511,4515,4520,4523,4527,4536,
  4540,4550,4554,4568,4572,4576,4582,4587,4590,4597,4602,4607,4611,4614,
  4617,4622,4627,4394,4429,4426,4443,4469,4472,4485,4531,4544,4547,4559,4563,4593};
inline unsigned freestyle_slot(const char* path,const char* custom_mask,
                              bool enabled,unsigned event,unsigned bank){
  if(!enabled||event!=992||bank!=34||!valid_audio_mask(custom_mask,71))return 0;
  const auto id=audio_path_slot(path,"newaudio/eventmusic/");
  unsigned position=0;
  for(;position<kFreestyleTracks.size();++position)if(kFreestyleTracks[position]==id)break;
  if(position==kFreestyleTracks.size())return 0;
  std::array<unsigned,71> slots={};unsigned count=0;
  for(unsigned i=0;i<71;++i)if(custom_mask[i]=='1')slots[count++]=i+1;
  return count?slots[position%count]:0;
}
inline bool freestyle_target(unsigned slot,char* output,std::size_t size){
  if(!slot||slot>71||!output||size<80)return false;
  const int written=std::snprintf(output,size,
    "newaudio/jukeboxmusic/%06u-000000-000000-000000-000000-000000.wav",slot);
  return written>0&&static_cast<std::size_t>(written)<size;
}
}
