#include "session_diagnostics.hpp"
#include <cassert>
#include <cstdio>

int main() {
  wchar_t temporary[MAX_PATH]{};
  assert(GetTempPathW(MAX_PATH,temporary));
  const std::wstring base=std::wstring(temporary)+L"nba2k27-session-path-"
    +std::to_wstring(GetCurrentProcessId())+L"-"+std::to_wstring(GetTickCount64());
  const std::wstring appdata=base+L"\\AppData";
  const std::wstring game=base+L"\\Game";
  const std::wstring organized=game+L"\\2k Launcher";
  assert(CreateDirectoryW(base.c_str(),nullptr));
  assert(CreateDirectoryW(appdata.c_str(),nullptr));
  assert(CreateDirectoryW(game.c_str(),nullptr));
  assert(CreateDirectoryW(organized.c_str(),nullptr));
  assert(CreateDirectoryW((organized+L"\\Data").c_str(),nullptr));
  const std::wstring pointer=appdata+L"\\nba2k27-mod-launcher-location.ini";
  assert(WritePrivateProfileStringW(L"Launcher",L"Root",organized.c_str(),pointer.c_str()));
  const std::wstring exe=game+L"\\NBA2K27.exe";
  assert(session_diagnostics_directory_for(exe,appdata)==organized+L"\\Logs\\game-session\\");
  assert(session_plain_directory(organized+L"\\Logs\\game-session"));
  assert(WritePrivateProfileStringW(L"Launcher",L"Root",L"C:\\Wrong Game\\2k Launcher",pointer.c_str()));
  assert(session_diagnostics_directory_for(exe,appdata)==appdata+L"\\nba2k27-mod-launcher\\diagnostics\\game-session\\");
  assert(session_plain_directory(appdata+L"\\nba2k27-mod-launcher\\diagnostics\\game-session"));
  const std::wstring current=appdata+L"\\2k Launcher";
  assert(CreateDirectoryW(current.c_str(),nullptr));
  assert(CreateDirectoryW((current+L"\\Data").c_str(),nullptr));
  HANDLE settings=CreateFileW((current+L"\\Data\\settings.json").c_str(),GENERIC_WRITE,0,nullptr,CREATE_NEW,FILE_ATTRIBUTE_NORMAL,nullptr);
  assert(settings!=INVALID_HANDLE_VALUE);
  assert(CloseHandle(settings));
  assert(session_diagnostics_directory_for(exe,appdata)==current+L"\\Logs\\game-session\\");
  const auto portable=base+L"\\Portable";
  assert(CreateDirectoryW(portable.c_str(),nullptr));
  assert(CreateDirectoryW((portable+L"\\App").c_str(),nullptr));
  assert(CreateDirectoryW((portable+L"\\Data").c_str(),nullptr));
  HANDLE portable_settings=CreateFileW((portable+L"\\Data\\settings.json").c_str(),GENERIC_WRITE,0,nullptr,CREATE_NEW,FILE_ATTRIBUTE_NORMAL,nullptr);
  assert(portable_settings!=INVALID_HANDLE_VALUE);assert(CloseHandle(portable_settings));
  const auto expected=portable+L"\\Logs\\game-session";
  std::string encoded="PORTABLE_LOG_DIR_HEX=";
  for(const wchar_t value:expected){char digits[5]{};std::snprintf(digits,sizeof(digits),"%04X",static_cast<unsigned>(value));encoded+=digits;}
  encoded+="\r\n";
  const auto config=game+L"\\nba2k27_provider_audit.cfg";
  HANDLE file=CreateFileW(config.c_str(),GENERIC_WRITE,0,nullptr,CREATE_NEW,FILE_ATTRIBUTE_NORMAL,nullptr);
  assert(file!=INVALID_HANDLE_VALUE);DWORD written=0;
  assert(WriteFile(file,encoded.data(),static_cast<DWORD>(encoded.size()),&written,nullptr)&&written==encoded.size());assert(CloseHandle(file));
  bool configured=false;
  assert(session_portable_directory_for(exe,&configured)==expected+L"\\"&&configured);
  assert(session_plain_directory(expected));
  return 0;
}
