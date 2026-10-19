#pragma once
#include <cstring>

namespace nba2k27 {
inline bool valid_audio_mask(const char *mask, unsigned count) {
  if(mask==nullptr || std::strlen(mask)!=count) return false;
  for(unsigned i=0;i<count;++i) if(mask[i]!='0'&&mask[i]!='1') return false;
  return true;
}
inline unsigned audio_path_slot(const char *path, const char *prefix) {
  constexpr char suffix[]="-000000-000000-000000-000000-000000.wav";
  if(!path||!prefix)return 0;
  const auto length=std::strlen(prefix);
  if(std::strlen(path)!=length+6+sizeof(suffix)-1||std::strncmp(path,prefix,length)!=0)return 0;
  unsigned slot=0;
  for(unsigned i=0;i<6;++i){const char c=path[length+i];if(c<'0'||c>'9')return 0;slot=slot*10+static_cast<unsigned>(c-'0');}
  return std::strcmp(path+length+6,suffix)==0?slot:0;
}
inline bool audio_path_blocked(const char *main_mask,const char *park_mask,const char *path){
  if(!main_mask||main_mask[0]=='\0')return false;
  auto slot=audio_path_slot(path,"newaudio/jukeboxmusic/");
  if(slot)return slot>71||main_mask[slot-1]!='1';
  slot=audio_path_slot(path,"newaudio/jukeboxmusic_park/");
  return slot&&(slot>54||!park_mask||park_mask[slot-1]!='1');
}
inline bool audio_custom_path(const char *mask,const char *path){
  const auto slot=audio_path_slot(path,"newaudio/jukeboxmusic/");
  return mask&&mask[0]&&slot>=1&&slot<=71&&mask[slot-1]=='1';
}
// Opt-in, full-path-only exception. A leaf alias would also match the park,
// commentary and other banks' identically named directory listings.
inline constexpr char kJukeboxIndexPath[] =
    "newaudio/jukeboxmusic/directory_listing.adb2";
inline bool valid_audio_index_override(const char *config_path) {
  return config_path != nullptr &&
         std::strcmp(config_path, kJukeboxIndexPath) == 0;
}
inline bool matches_audio_index_override(bool enabled, const char *normalized_path) {
  return enabled && valid_audio_index_override(normalized_path);
}
inline bool matches_audio_slot_override(unsigned count, const char *path) {
  constexpr char prefix[] = "newaudio/jukeboxmusic/";
  constexpr char suffix[] = "-000000-000000-000000-000000-000000.wav";
  if (count == 0 || count > 71 || path == nullptr) return false;
  if (std::strlen(path) != sizeof(prefix)-1+6+sizeof(suffix)-1 ||
      std::strncmp(path, prefix, sizeof(prefix)-1) != 0) return false;
  const char *digits = path + sizeof(prefix)-1;
  unsigned slot = 0;
  for (unsigned i=0; i<6; ++i) {
    if (digits[i]<'0' || digits[i]>'9') return false;
    slot = slot*10 + static_cast<unsigned>(digits[i]-'0');
  }
  return slot>=1 && slot<=count && std::strcmp(digits+6,suffix)==0;
}
}  // namespace nba2k27
