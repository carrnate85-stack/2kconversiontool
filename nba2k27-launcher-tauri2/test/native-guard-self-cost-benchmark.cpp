// QA073 self-only research. Never opens any other PID or runs a game worker.
#include "../native/outfit_match_v2_proxy_bridge.hpp"
#include <algorithm>
#include <filesystem>
#include <iomanip>
#include <iostream>
#include <stdexcept>
using namespace outfit_match_v2;
struct Stats {const char* name;unsigned count{},true_count{};std::vector<double> us;};
static double frequency() {LARGE_INTEGER f{};if(!QueryPerformanceFrequency(&f))throw std::runtime_error("QPC unavailable");return double(f.QuadPart);}
template<class F>static Stats measure(const char* name,unsigned count,F function) {
 const auto hz=frequency();Stats result{name,count,0,{}};result.us.reserve(count);
 for(unsigned n=0;n<3;++n)(void)function();
 for(unsigned n=0;n<count;++n){LARGE_INTEGER a{},b{};QueryPerformanceCounter(&a);const bool value=function();QueryPerformanceCounter(&b);result.true_count+=value;result.us.push_back(double(b.QuadPart-a.QuadPart)*1000000.0/hz);}
 return result;
}
static void emit(Stats& s,bool last=false) {
 std::sort(s.us.begin(),s.us.end());double total{};for(const auto n:s.us)total+=n;
 std::cout<<"    {\"name\":\""<<s.name<<"\",\"samples\":"<<s.count<<",\"trueResults\":"<<s.true_count
 <<",\"meanUs\":"<<total/s.count<<",\"p50Us\":"<<s.us[s.us.size()/2]<<",\"p95Us\":"<<s.us[(s.us.size()-1)*95/100]<<",\"minUs\":"<<s.us.front()<<",\"maxUs\":"<<s.us.back()<<"}"<<(last?"\n":",\n");
}
int main(int argc,char** argv) {
 try {
  if(argc!=2)throw std::runtime_error("Pass a fresh absolute artifacts-only fixture directory");
  const auto root=std::filesystem::absolute(std::filesystem::path(argv[1])).lexically_normal();
  const auto workspace=std::filesystem::current_path();
  const auto artifacts=workspace/L"artifacts";
  if(root.parent_path()!=artifacts || root.filename().wstring().find(L"qa073-self-guard-cost-")!=0 || std::filesystem::exists(root))throw std::runtime_error("Fresh fixed project-artifacts fixture required");
  const auto portable=root/L"Synthetic"/L"Portable";
  std::array<std::filesystem::path,4> leaves={portable/L"App",portable/L"Data",portable/L"Library",portable/L"Logs"/L"game-session"};
  for(const auto& path:leaves)std::filesystem::create_directories(path);
  std::array<PinnedDirectory,4> dirs;
  for(unsigned n=0;n<4;++n)if(!dirs[n].open(leaves[n].wstring()))throw std::runtime_error("Synthetic directory pin failed (including retained ancestry)");
  auto self=retain_current_process();FILETIME birth{},exit{},kernel{},user{};std::array<wchar_t,32768> buffer{};DWORD len=DWORD(buffer.size());
  if(!self || !GetProcessTimes(self.value,&birth,&exit,&kernel,&user) || !QueryFullProcessImageNameW(self.value,0,buffer.data(),&len))throw std::runtime_error("Self identity query failed");
  const std::wstring image(buffer.data(),len);const auto born=ticks(birth);const auto pid=GetCurrentProcessId();
  const auto lease_path=leaves[1]/L"synthetic-identity.lease";
  Handle lease(CreateFileW(lease_path.c_str(),GENERIC_READ|GENERIC_WRITE,FILE_SHARE_READ,nullptr,CREATE_NEW,FILE_FLAG_WRITE_THROUGH|FILE_FLAG_OPEN_REPARSE_POINT,nullptr));
  const std::vector<unsigned char> expected(1632,0x5a);DWORD written{};StableId lease_id{};
  if(!lease || !WriteFile(lease.value,expected.data(),DWORD(expected.size()),&written,nullptr) || written!=expected.size() || !FlushFileBuffers(lease.value) || !file_id(lease.value,false,lease_id))throw std::runtime_error("Synthetic lease setup failed");
  auto identity=[&]{return process_identity(self.value,born,image);};
  auto directories=[&]{for(const auto& dir:dirs)if(!dir.unchanged())return false;return true;};
  auto lease_read=[&]{StableId id{};std::vector<unsigned char> bytes;return file_id(lease.value,false,id)&&id==lease_id&&read_file(lease.value,1632,bytes)&&bytes==expected;};
  auto snapshot=[&]{return outfit_match_v2::bridge::protected_processes(pid,pid);};
  auto raw_snapshot=[&]{Handle h(CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS,0));if(!h)return false;PROCESSENTRY32W row{};row.dwSize=sizeof row;if(!Process32FirstW(h.value,&row))return false;while(Process32NextW(h.value,&row)){}return GetLastError()==ERROR_NO_MORE_FILES;};
  auto synthetic_owned=[&]{return identity()&&identity()&&directories()&&lease_read();};
  std::vector<Stats> all;
  all.push_back(measure("qpc_empty_baseline",500,[]{return true;}));
  all.push_back(measure("retained_self_process_identity",500,identity));
  all.push_back(measure("single_deep_directory_unchanged",250,[&]{return dirs[0].unchanged();}));
  all.push_back(measure("four_deep_directories_unchanged",250,directories));
  all.push_back(measure("synthetic_lease_id_read1632",250,lease_read));
  all.push_back(measure("full_toolhelp_process_enumeration",100,raw_snapshot));
  all.push_back(measure("production_protected_processes_names_only",100,snapshot));
  all.push_back(measure("synthetic_owned_callback_equivalent",100,synthetic_owned));
  // This executes only the components above against self and synthetic data.
  // It is NOT Authority.safe() and grants no TrustedBinding or worker access.
  all.push_back(measure("synthetic_current_safe_components",100,[&]{if(!identity()||!identity()||!synthetic_owned()||!synthetic_owned())return false;return !snapshot();}));
  all.push_back(measure("synthetic_combined_owned_safe_components",100,[&]{if(!identity()||!identity()||!synthetic_owned())return false;return !snapshot();}));
  std::cout<<std::fixed<<std::setprecision(3)<<"{\n  \"schema\":\"native-self-guard-cost-v1\",\n  \"selfPid\":"<<pid<<",\n  \"otherProcessHandlesOpened\":0,\n  \"gameCalls\":0,\n  \"gameWrites\":0,\n  \"actualDevDataAccess\":false,\n  \"limits\":\"Toolhelp reads process names only; protected boolean may short-circuit on an existing blocker. Synthetic safe components omit anti-cheat callback and real session events; no Authority or Session is opened.\",\n  \"measurements\":[\n";
  for(unsigned n=0;n<all.size();++n)emit(all[n],n+1==all.size());std::cout<<"  ]\n}\n";
  return 0;
 }catch(const std::exception& e){std::cerr<<e.what()<<"\n";return 1;}
}
