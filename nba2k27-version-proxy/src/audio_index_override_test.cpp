#include "audio_index_override.hpp"
#include <cstdio>
#include <initializer_list>
int main() {
  using namespace nba2k27;
  char main_mask[72]={},park_mask[55]={};
  std::memset(main_mask,'0',71);std::memset(park_mask,'0',54);main_mask[0]='1';
  if(!valid_audio_mask(main_mask,71)||valid_audio_mask("1",71))return 10;
  if(audio_path_blocked(main_mask,park_mask,"newaudio/jukeboxmusic/000001-000000-000000-000000-000000-000000.wav"))return 11;
  if(!audio_path_blocked(main_mask,park_mask,"newaudio/jukeboxmusic/000002-000000-000000-000000-000000-000000.wav"))return 12;
  if(!audio_path_blocked(main_mask,park_mask,"newaudio/jukeboxmusic/000080-000000-000000-000000-000000-000000.wav"))return 13;
  if(!audio_path_blocked(main_mask,park_mask,"newaudio/jukeboxmusic_park/000001-000000-000000-000000-000000-000000.wav"))return 14;
  if(audio_path_blocked(main_mask,park_mask,"newaudio/commentary/000001-000000-000000-000000-000000-000000.wav"))return 15;
  if(audio_path_blocked("",park_mask,"newaudio/jukeboxmusic/000002-000000-000000-000000-000000-000000.wav"))return 16;
  if(!audio_custom_path(main_mask,"newaudio/jukeboxmusic/000001-000000-000000-000000-000000-000000.wav"))return 17;
  if (!matches_audio_index_override(true, kJukeboxIndexPath)) return 1;
  if (matches_audio_index_override(false, kJukeboxIndexPath)) return 2;
  for (const char *path : {"directory_listing.adb2",
       "newaudio/jukeboxmusic_park/directory_listing.adb2",
       "newaudio/lines/directory_listing.adb2", "loc/english.iff",
       "../newaudio/jukeboxmusic/directory_listing.adb2", "", "other.iff"}) {
    if (matches_audio_index_override(true, path)) return 3;
    if (valid_audio_index_override(path)) return 4;
  }
  if (valid_audio_index_override(nullptr)) return 5;
  if (!matches_audio_slot_override(20, "newaudio/jukeboxmusic/000020-000000-000000-000000-000000-000000.wav")) return 6;
  if (matches_audio_slot_override(20, "newaudio/jukeboxmusic/000021-000000-000000-000000-000000-000000.wav")) return 7;
  if (matches_audio_slot_override(0, "newaudio/jukeboxmusic/000001-000000-000000-000000-000000-000000.wav")) return 8;
  for (const char *path : {"000001-000000-000000-000000-000000-000000.wav",
       "newaudio/jukeboxmusic_park/000001-000000-000000-000000-000000-000000.wav",
       "newaudio/jukeboxmusic/000000-000000-000000-000000-000000-000000.wav",
       "newaudio/jukeboxmusic/000001-000001-000000-000000-000000-000000.wav"}) {
    if (matches_audio_slot_override(71,path)) return 9;
  }
  std::puts("PASS: audio index override requires opt-in and the exact full jukebox path.");
  return 0;
}
