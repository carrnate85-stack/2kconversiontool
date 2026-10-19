#pragma once
#include <cstring>
namespace nba2k27 {
inline bool matches_highschool_digit_override(bool enabled,const char* path){
 return enabled&&path&&(std::strcmp(path,"gooey_scenes/scorebug_highschool.iff")==0||std::strcmp(path,"scorebug_highschool.iff")==0);
}
// Exact test-build allowlist only. Do not broaden this to all gooey assets.
inline bool matches_scoreboard_text_override(bool enabled,const char* path){
 if(!enabled||!path)return false;
 constexpr const char* names[]={"mycourtscrimmagescorebug.iff","myparkscorebugsimple.iff","proamspectatorscorebug.iff","scorebugfamilyflashbackaau.iff","scorebugfamilyflashbackhighschool.iff","scorebugfamilyflashbacknbaopenrun.iff","shutitdownscorebug.iff","thewscrimmagescorebug.iff","triplethreatonlinescorebug.iff"};
 const char* leaf=path;
 if(std::strncmp(path,"gooey/",6)==0)leaf=path+6;
 else if(std::strchr(path,'/')||std::strchr(path,'\\'))return false;
 for(const char* name:names)if(std::strcmp(leaf,name)==0)return true;
 return false;
}
}
