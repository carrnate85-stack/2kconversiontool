#include "scoreboard_text_override.hpp"
#include <cassert>
#include <string>
int main(){
 const char* names[]={"mycourtscrimmagescorebug.iff","myparkscorebugsimple.iff","proamspectatorscorebug.iff","scorebugfamilyflashbackaau.iff","scorebugfamilyflashbackhighschool.iff","scorebugfamilyflashbacknbaopenrun.iff","shutitdownscorebug.iff","thewscrimmagescorebug.iff","triplethreatonlinescorebug.iff"};
 for(const auto name:names){
  assert(nba2k27::matches_scoreboard_text_override(true,name));
  assert(nba2k27::matches_scoreboard_text_override(true,(std::string("gooey/")+name).c_str()));
  assert(!nba2k27::matches_scoreboard_text_override(false,name));
  for(const char* prefix:{"../","other/","gooey/../","gooey/other/","gooey\\"})assert(!nba2k27::matches_scoreboard_text_override(true,(std::string(prefix)+name).c_str()));
 }
 for(const char* unrelated:{"loc/english.iff","music.iff","gooey/scorebug.iff","gooey/scorebughighschool.iff",""})assert(!nba2k27::matches_scoreboard_text_override(true,unrelated));
 assert(!nba2k27::matches_scoreboard_text_override(true,nullptr));
 assert(nba2k27::matches_highschool_digit_override(true,"gooey_scenes/scorebug_highschool.iff"));
 assert(nba2k27::matches_highschool_digit_override(true,"scorebug_highschool.iff"));
 for(const char* path:{"gooey/scorebug_highschool.iff","gooey_scenes/../scorebug_highschool.iff","gooey_scenes/scorebug_highschool.iff.bak","loc/english.iff","scorebughighschool.iff"})assert(!nba2k27::matches_highschool_digit_override(true,path));
 assert(!nba2k27::matches_highschool_digit_override(false,"scorebug_highschool.iff"));
 assert(!nba2k27::matches_highschool_digit_override(true,nullptr));
}
