#pragma once
// Pure diagnostic formatting. No addresses are dereferenced or permissions inferred.
#include <cstdint>
#include <string>
#include <sstream>
namespace wnba_blacktop_roster {
struct ImageMetadata {
 enum class Kind {unknown,null,image,foreign};
 Kind kind{Kind::unknown};std::uintptr_t rva{};
 static ImageMetadata from(bool read,std::uintptr_t pointer,std::uintptr_t base) {
  if(!read)return {};
  if(!pointer)return {Kind::null,0};
  if(pointer>=base && pointer-base<0x35BDE000)return {Kind::image,pointer-base};
  return {Kind::foreign,0};
 }
};
struct Telemetry {
 bool retained{},context_read{},stable{},uid_read{};
 std::uint32_t current{},other{},stage{},active{},kind{};
 ImageMetadata installed,requested,playnow;
 std::uint64_t uid{};
};
inline std::string image_metadata(const ImageMetadata& value) {
 switch(value.kind) {
  case ImageMetadata::Kind::null:return "null";
  case ImageMetadata::Kind::foreign:return "foreign";
  case ImageMetadata::Kind::unknown:return "?";
  case ImageMetadata::Kind::image:{std::ostringstream out;out<<std::hex<<value.rva;return out.str();}
 }
 return "?";
}
inline std::string format_telemetry(const Telemetry& value) {
 if(!value.retained)return "ret=0 ctx=? d=?/?/? uid=?";
 std::ostringstream out;out<<"ret=1 ctx=";
 if(!value.context_read)out<<"?";
 else out<<(value.stable?"stable:":"unstable:")<<std::hex<<value.current<<"/"<<value.other
  <<std::dec<<",s="<<value.stage<<",a="<<value.active<<",k="<<value.kind;
 out<<" d="<<image_metadata(value.installed)<<"/"<<image_metadata(value.requested)<<"/"<<image_metadata(value.playnow)<<" uid=";
 if(value.uid_read)out<<value.uid;else out<<"?";
 return out.str();
}
} // namespace wnba_blacktop_roster
