#include "freestyle_audio.hpp"
#include <cstring>
#include <cstdio>
int main(){
  char mask[72]={};std::memset(mask,'0',71);mask[0]='1';mask[4]='1';
  char event[128]={},target[128]={};
  for(unsigned i=0;i<nba2k27::kFreestyleTracks.size();++i){
    std::snprintf(event,sizeof(event),"newaudio/eventmusic/%06u-000000-000000-000000-000000-000000.wav",nba2k27::kFreestyleTracks[i]);
    const auto expected=i%2?5u:1u;
    if(nba2k27::freestyle_slot(event,mask,true,992,34)!=expected)return 1;
    if(nba2k27::freestyle_slot(event,mask,false,992,34)||nba2k27::freestyle_slot(event,mask,true,991,34)||nba2k27::freestyle_slot(event,mask,true,992,35))return 2;
    if(!nba2k27::freestyle_target(expected,target,sizeof(target))||nba2k27::audio_path_slot(target,"newaudio/jukeboxmusic/")!=expected)return 3;
  }
  for(const char* invalid:{"newaudio/eventmusic/000001-000000-000000-000000-000000-000000.wav",
      "newaudio/eventmusic/004400-000001-000000-000000-000000-000000.wav",
      "004400-000000-000000-000000-000000-000000.wav",
      "newaudio/jukeboxmusic/004400-000000-000000-000000-000000-000000.wav",
      "newaudio/eventmusic/../eventmusic/004400-000000-000000-000000-000000-000000.wav"})
    if(nba2k27::freestyle_slot(invalid,mask,true,992,34))return 4;
  if(nba2k27::freestyle_slot(event,"1",true,992,34)||nba2k27::freestyle_slot(event,nullptr,true,992,34))return 5;
  std::memset(mask,'0',71);if(nba2k27::freestyle_slot(event,mask,true,992,34))return 6;
  if(nba2k27::freestyle_target(0,target,sizeof(target))||nba2k27::freestyle_target(72,target,sizeof(target))||nba2k27::freestyle_target(1,target,5))return 7;
  std::puts("PASS: 58 event paths, context guards, sparse custom slots, invalid paths and target bounds.");
}
