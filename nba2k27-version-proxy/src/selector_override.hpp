#pragma once
#include <cstring>
namespace nba2k27 {
inline const char* selector_override_path(bool enabled, const char* path) {
  if (!enabled || !path) return nullptr;
  constexpr const char* paths[] = {"char/signatureconfigs_all.iff", "char/signatureconfigs_editplayer.iff", "char/fpplayer_global.iff"};
  for (const char* candidate : paths) {
    if (std::strcmp(path, candidate) == 0 || std::strcmp(path, candidate + 5) == 0) return candidate;
  }
  return nullptr;
}
}
