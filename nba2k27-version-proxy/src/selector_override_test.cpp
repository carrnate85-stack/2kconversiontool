#include "selector_override.hpp"
#include <cassert>
int main() {
  using nba2k27::selector_override_path;
  assert(selector_override_path(true, "char/signatureconfigs_all.iff"));
  assert(selector_override_path(true, "char/fpplayer_global.iff"));
  assert(std::strcmp(selector_override_path(true, "fpplayer_global.iff"), "char/fpplayer_global.iff") == 0);
  assert(!selector_override_path(false, "char/fpplayer_global.iff"));
  assert(!selector_override_path(true, "other/fpplayer_global.iff"));
  assert(selector_override_path(true, "char/signatureconfigs_editplayer.iff"));
  assert(std::strcmp(selector_override_path(true, "signatureconfigs_all.iff"), "char/signatureconfigs_all.iff") == 0);
  assert(!selector_override_path(false, "char/signatureconfigs_all.iff"));
  assert(!selector_override_path(true, "other/signatureconfigs_all.iff"));
  assert(!selector_override_path(true, "char/png4109.iff"));
  assert(!selector_override_path(true, nullptr));
}
