#pragma once
#include <Windows.h>
#include <cwchar>
#include <string>

inline bool session_plain_directory(const std::wstring& path) {
  const DWORD attributes=GetFileAttributesW(path.c_str());
  return attributes!=INVALID_FILE_ATTRIBUTES && (attributes&FILE_ATTRIBUTE_DIRECTORY)!=0
    && (attributes&FILE_ATTRIBUTE_REPARSE_POINT)==0;
}

inline bool session_plain_file(const std::wstring& path) {
  const DWORD attributes=GetFileAttributesW(path.c_str());
  return attributes!=INVALID_FILE_ATTRIBUTES && (attributes&FILE_ATTRIBUTE_DIRECTORY)==0
    && (attributes&FILE_ATTRIBUTE_REPARSE_POINT)==0;
}

inline bool session_make_plain_directory(const std::wstring& path) {
  if(session_plain_directory(path))return true;
  if(GetFileAttributesW(path.c_str())!=INVALID_FILE_ATTRIBUTES)return false;
  return CreateDirectoryW(path.c_str(),nullptr)!=0 && session_plain_directory(path);
}

inline std::wstring session_trim_trailing_separator(std::wstring value) {
  while(value.size()>3 && (value.back()==L'\\'||value.back()==L'/'))value.pop_back();
  return value;
}

inline int session_hex(char value) {
  if(value>='0'&&value<='9')return value-'0';
  if(value>='a'&&value<='f')return value-'a'+10;
  if(value>='A'&&value<='F')return value-'A'+10;
  return -1;
}

// The launcher writes this path into its short-lived, game-side session
// configuration. A portable build does not need an AppData pointer at all.
// Presence of an invalid key fails closed instead of falling back elsewhere.
inline std::wstring session_portable_directory_for(const std::wstring& executable,bool* configured) {
  *configured=false;
  const auto slash=executable.find_last_of(L"\\/");
  if(slash==std::wstring::npos||_wcsicmp(executable.substr(slash+1).c_str(),L"NBA2K27.exe")!=0)return {};
  const auto config=executable.substr(0,slash)+L"\\nba2k27_provider_audit.cfg";
  const HANDLE file=CreateFileW(config.c_str(),GENERIC_READ,FILE_SHARE_READ|FILE_SHARE_WRITE|FILE_SHARE_DELETE,nullptr,OPEN_EXISTING,FILE_ATTRIBUTE_NORMAL,nullptr);
  if(file==INVALID_HANDLE_VALUE)return {};
  LARGE_INTEGER size{};
  char bytes[2048]{};DWORD read=0;
  const bool valid=GetFileSizeEx(file,&size)&&size.QuadPart>0&&size.QuadPart<static_cast<LONGLONG>(sizeof(bytes))
    &&ReadFile(file,bytes,static_cast<DWORD>(size.QuadPart),&read,nullptr)&&read==size.QuadPart;
  CloseHandle(file);
  if(!valid)return {};
  const std::string text(bytes,read),key="PORTABLE_LOG_DIR_HEX=";
  const auto found=text.find(key);
  if(found==std::string::npos)return {};
  *configured=true;
  if(found!=0&&text[found-1]!='\n')return {};
  const auto start=found+key.size(),end=text.find_first_of("\r\n",start);
  const auto hex=text.substr(start,end==std::string::npos?std::string::npos:end-start);
  if(hex.empty()||hex.size()>1600||hex.size()%4!=0)return {};
  std::wstring directory;
  for(std::size_t index=0;index<hex.size();index+=4){
    int value=0;
    for(std::size_t digit=0;digit<4;digit++){
      const int nibble=session_hex(hex[index+digit]);if(nibble<0)return {};
      value=(value<<4)|nibble;
    }
    if(value==0||value<32)return {};
    directory.push_back(static_cast<wchar_t>(value));
  }
  directory=session_trim_trailing_separator(directory);
  const std::wstring suffix=L"\\Logs\\game-session";
  if(directory.size()<=suffix.size()||_wcsicmp(directory.substr(directory.size()-suffix.size()).c_str(),suffix.c_str())!=0)return {};
  const auto root=directory.substr(0,directory.size()-suffix.size());
  if(root.size()<3||root[1]!=L':'||root[2]!=L'\\'||!session_plain_directory(root)
     ||!session_plain_directory(root+L"\\App")||!session_plain_directory(root+L"\\Data")
     ||!session_plain_file(root+L"\\Data\\settings.json"))return {};
  wchar_t canonical[32768]{};
  const DWORD length=GetFullPathNameW(directory.c_str(),32768,canonical,nullptr);
  if(!length||length>=32768||_wcsicmp(canonical,directory.c_str())!=0)return {};
  if(!session_make_plain_directory(root+L"\\Logs")||!session_make_plain_directory(directory))return {};
  return directory+L"\\";
}

// The pointer is a hint, not authority: it must name this exact game's data
// directory. This keeps the proxy and launcher on the same diagnostics path.
inline std::wstring session_diagnostics_directory_for(const std::wstring& executable,
                                                      const std::wstring& appdata) {
  if(appdata.empty())return {};
  const std::wstring current=appdata+L"\\2k Launcher";
  if(session_plain_directory(current) && session_plain_directory(current+L"\\Data")
     && session_plain_file(current+L"\\Data\\settings.json")) {
    const std::wstring logs=current+L"\\Logs";
    const std::wstring game_session=logs+L"\\game-session";
    if(session_make_plain_directory(logs) && session_make_plain_directory(game_session))return game_session+L"\\";
    return {};
  }
  const auto slash=executable.find_last_of(L"\\/");
  if(slash!=std::wstring::npos && _wcsicmp(executable.substr(slash+1).c_str(),L"NBA2K27.exe")==0) {
    const std::wstring root=executable.substr(0,slash)+L"\\2k Launcher";
    const std::wstring pointer=appdata+L"\\nba2k27-mod-launcher-location.ini";
    wchar_t configured[32768]{};
    const DWORD length=GetPrivateProfileStringW(L"Launcher",L"Root",L"",configured,32768,pointer.c_str());
    if(length && length<32767 && _wcsicmp(session_trim_trailing_separator(configured).c_str(),root.c_str())==0
       && session_plain_directory(root) && session_plain_directory(root+L"\\Data")) {
      const std::wstring logs=root+L"\\Logs";
      const std::wstring game_session=logs+L"\\game-session";
      if(session_make_plain_directory(logs) && session_make_plain_directory(game_session))return game_session+L"\\";
      return {};
    }
  }
  // Unmigrated installs still use the original AppData location.
  std::wstring legacy=appdata;
  for(const auto part:{L"\\nba2k27-mod-launcher",L"\\diagnostics",L"\\game-session"}) {
    legacy+=part;
    if(!session_make_plain_directory(legacy))return {};
  }
  return legacy+L"\\";
}

inline std::wstring session_diagnostics_directory() {
  wchar_t executable[32768]{};
  const DWORD executable_length=GetModuleFileNameW(nullptr,executable,32768);
  if(!executable_length || executable_length>=32768)return {};
  bool portable_configured=false;
  const auto portable=session_portable_directory_for(executable,&portable_configured);
  if(portable_configured)return portable;
  wchar_t appdata[32768]{};
  const DWORD appdata_length=GetEnvironmentVariableW(L"APPDATA",appdata,32768);
  if(!appdata_length || appdata_length>=32768)return {};
  return session_diagnostics_directory_for(executable,appdata);
}
