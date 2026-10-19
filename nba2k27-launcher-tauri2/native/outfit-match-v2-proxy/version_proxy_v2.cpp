#if defined(NBA2K27_SESSION_OUTFITS)
#error The Version2 variant must never compile the old outfit worker
#endif
#define NBA2K27_SESSION_OUTFITS_V2 1
#include "outfit_match_v2_proxy_bridge.hpp"
#include "wnba_priority_v2_proxy_bridge.hpp"
#include "roster_appearance_v2_proxy_bridge.hpp"
#include <Windows.h>
#include <bcrypt.h>

#include <array>
#include <cstdarg>
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstring>

#include "provider_resolver.hpp"
#include "audio_index_override.hpp"
#include "selector_override.hpp"
bool g_selector_override = false;
#include "freestyle_audio.hpp"
#include "freestyle_build_profile_v2.hpp" // Rust-owned; adds the V2 tuple
#ifdef NBA2K27_STARTUP_MOVIE_TEST
#include "startup_movie_test.hpp"
#endif
#ifdef NBA2K27_COACH_CLONE_AUDIT
#include "coach_clone_audit.hpp"
#endif
#ifdef NBA2K27_SCOREBOARD_TEXT_TEST
#include "scoreboard_text_override.hpp"
bool g_scoreboard_text_test=false;
#ifdef NBA2K27_HIGHSCHOOL_DIGIT_TEST
bool g_highschool_digit_test=false;
#endif
#endif
#ifdef NBA2K27_SESSION_OUTFITS
#define NBA2K27_EXPAND_COPY
#define NBA2K27_SAME_OUTFIT_REBUILD
#define NBA2K27_FULL_OUTFIT_REBUILD
#define NBA2K27_SAVED_OUTFIT_PLAN
#include "character_description_clone_audit.hpp"
#include "character_description_clone_sep11.hpp"
#endif
#ifdef NBA2K27_CHARACTER_QUEUE_NOOP_TEST
#include "character_queue_noop.hpp"
#endif
#ifdef NBA2K27_DESCRIPTION_CLONE_AUDIT
#ifdef NBA2K27_DESCRIPTION_CLONE_SEP11
#include "character_description_clone_sep11.hpp"
#else
#include "character_description_clone_audit.hpp"
#endif
#endif

namespace {

constexpr wchar_t kLogName[] = L"nba2k27_version_proxy.log";
constexpr wchar_t kArmedName[] = L"nba2k27_version_proxy_armed.log";
constexpr wchar_t kTriggerName[] = L"nba2k27_version_proxy_audit.trigger";
constexpr wchar_t kTraceName[] = L"nba2k27_version_proxy_audit.trace";
constexpr wchar_t kProviderConfigName[] = L"nba2k27_provider_audit.cfg";
constexpr char kSchema[] = "nba2k27-version-proxy-session-missing-asset-route-v11.8";
constexpr std::uint32_t kExpectedStockDeviceId = 0xC7E442DCu;
constexpr std::uint32_t kAuditDeviceId = 0xC27D1610u;
constexpr std::uint32_t kSuccess = 0x504521A8u;
constexpr std::uint32_t kNotFound = 0xDF7F513Eu;
constexpr std::size_t kDeviceBytes = 0x48;
constexpr std::size_t kDefaultDeviceIdOffset = 0x1E0;
constexpr std::size_t kGetIdSlot = 20;
constexpr std::size_t kExistsSlot = 32;
constexpr std::size_t kFileInfoSlot = 33;
constexpr std::size_t kActualOpenSlot = 35;
constexpr std::size_t kCloseSlot = 42;
constexpr std::size_t kTransferSlot = 43;
constexpr std::size_t kFileHandleBytes = 0xB8;
constexpr std::size_t kNativeHandleOffset = 0x38;
constexpr char kExistsMiss[] = "char/sig/__c27d1610_missing__.iff";
constexpr std::uint32_t kProbeResourceMagic = 0x3032564Cu;
constexpr std::size_t kProviderPipeBytes = 192;
constexpr std::size_t kProviderPathBytes = 260;
constexpr std::uint64_t kMaxAuditAssetBytes = 64ull * 1024ull * 1024ull;
constexpr std::uint64_t kMaxDynamicAssetBytes =
    16ull * 1024ull * 1024ull * 1024ull;
constexpr std::uint32_t kMaxProviderReadBytes = 1024u * 1024u;
constexpr std::size_t kMaxHandleBindings = 256;
constexpr std::uint32_t kProviderRequestMagic = 0x31504232u;
constexpr std::uint32_t kProviderResponseMagic = 0x31524232u;
constexpr std::uint16_t kProviderProtocolVersion = 1;
constexpr std::uint16_t kProviderQueryOperation = 1;
constexpr std::uint16_t kProviderReadOperation = 2;

struct ProbeStatus {
  std::uint32_t value;
};

struct DeviceIdResult {
  std::uint32_t value;
};

#pragma pack(push, 1)
struct ProviderRequestHeader {
  std::uint32_t magic;
  std::uint16_t version;
  std::uint16_t operation;
  std::uint64_t offset;
  std::uint32_t length;
  std::uint32_t path_length;
};

struct ProviderResponseHeader {
  std::uint32_t magic;
  std::uint16_t version;
  std::uint16_t operation;
  std::uint32_t status;
  std::uint32_t reserved;
  std::uint64_t asset_size;
  std::uint32_t bytes_read;
  std::uint32_t reserved_two;
};
#pragma pack(pop)

using OpenResult = std::array<std::byte, 0x130>;

using ManagerGetter = void *(*)();
using ManagerRegister = ProbeStatus *(*)(void *, ProbeStatus *, void *);
using ManagerFind = void *(*)(void *, std::uint32_t);
using ReferenceRelease = bool (*)(void *);
using BaseConstructor = void *(*)(void *);
using SetDeviceId = ProbeStatus *(*)(void *, ProbeStatus *, std::uint32_t);
using BaseNonDeletingDestructor = void (*)(void *);
using ExistsCallback = bool (*)(void *, const char *);
using FileInfoCallback = ProbeStatus *(*)(void *, ProbeStatus *, OpenResult *,
                                          const char *);
using ActualOpenCallback = ProbeStatus *(*)(
    void *, ProbeStatus *, void *, const char *, std::uint32_t,
    std::uint32_t, std::uint32_t);
using TransferCallback = ProbeStatus *(*)(void *, ProbeStatus *, void *, void *,
                                          std::uint64_t, std::uint64_t);
using CloseCallback = ProbeStatus *(*)(void *, ProbeStatus *, void *);
using ManagerOpen = ProbeStatus *(*) (
    void *, ProbeStatus *, std::uint32_t, void *, const char *,
    std::uint32_t, std::uint32_t, std::uint32_t);
using FileHandleConstructor = void *(*)(void *);
using FileHandleInitialize = void (*)(void *, void *, std::uint64_t,
                                      std::uint64_t);
using FileHandleRead = ProbeStatus *(*) (
    void *, ProbeStatus *, void *, std::uint64_t, std::uint64_t,
    std::uint64_t *, std::uint32_t);
using FileHandleDestructor = void (*)(void *);

struct RegistrationAudit {
  bool attempted = false;
  bool manager_ready = false;
  bool runtime_identity_matches = false;
  bool audit_id_available = false;
  bool constructed = false;
  bool constructor_self_linked = false;
  bool id_set = false;
  bool private_vtable_verified = false;
  bool stock_exists_inherited = false;
  bool registered = false;
  bool register_called = false;
  bool find_during = false;
  bool find_during_released = false;
  bool destructor_called = false;
  bool self_linked_after = false;
  bool absent_after = false;
  bool default_unchanged = false;
  bool callbacks_quiesced = false;
  bool exists_hit = false;
  bool exists_miss_rejected = false;
  bool file_handle_constructed = false;
  bool provider_configured = false;
  bool payload_match = false;
  bool resource_released = false;
  bool passed = false;
  std::uint32_t default_before = 0;
  std::uint32_t default_after = 0;
  std::uint32_t set_id_status = 0xFFFFFFFFu;
  std::uint32_t registration_status = 0xFFFFFFFFu;
  std::uint32_t get_id_calls = 0;
  std::uint32_t file_info_calls = 0;
  std::uint32_t exists_file_info_calls = 0;
  std::uint32_t manager_open_status = 0xFFFFFFFFu;
  std::uint32_t actual_open_calls = 0;
  std::uint32_t actual_open_successes = 0;
  std::uint32_t read_status = 0xFFFFFFFFu;
  std::uint32_t transfer_calls = 0;
  std::uint32_t close_status = 0xFFFFFFFFu;
  std::uint32_t close_calls = 0;
  std::uint32_t resource_handles = 0;
  std::uint32_t provider_query_calls = 0;
  std::uint32_t provider_read_calls = 0;
  std::uint64_t bytes_read = 0;
  std::uint64_t provider_bytes = 0;
  std::uint64_t asset_size = 0;
  const char *failure = "not attempted";
};

struct ProbeResource {
  std::uint32_t magic;
  std::uint64_t size;
};

constexpr LONG kBindingFree = 0;
constexpr LONG kBindingReserved = 1;
constexpr LONG kBindingProvider = 2;
constexpr LONG kBindingStock = 3;
constexpr LONG kBindingClosing = 4;

struct HandleBinding {
  volatile LONG state = kBindingFree;
  void *file_handle = nullptr;
  ProbeResource provider_resource{kProbeResourceMagic, 0};
  std::array<char, kProviderPathBytes + 1> provider_path = {};
  volatile LONG target = 0;
  volatile LONG transferred = 0;
};

struct LiveRouteResult {
  bool started = false;
  bool session_active = false;
  bool target_read = false;
  bool target_completed = false;
  bool timed_out = false;
  bool default_restored = false;
  bool unregistered = false;
  bool callbacks_quiesced = false;
  bool handles_released = false;
  bool passed = false;
  std::uint32_t default_before = 0;
  std::uint32_t default_during = 0;
  std::uint32_t default_after = 0;
  std::uint32_t registration_status = 0xFFFFFFFFu;
  std::uint32_t file_info_calls = 0;
  std::uint32_t stock_file_info_successes = 0;
  std::uint32_t local_file_info_successes = 0;
  std::uint32_t actual_open_calls = 0;
  std::uint32_t stock_open_successes = 0;
  std::uint32_t local_open_successes = 0;
  std::uint32_t transfer_calls = 0;
  std::uint32_t close_calls = 0;
  std::uint32_t binding_allocations = 0;
  std::uint32_t binding_releases = 0;
  std::uint32_t binding_peak = 0;
  std::uint32_t binding_active = 0;
  std::uint32_t provider_query_calls = 0;
  std::uint32_t provider_read_calls = 0;
  std::uint64_t provider_bytes = 0;
  const char *failure = "not started";
};

static_assert(sizeof(OpenResult) == 0x130);
static_assert(sizeof(ProbeResource) == 0x10);
static_assert(sizeof(ProviderRequestHeader) == 24);
static_assert(sizeof(ProviderResponseHeader) == 32);

alignas(16) std::array<std::uintptr_t, nba2k27::kBaseVtableSlots>
    g_audit_vtable = {};
volatile LONG g_get_id_calls = 0;
volatile LONG g_file_info_calls = 0;
volatile LONG g_actual_open_calls = 0;
volatile LONG g_actual_open_successes = 0;
volatile LONG g_transfer_calls = 0;
volatile LONG g_close_calls = 0;
volatile LONG g_resource_handles = 0;
volatile LONG g_callbacks_in_flight = 0;
volatile LONG g_provider_query_calls = 0;
volatile LONG g_provider_read_calls = 0;
volatile LONGLONG g_provider_bytes = 0;
FileHandleInitialize g_file_handle_initialize = nullptr;
void *g_bound_file_handle = nullptr;
ProbeResource g_probe_resource{kProbeResourceMagic, 0};
alignas(16) std::array<std::byte, kFileHandleBytes> g_file_handle = {};
std::array<wchar_t, kProviderPipeBytes + 1> g_provider_pipe = {};
std::array<char, kProviderPathBytes + 1> g_provider_path = {};
std::array<char, kProviderPathBytes + 1> g_forced_override_path = {};
std::array<char, kProviderPathBytes + 1> g_forced_override_path2 = {};
std::array<char, kProviderPathBytes + 1> g_forced_override_path3 = {};
std::array<char, kProviderPathBytes + 1> g_forced_override_path4 = {};
bool g_audio_index_override = false;
unsigned g_audio_slot_count = 0;
std::array<char,72> g_audio_main_mask = {};
std::array<char,55> g_audio_park_mask = {};
std::array<char,72> g_audio_custom_mask = {};
bool g_freestyle_test = false;
bool g_freestyle_playlist = false;
std::array<char,72> g_freestyle_selection = {};
volatile LONGLONG g_freestyle_first_request = 0;
volatile LONG g_freestyle_opens = 0;
volatile LONG g_freestyle_failures = 0;
std::array<std::uint8_t, 32> g_expected_sha256 = {};
std::array<std::uint8_t, 32 + kMaxProviderReadBytes> g_provider_response = {};
std::uint64_t g_expected_asset_size = 0;
std::uint32_t g_last_close_status = kNotFound;
SRWLOCK g_provider_lock = SRWLOCK_INIT;
alignas(16) std::array<std::uintptr_t, nba2k27::kBaseVtableSlots>
    g_live_vtable = {};
alignas(16) std::array<std::byte, kDeviceBytes> g_live_device = {};
alignas(16) std::array<HandleBinding, kMaxHandleBindings> g_bindings = {};
const std::byte *g_live_module = nullptr;
std::uint32_t g_live_image_size = 0;
std::uint32_t g_live_timestamp = 0;
void *g_live_manager = nullptr;
void *g_stock_device = nullptr;
volatile LONG g_live_file_info_calls = 0;
volatile LONG g_live_stock_file_info_successes = 0;
volatile LONG g_live_local_file_info_successes = 0;
volatile LONG g_live_actual_open_calls = 0;
volatile LONG g_live_stock_open_successes = 0;
volatile LONG g_live_local_open_successes = 0;
volatile LONG g_live_transfer_calls = 0;
volatile LONG g_live_close_calls = 0;
volatile LONG g_binding_allocations = 0;
volatile LONG g_binding_releases = 0;
volatile LONG g_binding_active = 0;
volatile LONG g_binding_peak = 0;
volatile LONG g_target_read = 0;
volatile LONG g_target_completed = 0;
volatile LONGLONG g_last_local_activity = 0;

int hex_value(char value) {
  if (value >= '0' && value <= '9') return value - '0';
  if (value >= 'a' && value <= 'f') return value - 'a' + 10;
  if (value >= 'A' && value <= 'F') return value - 'A' + 10;
  return -1;
}

const char *config_value(char *buffer, std::size_t length, const char *key) {
  const std::size_t key_length = std::strlen(key);
  std::size_t offset = 0;
  while (offset < length) {
    while (offset < length && buffer[offset] == '\0') ++offset;
    if (offset >= length) break;
    char *line = buffer + offset;
    const std::size_t line_length = std::strlen(line);
    if (line_length > key_length &&
        std::strncmp(line, key, key_length) == 0) {
      return line + key_length;
    }
    offset += line_length + 1;
  }
  return nullptr;
}

bool safe_provider_path(const char *path, std::size_t length) {
  if (path == nullptr || length == 0 || length > kProviderPathBytes ||
      path[0] == '/' || path[0] == '\\') {
    return false;
  }
  std::size_t segment_start = 0;
  for (std::size_t index = 0; index <= length; ++index) {
    if (index != length && path[index] != '/' && path[index] != '\\') {
      const auto value = static_cast<unsigned char>(path[index]);
      if (value < 0x20 || value > 0x7E || path[index] == ':') return false;
      continue;
    }
    const std::size_t segment_length = index - segment_start;
    if (segment_length == 0 ||
        (segment_length == 1 && path[segment_start] == '.') ||
        (segment_length == 2 && path[segment_start] == '.' &&
         path[segment_start + 1] == '.')) {
      return false;
    }
    segment_start = index + 1;
  }
  return true;
}

bool load_provider_config(const wchar_t *path) {
  g_provider_pipe.fill(L'\0');
  g_provider_path.fill('\0');
  g_forced_override_path.fill('\0');
  g_forced_override_path2.fill('\0');
  g_forced_override_path3.fill('\0');
  g_forced_override_path4.fill('\0');
  g_selector_override = false;
#ifdef NBA2K27_SCOREBOARD_TEXT_TEST
  g_scoreboard_text_test=false;
#ifdef NBA2K27_HIGHSCHOOL_DIGIT_TEST
  g_highschool_digit_test=false;
#endif
#endif
  g_audio_index_override = false;
  g_audio_slot_count = 0;
  g_freestyle_test = false;
  g_freestyle_playlist = false;
  g_freestyle_selection.fill('\0');
  g_audio_main_mask.fill('\0');g_audio_park_mask.fill('\0');g_audio_custom_mask.fill('\0');
  g_expected_sha256.fill(0);
  g_expected_asset_size = 0;
  const HANDLE file = CreateFileW(path, GENERIC_READ,
                                  FILE_SHARE_READ | FILE_SHARE_WRITE |
                                      FILE_SHARE_DELETE,
                                  nullptr, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL,
                                  nullptr);
  if (file == INVALID_HANDLE_VALUE) return false;
  LARGE_INTEGER size = {};
  if (!GetFileSizeEx(file, &size) || size.QuadPart <= 0 ||
      size.QuadPart > 2047) {
    CloseHandle(file);
    return false;
  }
  std::array<char, 2048> buffer = {};
  DWORD read = 0;
  const bool read_ok = ReadFile(file, buffer.data(),
                                static_cast<DWORD>(size.QuadPart), &read,
                                nullptr) != FALSE &&
                       read == static_cast<DWORD>(size.QuadPart);
  CloseHandle(file);
  if (!read_ok) return false;
  const std::size_t length = static_cast<std::size_t>(read);
  for (std::size_t index = 0; index < length; ++index) {
    if (buffer[index] == '\r' || buffer[index] == '\n') buffer[index] = '\0';
  }

  const char *pipe = config_value(buffer.data(), length, "PIPE=");
  const char *asset_path = config_value(buffer.data(), length, "PATH=");
  const char *size_text = config_value(buffer.data(), length, "SIZE=");
  const char *sha_text = config_value(buffer.data(), length, "SHA256=");
  const char *override_path = config_value(buffer.data(), length, "OVERRIDE=");
  const char *override_path2 = config_value(buffer.data(), length, "OVERRIDE_2=");
  const char *override_path3 = config_value(buffer.data(), length, "OVERRIDE_3=");
  const char *override_path4 = config_value(buffer.data(), length, "OVERRIDE_4=");
  const char *selector_override = config_value(buffer.data(), length, "SELECTOR_OVERRIDE=");
  if (selector_override != nullptr) {
    if (std::strcmp(selector_override, "1") != 0) return false;
    g_selector_override = true;
  }
#ifdef NBA2K27_SCOREBOARD_TEXT_TEST
  const char *scoreboard_text = config_value(buffer.data(), length, "SCOREBOARD_TEXT_TEST=");
  if(scoreboard_text!=nullptr){if(std::strcmp(scoreboard_text,"1")!=0)return false;g_scoreboard_text_test=true;}
#ifdef NBA2K27_HIGHSCHOOL_DIGIT_TEST
  const char *highschool_digit=config_value(buffer.data(),length,"HIGHSCHOOL_DIGIT_TEST=");
  if(highschool_digit!=nullptr){if(std::strcmp(highschool_digit,"7")!=0)return false;g_highschool_digit_test=true;}
#endif
#endif
  const char *audio_index = config_value(buffer.data(), length, "AUDIO_INDEX_OVERRIDE=");
  const char *audio_slots = config_value(buffer.data(), length, "AUDIO_SLOT_COUNT=");
  const char *main_mask=config_value(buffer.data(),length,"AUDIO_ALLOWED_MAIN=");
  const char *park_mask=config_value(buffer.data(),length,"AUDIO_ALLOWED_PARK=");
  const char *custom_mask=config_value(buffer.data(),length,"AUDIO_CUSTOM_MAIN=");
  const char *freestyle=config_value(buffer.data(),length,"FREESTYLE_EVENT_TEST=");
  const char *freestyle_playlist=config_value(buffer.data(),length,"FREESTYLE_PLAYLIST=");
  const char *freestyle_selection=config_value(buffer.data(),length,"FREESTYLE_SELECTION=");
  if(main_mask||park_mask||custom_mask){
    if(!nba2k27::valid_audio_mask(main_mask,71)||!nba2k27::valid_audio_mask(park_mask,54)||!nba2k27::valid_audio_mask(custom_mask,71)||audio_index||!audio_slots||std::strcmp(audio_slots,"71")!=0)return false;
    for(unsigned i=0;i<71;++i)if(custom_mask[i]=='1'&&main_mask[i]!='1')return false;
    std::memcpy(g_audio_main_mask.data(),main_mask,71);std::memcpy(g_audio_park_mask.data(),park_mask,54);std::memcpy(g_audio_custom_mask.data(),custom_mask,71);
  }
  if(freestyle){
    if(std::strcmp(freestyle,"992")!=0||!custom_mask||!std::strchr(custom_mask,'1'))return false;
    g_freestyle_test=true;
  }
  if(freestyle_playlist){
    if(freestyle||std::strcmp(freestyle_playlist,"992")!=0||!custom_mask||!std::strchr(custom_mask,'1')||std::strcmp(custom_mask,main_mask)!=0)return false;
    if(!nba2k27::valid_audio_mask(freestyle_selection,71))return false;
    unsigned count=0;for(unsigned i=0;i<71;++i)if(freestyle_selection[i]=='1'){if(custom_mask[i]!='1')return false;++count;}
    if(count<1||count>58)return false;
    std::memcpy(g_freestyle_selection.data(),freestyle_selection,71);
    g_freestyle_playlist=true;
  }
  if(freestyle_selection&&!freestyle_playlist)return false;
  if (audio_slots != nullptr) {
    const auto digits = std::strlen(audio_slots);
    if (digits < 1 || digits > 2) return false;
    for (std::size_t i=0; i<digits; ++i) {
      if (audio_slots[i]<'0' || audio_slots[i]>'9') return false;
      g_audio_slot_count = g_audio_slot_count*10 + static_cast<unsigned>(audio_slots[i]-'0');
    }
    if (g_audio_slot_count < 1 || g_audio_slot_count > 71) return false;
    if (audio_index != nullptr) return false; // Do not combine the experiments.
  }
  if (audio_index != nullptr) {
    if (!nba2k27::valid_audio_index_override(audio_index)) return false;
    g_audio_index_override = true;
  }
  const char* outfit_v2_session=config_value(buffer.data(),length,"OUTFIT_V2_SESSION=");
  const char* outfit_v2_portable=config_value(buffer.data(),length,"PORTABLE_LOG_DIR_HEX=");
  if(!outfit_match_v2::bridge::configure(outfit_v2_session,outfit_v2_portable))return false;
  if (pipe == nullptr || asset_path == nullptr || size_text == nullptr ||
      sha_text == nullptr) {
    return false;
  }

  const std::size_t pipe_length = std::strlen(pipe);
  constexpr char kPipePrefix[] = "\\\\.\\pipe\\NBA2K27ModProvider-";
  if (pipe_length == 0 || pipe_length > kProviderPipeBytes ||
      std::strncmp(pipe, kPipePrefix, sizeof(kPipePrefix) - 1) != 0) {
    return false;
  }
  for (std::size_t index = 0; index < pipe_length; ++index) {
    const auto value = static_cast<unsigned char>(pipe[index]);
    if (value < 0x20 || value > 0x7E) return false;
    g_provider_pipe[index] = static_cast<wchar_t>(value);
  }

  const std::size_t path_length = std::strlen(asset_path);
  if (!safe_provider_path(asset_path, path_length)) return false;
  for (std::size_t index = 0; index < path_length; ++index) {
    g_provider_path[index] = asset_path[index] == '\\' ? '/' : asset_path[index];
  }

  if (override_path != nullptr) {
    const std::size_t override_length = std::strlen(override_path);
    if (!safe_provider_path(override_path, override_length)) return false;
    for (std::size_t index = 0; index < override_length; ++index) {
      g_forced_override_path[index] =
          override_path[index] == '\\' ? '/' : override_path[index];
    }
  }
  if (override_path2 != nullptr) {
    if (override_path == nullptr) return false;
    const std::size_t override_length = std::strlen(override_path2);
    if (!safe_provider_path(override_path2, override_length)) return false;
    for (std::size_t index = 0; index < override_length; ++index) {
      g_forced_override_path2[index] =
          override_path2[index] == '\\' ? '/' : override_path2[index];
    }
    if (g_forced_override_path2 == g_forced_override_path) return false;
  }
  if (override_path3 != nullptr) {
    if (override_path2 == nullptr) return false;
    const std::size_t override_length = std::strlen(override_path3);
    if (!safe_provider_path(override_path3, override_length)) return false;
    for (std::size_t index = 0; index < override_length; ++index) {
      g_forced_override_path3[index] =
          override_path3[index] == '\\' ? '/' : override_path3[index];
    }
    if (g_forced_override_path3 == g_forced_override_path ||
        g_forced_override_path3 == g_forced_override_path2) return false;
  }

  if (override_path4 != nullptr) {
    if (override_path3 == nullptr) return false;
    const std::size_t override_length = std::strlen(override_path4);
    if (!safe_provider_path(override_path4, override_length)) return false;
    for (std::size_t index = 0; index < override_length; ++index) {
      g_forced_override_path4[index] = override_path4[index] == '\\' ? '/' : override_path4[index];
    }
    if (g_forced_override_path4 == g_forced_override_path ||
        g_forced_override_path4 == g_forced_override_path2 ||
        g_forced_override_path4 == g_forced_override_path3) return false;
  }
  std::uint64_t parsed_size = 0;
  const std::size_t size_length = std::strlen(size_text);
  if (size_length == 0 || size_length > 20) return false;
  for (std::size_t index = 0; index < size_length; ++index) {
    if (size_text[index] < '0' || size_text[index] > '9') return false;
    const std::uint64_t digit = static_cast<std::uint64_t>(size_text[index] - '0');
    if (parsed_size > (kMaxAuditAssetBytes - digit) / 10) return false;
    parsed_size = parsed_size * 10 + digit;
  }
  if (parsed_size == 0 || parsed_size > kMaxAuditAssetBytes) return false;
  g_expected_asset_size = parsed_size;

  if (std::strlen(sha_text) != g_expected_sha256.size() * 2) return false;
  for (std::size_t index = 0; index < g_expected_sha256.size(); ++index) {
    const int high = hex_value(sha_text[index * 2]);
    const int low = hex_value(sha_text[index * 2 + 1]);
    if (high < 0 || low < 0) return false;
    g_expected_sha256[index] = static_cast<std::uint8_t>((high << 4) | low);
  }
  return true;
}

bool write_all(HANDLE handle, const void *source, std::uint32_t length) {
  const auto *bytes = static_cast<const std::uint8_t *>(source);
  std::uint32_t completed = 0;
  while (completed < length) {
    DWORD written = 0;
    if (!WriteFile(handle, bytes + completed, length - completed, &written,
                   nullptr) || written == 0) {
      return false;
    }
    completed += written;
  }
  return true;
}

bool read_exact(HANDLE handle, void *destination, std::uint32_t length) {
  auto *bytes = static_cast<std::uint8_t *>(destination);
  std::uint32_t completed = 0;
  while (completed < length) {
    DWORD read = 0;
    if (!ReadFile(handle, bytes + completed, length - completed, &read,
                  nullptr) || read == 0) {
      return false;
    }
    completed += read;
  }
  return true;
}

bool open_provider_pipe(HANDLE *pipe) {
  *pipe = CreateFileW(g_provider_pipe.data(), GENERIC_READ | GENERIC_WRITE, 0,
                      nullptr, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, nullptr);
  if (*pipe != INVALID_HANDLE_VALUE) return true;
  if (GetLastError() != ERROR_PIPE_BUSY ||
      !WaitNamedPipeW(g_provider_pipe.data(), 1000)) {
    return false;
  }
  *pipe = CreateFileW(g_provider_pipe.data(), GENERIC_READ | GENERIC_WRITE, 0,
                      nullptr, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, nullptr);
  return *pipe != INVALID_HANDLE_VALUE;
}

struct ProviderLockGuard {
  ProviderLockGuard() { AcquireSRWLockExclusive(&g_provider_lock); }
  ~ProviderLockGuard() { ReleaseSRWLockExclusive(&g_provider_lock); }
  ProviderLockGuard(const ProviderLockGuard &) = delete;
  ProviderLockGuard &operator=(const ProviderLockGuard &) = delete;
};

bool provider_request(const char *provider_path,
                      std::uint64_t expected_asset_size,
                      std::uint16_t operation, std::uint64_t offset,
                      std::uint32_t length, std::uint8_t *destination,
                      std::uint64_t *asset_size) {
  if (provider_path == nullptr) return false;
  const std::size_t path_length = std::strlen(provider_path);
  if (path_length == 0 || path_length > kProviderPathBytes ||
      (operation != kProviderQueryOperation &&
       operation != kProviderReadOperation) ||
      length > kMaxProviderReadBytes) {
    return false;
  }
  const ProviderRequestHeader header = {
      kProviderRequestMagic, kProviderProtocolVersion, operation, offset,
      length, static_cast<std::uint32_t>(path_length)};
  std::array<std::uint8_t, 4 + sizeof(header) + kProviderPathBytes> frame = {};
  const std::uint32_t body_size =
      static_cast<std::uint32_t>(sizeof(header) + path_length);
  std::memcpy(frame.data(), &body_size, sizeof(body_size));
  std::memcpy(frame.data() + 4, &header, sizeof(header));
  std::memcpy(frame.data() + 4 + sizeof(header), provider_path,
              path_length);

  ProviderLockGuard provider_lock;
  HANDLE pipe = INVALID_HANDLE_VALUE;
  if (!open_provider_pipe(&pipe)) return false;
  outfit_match_v2::bridge::record_server(pipe);
  bool success = write_all(pipe, frame.data(), 4 + body_size);
  std::uint32_t response_size = 0;
  if (success) success = read_exact(pipe, &response_size, sizeof(response_size));
  if (success &&
      (response_size < sizeof(ProviderResponseHeader) ||
       response_size > g_provider_response.size())) {
    success = false;
  }
  if (success) {
    success = read_exact(pipe, g_provider_response.data(), response_size);
  }
  CloseHandle(pipe);
  if (!success) return false;

  const auto *response = reinterpret_cast<const ProviderResponseHeader *>(
      g_provider_response.data());
  if (response->magic != kProviderResponseMagic ||
      response->version != kProviderProtocolVersion ||
      response->operation != operation || response->status != 0 ||
      response->reserved != 0 || response->reserved_two != 0 ||
      response->asset_size == 0 ||
      response->asset_size > kMaxDynamicAssetBytes ||
      (expected_asset_size != 0 &&
       response->asset_size != expected_asset_size) ||
      response_size != sizeof(ProviderResponseHeader) + response->bytes_read) {
    return false;
  }
  if (asset_size != nullptr) *asset_size = response->asset_size;
  if (operation == kProviderQueryOperation) {
    if (offset != 0 || length != 0 || response->bytes_read != 0) return false;
    InterlockedIncrement(&g_provider_query_calls);
    return true;
  }
  if (response->bytes_read != length || destination == nullptr) return false;
  std::memcpy(destination,
              g_provider_response.data() + sizeof(ProviderResponseHeader),
              response->bytes_read);
  InterlockedIncrement(&g_provider_read_calls);
  InterlockedExchangeAdd64(&g_provider_bytes,
                           static_cast<LONGLONG>(response->bytes_read));
  return true;
}

bool provider_read_range(std::uint8_t *destination, std::uint64_t offset,
                         std::uint64_t length) {
  if (destination == nullptr || offset > g_expected_asset_size ||
      length > g_expected_asset_size - offset) {
    return false;
  }
  std::uint64_t completed = 0;
  while (completed < length) {
    const std::uint64_t remaining = length - completed;
    const auto chunk = static_cast<std::uint32_t>(
        remaining > kMaxProviderReadBytes ? kMaxProviderReadBytes : remaining);
    std::uint64_t reported_size = 0;
    if (!provider_request(g_provider_path.data(), g_expected_asset_size,
                          kProviderReadOperation, offset + completed, chunk,
                          destination + static_cast<std::size_t>(completed),
                          &reported_size) ||
        reported_size != g_expected_asset_size) {
      return false;
    }
    completed += chunk;
  }
  return true;
}

bool prepare_dynamic_provider_path(
    const char *path,
    std::array<char, kProviderPathBytes + 1> *provider_path) {
  if (provider_path == nullptr) return false;
  provider_path->fill('\0');
  if (path == nullptr) return false;
  std::size_t length = 0;
  while (length <= kProviderPathBytes && path[length] != '\0') ++length;
  if (length > kProviderPathBytes) return false;
  if (!safe_provider_path(path, length)) return false;
  for (std::size_t index = 0; index < length; ++index) {
    (*provider_path)[index] = path[index] == '\\' ? '/' : path[index];
  }
  return true;
}

const char *path_leaf(const char *path) {
  if (path == nullptr) return nullptr;
  const char *leaf = path;
  for (const char *cursor = path; *cursor != '\0'; ++cursor) {
    if (*cursor == '/' || *cursor == '\\') leaf = cursor + 1;
  }
  return leaf;
}

bool query_dynamic_provider(
    const char *requested_path,
    std::array<char, kProviderPathBytes + 1> *provider_path,
    std::uint64_t *asset_size, bool *is_target) {
  if (provider_path == nullptr || asset_size == nullptr ||
      is_target == nullptr) {
    return false;
  }
  *asset_size = 0;
  *is_target = false;
  if (!prepare_dynamic_provider_path(requested_path, provider_path)) {
    return false;
  }
  if (const char* selector = nba2k27::selector_override_path(g_selector_override, provider_path->data())) {
    std::strcpy(provider_path->data(), selector);
  }
  if (std::strchr(requested_path, '/') == nullptr &&
      std::strchr(requested_path, '\\') == nullptr &&
      std::strcmp(requested_path, path_leaf(g_provider_path.data())) == 0) {
    *provider_path = g_provider_path;
  }
  *is_target = std::strcmp(provider_path->data(),
                           g_provider_path.data()) == 0;
  return provider_request(provider_path->data(), 0,
                          kProviderQueryOperation, 0, 0, nullptr,
                          asset_size) &&
         *asset_size > 0 && *asset_size <= kMaxDynamicAssetBytes;
}

bool playlist_path_policy(const char *path, bool custom) {
  std::array<char,kProviderPathBytes+1> normalized={};
  if(!path||!prepare_dynamic_provider_path(path,&normalized))return false;
  return custom?nba2k27::audio_custom_path(g_audio_custom_mask.data(),normalized.data()):nba2k27::audio_path_blocked(g_audio_main_mask.data(),g_audio_park_mask.data(),normalized.data());
}
bool make_sibling_path(const wchar_t*,const wchar_t*,wchar_t*,std::size_t);
void trace_checkpoint(const wchar_t*,const char*);

bool freestyle_redirect(const char* path,std::array<char,kProviderPathBytes+1>* target){
  if((!g_freestyle_test&&!g_freestyle_playlist)||!g_live_module||!target)return false;
  const auto* profile=nba2k27::freestyle_build_profile(g_live_timestamp,g_live_image_size);
  if(!profile)return false;
  std::array<char,kProviderPathBytes+1> normalized={};
  if(!prepare_dynamic_provider_path(path,&normalized)||
     !nba2k27::audio_path_slot(normalized.data(),"newaudio/eventmusic/"))return false;
  // Reads only: never call a game getter, change a selector or patch instructions.
  std::array<unsigned char,7> live_anchor={};SIZE_T done=0;
  if(!ReadProcessMemory(GetCurrentProcess(),g_live_module+profile->anchor,live_anchor.data(),live_anchor.size(),&done)||
     done!=live_anchor.size()||live_anchor!=profile->bytes)return false;
  unsigned char event[0x150]={};
  if(!ReadProcessMemory(GetCurrentProcess(),g_live_module+profile->event,event,sizeof(event),&done)||done!=sizeof(event))return false;
  std::uint32_t index=0,bank=0,key=0;
  std::memcpy(&index,event+0x40,4);std::memcpy(&bank,event+0x14c,4);
  if(!ReadProcessMemory(GetCurrentProcess(),g_live_module+profile->keys+992*4,&key,4,&done)||done!=4||key!=0xFAB3211A)return false;
  // +0x40 is set before opening audio; +0x10 only becomes active AFTER open.
  const auto slot=nba2k27::freestyle_slot(normalized.data(),g_freestyle_playlist?g_freestyle_selection.data():g_audio_custom_mask.data(),event[0x44]==1,index,bank);
  if(!slot)return false;
  const auto now=static_cast<LONGLONG>(GetTickCount64());
  InterlockedCompareExchange64(&g_freestyle_first_request,now,0);
  const auto first=InterlockedCompareExchange64(&g_freestyle_first_request,0,0);
  if(g_freestyle_test&&(now<first||now-first>=600000))return false; // Test only; saved playlist lasts the session.
  return nba2k27::freestyle_target(slot,target->data(),target->size());
}
void trace_freestyle(const char* requested,const char* target,bool success){
  wchar_t executable[MAX_PATH]={},trace[MAX_PATH]={};
  if(!GetModuleFileNameW(nullptr,executable,MAX_PATH)||
     !make_sibling_path(executable,kTraceName,trace,MAX_PATH))return;
  char message[640]={};
  std::snprintf(message,sizeof(message),"FREESTYLE_REDIRECT_%s source=%s target=%s",
                success?"OPEN":"FAILED",requested,target);
  trace_checkpoint(trace,message);
}
// 0.52 WNBA roster diagnostics. Bounded lines in the existing audit trace:
// the exact spelling of any device file-info/open path naming a roster, and
// every forced-override list match (exact or case-insensitive).
volatile LONG g_roster_path_lines = 0;
volatile LONG g_override_match_lines = 0;
constexpr LONG kMaxRosterPathLines = 64;
constexpr LONG kMaxOverrideMatchLines = 32;
char ascii_lower(char c) { return (c >= 'A' && c <= 'Z') ? static_cast<char>(c - 'A' + 'a') : c; }
bool ascii_iequal(const char *a, const char *b) {
  if (a == nullptr || b == nullptr) return false;
  for (; *a != '\0' && *b != '\0'; ++a, ++b) {
    if (ascii_lower(*a) != ascii_lower(*b)) return false;
  }
  return *a == *b;
}
bool ascii_icontains(const char *text, const char *needle) {
  if (text == nullptr || needle == nullptr) return false;
  for (const char *start = text; *start != '\0'; ++start) {
    const char *t = start, *n = needle;
    while (*t != '\0' && *n != '\0' && ascii_lower(*t) == ascii_lower(*n)) { ++t; ++n; }
    if (*n == '\0') return true;
  }
  return false;
}
void trace_bounded(volatile LONG *counter, LONG limit, const char *message) {
  if (InterlockedIncrement(counter) > limit) return;
  wchar_t executable[MAX_PATH] = {}, trace[MAX_PATH] = {};
  if (!GetModuleFileNameW(nullptr, executable, MAX_PATH) ||
      !make_sibling_path(executable, kTraceName, trace, MAX_PATH)) return;
  trace_checkpoint(trace, message);
}
void trace_roster_path(const char *via, const char *path, bool forced, bool provider, bool success) {
  if (path == nullptr || (!ascii_icontains(path, "roster") && !ascii_icontains(path, "learn2k"))) return;
  char message[232] = {};
  std::snprintf(message, sizeof(message),
                "ROSTER_PATH via=%s device=0x%08X forced=%d provider=%d success=%d path=%.150s",
                via, kAuditDeviceId, forced ? 1 : 0, provider ? 1 : 0, success ? 1 : 0, path);
  trace_bounded(&g_roster_path_lines, kMaxRosterPathLines, message);
}
void trace_override_match(const char *kind, const char *requested, const char *configured) {
  char message[232] = {};
  std::snprintf(message, sizeof(message), "FORCED_OVERRIDE_MATCH kind=%s requested=%.90s configured=%.90s",
                kind, requested, configured);
  trace_bounded(&g_override_match_lines, kMaxOverrideMatchLines, message);
}
bool is_forced_override(const char *requested_path) {
  if (requested_path == nullptr) {
    return false;
  }
  std::array<char, kProviderPathBytes + 1> normalized = {};
  if (!prepare_dynamic_provider_path(requested_path, &normalized)) return false;
  if (nba2k27::selector_override_path(g_selector_override, normalized.data())) return true;
#ifdef NBA2K27_SCOREBOARD_TEXT_TEST
  if(nba2k27::matches_scoreboard_text_override(g_scoreboard_text_test,normalized.data()))return true;
#ifdef NBA2K27_HIGHSCHOOL_DIGIT_TEST
  if(nba2k27::matches_highschool_digit_override(g_highschool_digit_test,normalized.data()))return true;
#endif
#endif
  if (nba2k27::matches_audio_slot_override(g_audio_slot_count, normalized.data())) return true;
  if (nba2k27::matches_audio_index_override(g_audio_index_override, normalized.data())) return true;
  const bool leaf_only = std::strchr(requested_path, '/') == nullptr &&
                         std::strchr(requested_path, '\\') == nullptr;
  for (const auto *override_path : {&g_forced_override_path,
                                    &g_forced_override_path2,
                                    &g_forced_override_path3,
                                    &g_forced_override_path4}) {
    if ((*override_path)[0] == '\0') continue;
    // The provider matches case-insensitively; the engine may hand the
    // device an uppercased spelling of a stock path. Only this explicit
    // override list is widened; selector/audio/scoreboard routes are exact.
    if (std::strcmp(normalized.data(), override_path->data()) == 0) {
      trace_override_match("exact", requested_path, override_path->data());
      return true;
    }
    if (ascii_iequal(normalized.data(), override_path->data())) {
      trace_override_match("case-insensitive", requested_path, override_path->data());
      return true;
    }
    if (leaf_only && ascii_iequal(normalized.data(), path_leaf(override_path->data()))) {
      trace_override_match(std::strcmp(normalized.data(), path_leaf(override_path->data())) == 0
                               ? "leaf" : "leaf-case-insensitive",
                           requested_path, override_path->data());
      return true;
    }
  }
  return false;
}

bool sha256_matches(const std::uint8_t *data, std::uint64_t length) {
  if (data == nullptr || length == 0 || length > kMaxAuditAssetBytes ||
      length > 0xFFFFFFFFull) {
    return false;
  }
  BCRYPT_ALG_HANDLE algorithm = nullptr;
  if (BCryptOpenAlgorithmProvider(
          &algorithm, BCRYPT_SHA256_ALGORITHM, nullptr, 0) < 0) {
    return false;
  }
  DWORD object_bytes = 0;
  DWORD property_bytes = 0;
  if (BCryptGetProperty(
          algorithm, BCRYPT_OBJECT_LENGTH,
          reinterpret_cast<PUCHAR>(&object_bytes), sizeof(object_bytes),
          &property_bytes, 0) < 0 ||
      property_bytes != sizeof(object_bytes) || object_bytes == 0) {
    BCryptCloseAlgorithmProvider(algorithm, 0);
    return false;
  }
  auto *hash_object = static_cast<PUCHAR>(VirtualAlloc(
      nullptr, object_bytes, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE));
  if (hash_object == nullptr) {
    BCryptCloseAlgorithmProvider(algorithm, 0);
    return false;
  }
  BCRYPT_HASH_HANDLE hash = nullptr;
  std::array<std::uint8_t, 32> digest = {};
  NTSTATUS status = BCryptCreateHash(
      algorithm, &hash, hash_object, object_bytes, nullptr, 0, 0);
  if (status >= 0) {
    status = BCryptHashData(
        hash, const_cast<PUCHAR>(reinterpret_cast<const UCHAR *>(data)),
        static_cast<ULONG>(length), 0);
  }
  if (status >= 0) {
    status = BCryptFinishHash(hash, digest.data(),
                              static_cast<ULONG>(digest.size()), 0);
  }
  if (hash != nullptr) BCryptDestroyHash(hash);
  VirtualFree(hash_object, 0, MEM_RELEASE);
  BCryptCloseAlgorithmProvider(algorithm, 0);
  return status >= 0 &&
      std::memcmp(digest.data(), g_expected_sha256.data(), digest.size()) == 0;
}

DeviceIdResult *audit_get_id(void *device, DeviceIdResult *result) {
  InterlockedIncrement(&g_callbacks_in_flight);
  InterlockedIncrement(&g_get_id_calls);
  if (device != nullptr && result != nullptr) {
    const auto *bytes = reinterpret_cast<const std::byte *>(device);
    result->value =
        *reinterpret_cast<const std::uint32_t *>(bytes + 0x40);
  }
  InterlockedDecrement(&g_callbacks_in_flight);
  return result;
}

ProbeStatus *audit_file_info(void *, ProbeStatus *status, OpenResult *result,
                             const char *path) {
  InterlockedIncrement(&g_callbacks_in_flight);
  InterlockedIncrement(&g_file_info_calls);
  if (status != nullptr) {
    status->value = kNotFound;
    if (result != nullptr && path != nullptr &&
        std::strcmp(path, g_provider_path.data()) == 0) {
      status->value = kSuccess;
    }
  }
  InterlockedDecrement(&g_callbacks_in_flight);
  return status;
}

ProbeStatus *audit_actual_open(
    void *, ProbeStatus *status, void *file_handle, const char *path,
    std::uint32_t, std::uint32_t, std::uint32_t) {
  InterlockedIncrement(&g_callbacks_in_flight);
  InterlockedIncrement(&g_actual_open_calls);
  if (status != nullptr) {
    status->value = kNotFound;
    std::uint64_t provider_size = 0;
    if (file_handle != nullptr && path != nullptr &&
        std::strcmp(path, g_provider_path.data()) == 0 &&
        g_file_handle_initialize != nullptr && g_bound_file_handle == nullptr &&
        provider_request(g_provider_path.data(), g_expected_asset_size,
                         kProviderQueryOperation, 0, 0, nullptr,
                         &provider_size) &&
        provider_size == g_expected_asset_size) {
      g_probe_resource.magic = kProbeResourceMagic;
      g_probe_resource.size = g_expected_asset_size;
      g_bound_file_handle = file_handle;
      g_file_handle_initialize(file_handle, &g_probe_resource, 0,
                               g_expected_asset_size);
      status->value = kSuccess;
      InterlockedIncrement(&g_actual_open_successes);
      InterlockedIncrement(&g_resource_handles);
    }
  }
  InterlockedDecrement(&g_callbacks_in_flight);
  return status;
}

ProbeStatus *audit_transfer(
    void *, ProbeStatus *status, void *file_handle, void *destination,
    std::uint64_t offset, std::uint64_t length) {
  InterlockedIncrement(&g_callbacks_in_flight);
  InterlockedIncrement(&g_transfer_calls);
  if (status != nullptr) {
    status->value = kNotFound;
    auto *const token = file_handle == nullptr
        ? nullptr
        : *reinterpret_cast<ProbeResource **>(
              reinterpret_cast<std::byte *>(file_handle) +
              kNativeHandleOffset);
    if (file_handle == g_bound_file_handle && destination != nullptr &&
        token == &g_probe_resource && token->magic == kProbeResourceMagic &&
        offset <= token->size && length <= token->size - offset) {
      if (provider_read_range(static_cast<std::uint8_t *>(destination),
                              offset, length)) {
        status->value = kSuccess;
      }
    }
  }
  InterlockedDecrement(&g_callbacks_in_flight);
  return status;
}

ProbeStatus *audit_close(void *, ProbeStatus *status, void *file_handle) {
  InterlockedIncrement(&g_callbacks_in_flight);
  InterlockedIncrement(&g_close_calls);
  g_last_close_status = kNotFound;
  if (file_handle != nullptr && file_handle == g_bound_file_handle) {
    auto **const token_slot = reinterpret_cast<ProbeResource **>(
        reinterpret_cast<std::byte *>(file_handle) + kNativeHandleOffset);
    if (*token_slot == &g_probe_resource) {
      *token_slot = nullptr;
      g_bound_file_handle = nullptr;
      g_last_close_status = kSuccess;
      InterlockedDecrement(&g_resource_handles);
    }
  }
  if (status != nullptr) status->value = g_last_close_status;
  InterlockedDecrement(&g_callbacks_in_flight);
  return status;
}

bool executable_live_target(std::uintptr_t address) {
  const auto base = reinterpret_cast<std::uintptr_t>(g_live_module);
  return base != 0 && address >= base &&
         address < base + static_cast<std::uintptr_t>(g_live_image_size);
}

template <typename Function>
Function stock_callback(std::size_t slot) {
  if (g_stock_device == nullptr || slot >= nba2k27::kBaseVtableSlots) {
    return nullptr;
  }
  const auto *table =
      *reinterpret_cast<std::uintptr_t *const *>(g_stock_device);
  if (table == nullptr || !executable_live_target(table[slot])) return nullptr;
  return reinterpret_cast<Function>(table[slot]);
}

void update_binding_peak(LONG value) {
  LONG observed = InterlockedCompareExchange(&g_binding_peak, 0, 0);
  while (value > observed) {
    const LONG previous =
        InterlockedCompareExchange(&g_binding_peak, value, observed);
    if (previous == observed) return;
    observed = previous;
  }
}

HandleBinding *reserve_binding(void *file_handle) {
  if (file_handle == nullptr) return nullptr;
  for (auto &binding : g_bindings) {
    if (InterlockedCompareExchange(&binding.state, kBindingReserved,
                                   kBindingFree) == kBindingFree) {
      binding.file_handle = file_handle;
      binding.provider_resource.magic = kProbeResourceMagic;
      binding.provider_resource.size = 0;
      binding.provider_path.fill('\0');
      InterlockedExchange(&binding.target, 0);
      InterlockedExchange(&binding.transferred, 0);
      return &binding;
    }
  }
  return nullptr;
}

void abandon_binding(HandleBinding *binding) {
  if (binding == nullptr) return;
  binding->file_handle = nullptr;
  binding->provider_resource.size = 0;
  binding->provider_path.fill('\0');
  InterlockedExchange(&binding->target, 0);
  InterlockedExchange(&binding->transferred, 0);
  InterlockedExchange(&binding->state, kBindingFree);
}

void activate_binding(HandleBinding *binding, LONG source) {
  InterlockedExchange(&binding->state, source);
  InterlockedIncrement(&g_binding_allocations);
  const LONG active = InterlockedIncrement(&g_binding_active);
  update_binding_peak(active);
  InterlockedIncrement(&g_resource_handles);
}

HandleBinding *find_binding(void *file_handle, LONG *source) {
  if (source == nullptr) return nullptr;
  *source = kBindingFree;
  if (file_handle == nullptr) return nullptr;
  for (auto &binding : g_bindings) {
    const LONG state = InterlockedCompareExchange(&binding.state, 0, 0);
    if ((state == kBindingProvider || state == kBindingStock) &&
        binding.file_handle == file_handle) {
      *source = state;
      return &binding;
    }
  }
  return nullptr;
}

void release_binding(HandleBinding *binding) {
  if (binding == nullptr) return;
  binding->file_handle = nullptr;
  binding->provider_resource.size = 0;
  binding->provider_path.fill('\0');
  InterlockedExchange(&binding->target, 0);
  InterlockedExchange(&binding->transferred, 0);
  InterlockedIncrement(&g_binding_releases);
  InterlockedDecrement(&g_binding_active);
  InterlockedExchange(&binding->state, kBindingFree);
}

bool provider_read_binding(HandleBinding *binding, std::uint8_t *destination,
                           std::uint64_t offset, std::uint64_t length) {
  if (binding == nullptr || destination == nullptr ||
      offset > binding->provider_resource.size ||
      length > binding->provider_resource.size - offset) {
    return false;
  }
  std::uint64_t completed = 0;
  while (completed < length) {
    const std::uint64_t remaining = length - completed;
    const auto chunk = static_cast<std::uint32_t>(
        remaining > kMaxProviderReadBytes ? kMaxProviderReadBytes : remaining);
    std::uint64_t reported_size = 0;
    if (!provider_request(
            binding->provider_path.data(), binding->provider_resource.size,
            kProviderReadOperation, offset + completed, chunk,
            destination + static_cast<std::size_t>(completed),
            &reported_size) ||
        reported_size != binding->provider_resource.size) {
      return false;
    }
    completed += chunk;
  }
  return true;
}

ProbeStatus *live_file_info(void *, ProbeStatus *status, OpenResult *result,
                            const char *path) {
  InterlockedIncrement(&g_callbacks_in_flight);
  InterlockedIncrement(&g_live_file_info_calls);
  if(playlist_path_policy(path,false)){
    if(status)status->value=kNotFound;
    InterlockedDecrement(&g_callbacks_in_flight);return status;
  }
  if (status != nullptr) {
    status->value = kNotFound;
    std::array<char,kProviderPathBytes+1> redirect={};
    const bool redirected=freestyle_redirect(path,&redirect);
    const char* query_path=redirected?redirect.data():path;
    const bool forced_override = redirected||is_forced_override(path);
    bool provider_served = false;
    if (forced_override && result != nullptr && path != nullptr) {
      std::array<char, kProviderPathBytes + 1> provider_path = {};
      std::uint64_t provider_size = 0;
      bool is_target = false;
      if (query_dynamic_provider(query_path, &provider_path, &provider_size,
                                 &is_target)) {
        status->value = kSuccess;
        provider_served = true;
        InterlockedIncrement(&g_live_local_file_info_successes);
        InterlockedExchange64(&g_last_local_activity,
                              static_cast<LONGLONG>(GetTickCount64()));
      }
    }
    if (!redirected && !playlist_path_policy(path,true) && status->value != kSuccess && result != nullptr && path != nullptr &&
        g_stock_device != nullptr) {
      const auto stock = stock_callback<FileInfoCallback>(kFileInfoSlot);
      if (stock != nullptr) {
        stock(g_stock_device, status, result, path);
        if (status->value == kSuccess) {
          InterlockedIncrement(&g_live_stock_file_info_successes);
        }
      }
    }
    if (!forced_override && status->value != kSuccess && result != nullptr &&
        path != nullptr) {
      std::array<char, kProviderPathBytes + 1> provider_path = {};
      std::uint64_t provider_size = 0;
      bool is_target = false;
      if (query_dynamic_provider(path, &provider_path, &provider_size,
                                 &is_target)) {
        status->value = kSuccess;
        provider_served = true;
        InterlockedIncrement(&g_live_local_file_info_successes);
        InterlockedExchange64(&g_last_local_activity,
                              static_cast<LONGLONG>(GetTickCount64()));
      }
    }
    trace_roster_path("file_info", path, forced_override, provider_served,
                      status->value == kSuccess);
  }
  InterlockedDecrement(&g_callbacks_in_flight);
  return status;
}

ProbeStatus *live_actual_open(
    void *, ProbeStatus *status, void *file_handle, const char *path,
    std::uint32_t argument_one, std::uint32_t argument_two,
    std::uint32_t argument_three) {
  InterlockedIncrement(&g_callbacks_in_flight);
  InterlockedIncrement(&g_live_actual_open_calls);
  if (status != nullptr) status->value = kNotFound;
  if(playlist_path_policy(path,false)){
    InterlockedDecrement(&g_callbacks_in_flight);return status;
  }
  std::array<char,kProviderPathBytes+1> redirect={};
  const bool redirected=freestyle_redirect(path,&redirect);
  const char* query_path=redirected?redirect.data():path;
  const bool forced_override = redirected||is_forced_override(path);
  bool provider_served = false;
  if (forced_override && status != nullptr && file_handle != nullptr &&
      path != nullptr) {
    HandleBinding *binding = reserve_binding(file_handle);
    std::uint64_t provider_size = 0;
    bool is_target = false;
    if (binding != nullptr &&
        query_dynamic_provider(query_path, &binding->provider_path, &provider_size,
                               &is_target) &&
        g_file_handle_initialize != nullptr) {
      binding->provider_resource.size = provider_size;
      InterlockedExchange(&binding->target, redirected ? 2 : (is_target ? 1 : 0));
      g_file_handle_initialize(file_handle, &binding->provider_resource, 0,
                               provider_size);
      activate_binding(binding, kBindingProvider);
      status->value = kSuccess;
      provider_served = true;
      InterlockedIncrement(&g_live_local_open_successes);
      InterlockedExchange64(&g_last_local_activity,
                            static_cast<LONGLONG>(GetTickCount64()));
    } else {
      abandon_binding(binding);
    }
  }
  if(redirected){
    const bool success=status&&status->value==kSuccess;
    InterlockedIncrement(success?&g_freestyle_opens:&g_freestyle_failures);
    trace_freestyle(path,query_path,success);
  }
  if (!redirected && !playlist_path_policy(path,true) && status != nullptr && status->value != kSuccess &&
      file_handle != nullptr && path != nullptr &&
      g_stock_device != nullptr) {
    HandleBinding *binding = reserve_binding(file_handle);
    const auto stock = stock_callback<ActualOpenCallback>(kActualOpenSlot);
    if (binding != nullptr && stock != nullptr) {
      stock(g_stock_device, status, file_handle, path, argument_one,
            argument_two, argument_three);
    }
    if (status->value == kSuccess && binding != nullptr) {
      activate_binding(binding, kBindingStock);
      InterlockedIncrement(&g_live_stock_open_successes);
    } else {
      abandon_binding(binding);
    }
  }
  if (!forced_override && status != nullptr && status->value != kSuccess &&
      file_handle != nullptr && path != nullptr) {
    HandleBinding *binding = reserve_binding(file_handle);
    std::uint64_t provider_size = 0;
    bool is_target = false;
    if (binding != nullptr &&
        query_dynamic_provider(path, &binding->provider_path, &provider_size,
                               &is_target) &&
        g_file_handle_initialize != nullptr) {
      binding->provider_resource.size = provider_size;
      InterlockedExchange(&binding->target, is_target ? 1 : 0);
      g_file_handle_initialize(file_handle, &binding->provider_resource, 0,
                               provider_size);
      activate_binding(binding, kBindingProvider);
      status->value = kSuccess;
      provider_served = true;
      InterlockedIncrement(&g_live_local_open_successes);
      InterlockedExchange64(&g_last_local_activity,
                            static_cast<LONGLONG>(GetTickCount64()));
    } else {
      abandon_binding(binding);
    }
  }
  trace_roster_path("open", path, forced_override, provider_served,
                    status != nullptr && status->value == kSuccess);
  InterlockedDecrement(&g_callbacks_in_flight);
  return status;
}

ProbeStatus *live_transfer(void *, ProbeStatus *status, void *file_handle,
                           void *destination, std::uint64_t offset,
                           std::uint64_t length) {
  InterlockedIncrement(&g_callbacks_in_flight);
  InterlockedIncrement(&g_live_transfer_calls);
  if (status != nullptr) status->value = kNotFound;
  if (status != nullptr && file_handle != nullptr && destination != nullptr) {
    LONG source = kBindingFree;
    HandleBinding *binding = find_binding(file_handle, &source);
    auto *const token = *reinterpret_cast<ProbeResource **>(
        reinterpret_cast<std::byte *>(file_handle) + kNativeHandleOffset);
    if (binding != nullptr && source == kBindingProvider &&
        token == &binding->provider_resource &&
        token->magic == kProbeResourceMagic && offset <= token->size &&
        length <= token->size - offset &&
        provider_read_binding(binding,
                              static_cast<std::uint8_t *>(destination),
                              offset, length)) {
      status->value = kSuccess;
      const LONG was_transferred=InterlockedExchange(&binding->transferred, 1);
      if(was_transferred==0&&InterlockedCompareExchange(&binding->target,0,0)==2){
        wchar_t executable[MAX_PATH]={},trace[MAX_PATH]={};
        if(GetModuleFileNameW(nullptr,executable,MAX_PATH)&&make_sibling_path(executable,kTraceName,trace,MAX_PATH)){
          char message[384]={};
          std::snprintf(message,sizeof(message),"FREESTYLE_REDIRECT_READ target=%s bytes=%llu",binding->provider_path.data(),static_cast<unsigned long long>(length));
          trace_checkpoint(trace,message);
        }
      }
      if (InterlockedCompareExchange(&binding->target, 0, 0) == 1) {
        InterlockedExchange(&g_target_read, 1);
      }
      InterlockedExchange64(&g_last_local_activity,
                            static_cast<LONGLONG>(GetTickCount64()));
    } else if (binding != nullptr && source == kBindingStock &&
               g_stock_device != nullptr) {
      const auto stock = stock_callback<TransferCallback>(kTransferSlot);
      if (stock != nullptr) {
        stock(g_stock_device, status, file_handle, destination, offset, length);
      }
    }
  }
  InterlockedDecrement(&g_callbacks_in_flight);
  return status;
}

ProbeStatus *live_close(void *, ProbeStatus *status, void *file_handle) {
  InterlockedIncrement(&g_callbacks_in_flight);
  InterlockedIncrement(&g_live_close_calls);
  std::uint32_t close_status = kNotFound;
  if (file_handle != nullptr) {
    LONG source = kBindingFree;
    HandleBinding *binding = find_binding(file_handle, &source);
    const LONG target = binding == nullptr
        ? 0
        : InterlockedCompareExchange(&binding->target, 0, 0);
    const LONG transferred = binding == nullptr
        ? 0
        : InterlockedCompareExchange(&binding->transferred, 0, 0);
    if (binding != nullptr &&
        InterlockedCompareExchange(&binding->state, kBindingClosing,
                                   source) == source) {
      auto **const token_slot = reinterpret_cast<ProbeResource **>(
          reinterpret_cast<std::byte *>(file_handle) + kNativeHandleOffset);
      if (source == kBindingProvider &&
          *token_slot == &binding->provider_resource) {
        *token_slot = nullptr;
        close_status = kSuccess;
        InterlockedExchange64(&g_last_local_activity,
                              static_cast<LONGLONG>(GetTickCount64()));
      } else if (source == kBindingStock && g_stock_device != nullptr) {
        ProbeStatus stock_status{kNotFound};
        const auto stock = stock_callback<CloseCallback>(kCloseSlot);
        if (stock != nullptr) {
          stock(g_stock_device, &stock_status, file_handle);
        }
        close_status = stock_status.value;
      }
      if (close_status == kSuccess) {
        release_binding(binding);
        InterlockedDecrement(&g_resource_handles);
        if (source == kBindingProvider && target == 1 && transferred == 1) {
          InterlockedExchange(&g_target_completed, 1);
        }
      } else {
        InterlockedExchange(&binding->state, source);
      }
    }
  }
  if (status != nullptr) status->value = close_status;
  InterlockedDecrement(&g_callbacks_in_flight);
  return status;
}

bool clone_live_vtable(const std::byte *module,
                       const nba2k27::ProviderLayout &layout) {
  if (module == nullptr) return false;
  const auto *stock = reinterpret_cast<const std::uintptr_t *>(
      module + layout.base_vtable);
  const auto module_base = reinterpret_cast<std::uintptr_t>(module);
  for (std::size_t index = 0; index < nba2k27::kBaseVtableSlots; ++index) {
    if (stock[index] != module_base + layout.base_vtable_targets[index]) {
      return false;
    }
    g_live_vtable[index] = stock[index];
  }
  g_live_vtable[kGetIdSlot] = reinterpret_cast<std::uintptr_t>(&audit_get_id);
  g_live_vtable[kFileInfoSlot] =
      reinterpret_cast<std::uintptr_t>(&live_file_info);
  g_live_vtable[kActualOpenSlot] =
      reinterpret_cast<std::uintptr_t>(&live_actual_open);
  g_live_vtable[kCloseSlot] = reinterpret_cast<std::uintptr_t>(&live_close);
  g_live_vtable[kTransferSlot] =
      reinterpret_cast<std::uintptr_t>(&live_transfer);
  return true;
}

bool has_anti_cheat_module() {
  constexpr const wchar_t *modules[] = {
      L"EasyAntiCheat_EOS.dll",
      L"EasyAntiCheat_x64.dll",
      L"EasyAntiCheat.dll",
  };
  for (const wchar_t *module : modules) {
    if (GetModuleHandleW(module) != nullptr) return true;
  }
  return false;
}

void append(char *output, std::size_t capacity, std::size_t *used,
            const char *format, ...) {
  if (*used >= capacity) return;
  va_list arguments;
  va_start(arguments, format);
  const int added =
      std::vsnprintf(output + *used, capacity - *used, format, arguments);
  va_end(arguments);
  if (added <= 0) return;
  const auto count = static_cast<std::size_t>(added);
  *used += count < capacity - *used ? count : capacity - *used;
}

bool make_sibling_path(const wchar_t *executable_path, const wchar_t *leaf,
                       wchar_t *output, std::size_t capacity) {
  if(!wcscmp(leaf,kLogName)||!wcscmp(leaf,kArmedName)||!wcscmp(leaf,kTraceName)) {
    const auto directory=outfit_match_v2::bridge::diagnostics_for_executable(executable_path);
    return !directory.empty()&&wcscpy_s(output,capacity,(directory+leaf).c_str())==0;
  }
#ifdef NBA2K27_SESSION_OUTFITS
  if(!wcscmp(leaf,kLogName)||!wcscmp(leaf,kArmedName)||!wcscmp(leaf,kTraceName)) {
    const auto directory=session_diagnostics_directory();
    return !directory.empty()&&wcscpy_s(output,capacity,(directory+leaf).c_str())==0;
  }
#endif
  if (wcscpy_s(output, capacity, executable_path) != 0) return false;
  wchar_t *separator = wcsrchr(output, L'\\');
  if (separator == nullptr) return false;
  *(separator + 1) = L'\0';
  return wcscat_s(output, capacity, leaf) == 0;
}

bool write_text_file(const wchar_t *path, const char *text) {
  const HANDLE file = CreateFileW(path, GENERIC_WRITE,
                                  FILE_SHARE_READ | FILE_SHARE_WRITE |
                                      FILE_SHARE_DELETE,
                                  nullptr, CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL,
                                  nullptr);
  if (file == INVALID_HANDLE_VALUE) return false;
  const DWORD requested = static_cast<DWORD>(std::strlen(text));
  DWORD written = 0;
  const BOOL ok = WriteFile(file, text, requested, &written, nullptr);
  FlushFileBuffers(file);
  CloseHandle(file);
  return ok != FALSE && written == requested;
}

void trace_checkpoint(const wchar_t *path, const char *message) {
  if (path == nullptr || message == nullptr) return;
  const HANDLE file = CreateFileW(path, FILE_APPEND_DATA,
                                  FILE_SHARE_READ | FILE_SHARE_WRITE |
                                      FILE_SHARE_DELETE,
                                  nullptr, OPEN_ALWAYS, FILE_ATTRIBUTE_NORMAL,
                                  nullptr);
  if (file == INVALID_HANDLE_VALUE) return;
  char line[256] = {};
  const int length = std::snprintf(
      line, sizeof(line), "%llu %s\r\n",
      static_cast<unsigned long long>(GetTickCount64()), message);
  if (length > 0) {
    DWORD written = 0;
    WriteFile(file, line, static_cast<DWORD>(length), &written, nullptr);
    FlushFileBuffers(file);
  }
  CloseHandle(file);
}

bool runtime_identity_matches(HMODULE module,
                              const nba2k27::ResolverReport &resolver) {
  if (module == nullptr || !resolver.image_parsed) return false;
  const auto *base = reinterpret_cast<const std::byte *>(module);
  const auto *dos = reinterpret_cast<const IMAGE_DOS_HEADER *>(base);
  if (dos->e_magic != IMAGE_DOS_SIGNATURE || dos->e_lfanew <= 0) return false;
  const auto *nt = reinterpret_cast<const IMAGE_NT_HEADERS64 *>(
      base + static_cast<std::size_t>(dos->e_lfanew));
  return nt->Signature == IMAGE_NT_SIGNATURE &&
         nt->FileHeader.TimeDateStamp == resolver.timestamp &&
         nt->OptionalHeader.SizeOfImage == resolver.image_size;
}

std::uint32_t read_default_device_id(void *manager) {
  const auto *bytes = reinterpret_cast<const std::byte *>(manager);
  return *reinterpret_cast<const volatile std::uint32_t *>(
      bytes + kDefaultDeviceIdOffset);
}

bool release_device_reference(const std::byte *module,
                              const nba2k27::ProviderLayout &layout,
                              void *device) {
  if (module == nullptr || device == nullptr) return false;
  const auto release = reinterpret_cast<ReferenceRelease>(
      const_cast<std::byte *>(module) + layout.reference_release);
  return release(reinterpret_cast<std::byte *>(device) + 0x18);
}

bool clone_verified_vtable(const std::byte *module,
                           const nba2k27::ProviderLayout &layout) {
  if (module == nullptr) return false;
  const auto *stock = reinterpret_cast<const std::uintptr_t *>(
      module + layout.base_vtable);
  const auto module_base = reinterpret_cast<std::uintptr_t>(module);
  for (std::size_t index = 0; index < nba2k27::kBaseVtableSlots; ++index) {
    if (stock[index] != module_base + layout.base_vtable_targets[index]) {
      return false;
    }
    g_audit_vtable[index] = stock[index];
  }
  g_audit_vtable[kGetIdSlot] =
      reinterpret_cast<std::uintptr_t>(&audit_get_id);
  g_audit_vtable[kFileInfoSlot] =
      reinterpret_cast<std::uintptr_t>(&audit_file_info);
  g_audit_vtable[kActualOpenSlot] =
      reinterpret_cast<std::uintptr_t>(&audit_actual_open);
  g_audit_vtable[kCloseSlot] =
      reinterpret_cast<std::uintptr_t>(&audit_close);
  g_audit_vtable[kTransferSlot] =
      reinterpret_cast<std::uintptr_t>(&audit_transfer);
  return true;
}

RegistrationAudit run_registration_audit(
    const nba2k27::ResolverReport &resolver, bool anti_cheat_present,
    const wchar_t *trace_path, const wchar_t *provider_config_path) {
  RegistrationAudit audit = {};
  trace_checkpoint(trace_path, "AUDIT_ENTERED");
  if (!resolver.complete) {
    audit.failure = "structural resolver incomplete";
    return audit;
  }
  if (anti_cheat_present) {
    audit.failure = "anti-cheat module present";
    return audit;
  }
  trace_checkpoint(trace_path, "PROVIDER_CONFIG_LOAD_BEGIN");
  audit.provider_configured = load_provider_config(provider_config_path);
  audit.asset_size = g_expected_asset_size;
  trace_checkpoint(trace_path, audit.provider_configured
                                   ? "PROVIDER_CONFIG_LOAD_PASS"
                                   : "PROVIDER_CONFIG_LOAD_FAIL");
  if (!audit.provider_configured) {
    audit.failure = "local provider audit configuration is invalid";
    return audit;
  }

  const auto module_handle = GetModuleHandleW(nullptr);
  trace_checkpoint(trace_path, "RUNTIME_IDENTITY_CHECK_BEGIN");
  audit.runtime_identity_matches =
      runtime_identity_matches(module_handle, resolver);
  trace_checkpoint(trace_path, audit.runtime_identity_matches
                                   ? "RUNTIME_IDENTITY_CHECK_PASS"
                                   : "RUNTIME_IDENTITY_CHECK_FAIL");
  if (!audit.runtime_identity_matches) {
    audit.failure = "runtime image does not match resolved file";
    return audit;
  }
  const auto *module = reinterpret_cast<const std::byte *>(module_handle);
  const auto &layout = resolver.layout;
  const auto manager_getter = reinterpret_cast<ManagerGetter>(
      const_cast<std::byte *>(module) + layout.manager_getter);

  void *manager = nullptr;
  for (std::uint32_t attempt = 0; attempt < 75; ++attempt) {
    if (has_anti_cheat_module()) {
      audit.failure = "anti-cheat module appeared while waiting";
      return audit;
    }
    trace_checkpoint(trace_path, "CALL_MANAGER_GETTER");
    manager = manager_getter();
    trace_checkpoint(trace_path, "RETURN_MANAGER_GETTER");
    if (manager != nullptr) {
      trace_checkpoint(trace_path, "READ_DEFAULT_FOR_READINESS");
      if (read_default_device_id(manager) != 0) break;
      trace_checkpoint(trace_path, "DEFAULT_NOT_READY");
    }
    Sleep(1000);
  }
  trace_checkpoint(trace_path, "MANAGER_WAIT_FINISHED");
  if (manager == nullptr || read_default_device_id(manager) == 0) {
    audit.failure = "resource manager did not become ready";
    return audit;
  }

  audit.attempted = true;
  audit.manager_ready = true;
  trace_checkpoint(trace_path, "MANAGER_READY");
  audit.default_before = read_default_device_id(manager);
  if (audit.default_before != kExpectedStockDeviceId) {
    audit.default_after = audit.default_before;
    audit.failure = "stock default device ID was not present";
    return audit;
  }

  const auto base_constructor = reinterpret_cast<BaseConstructor>(
      const_cast<std::byte *>(module) + layout.base_constructor);
  const auto set_device_id = reinterpret_cast<SetDeviceId>(
      const_cast<std::byte *>(module) + layout.set_device_id);
  const auto manager_register = reinterpret_cast<ManagerRegister>(
      const_cast<std::byte *>(module) + layout.manager_register);
  const auto manager_find = reinterpret_cast<ManagerFind>(
      const_cast<std::byte *>(module) + layout.manager_find);
  const auto base_destructor = reinterpret_cast<BaseNonDeletingDestructor>(
      const_cast<std::byte *>(module) + layout.base_destructor);
  const auto manager_open = reinterpret_cast<ManagerOpen>(
      const_cast<std::byte *>(module) + layout.manager_open);
  const auto file_handle_constructor = reinterpret_cast<FileHandleConstructor>(
      const_cast<std::byte *>(module) + layout.file_handle_constructor);
  g_file_handle_initialize = reinterpret_cast<FileHandleInitialize>(
      const_cast<std::byte *>(module) + layout.file_handle_initialize);
  const auto file_handle_read = reinterpret_cast<FileHandleRead>(
      const_cast<std::byte *>(module) + layout.file_handle_read);
  const auto file_handle_destructor = reinterpret_cast<FileHandleDestructor>(
      const_cast<std::byte *>(module) + layout.file_handle_destructor);

  trace_checkpoint(trace_path, "CALL_MANAGER_FIND_PRECHECK");
  void *existing = manager_find(manager, kAuditDeviceId);
  trace_checkpoint(trace_path, "RETURN_MANAGER_FIND_PRECHECK");
  audit.audit_id_available = existing == nullptr;
  if (existing != nullptr) {
    release_device_reference(module, layout, existing);
    audit.default_after = read_default_device_id(manager);
    audit.default_unchanged = audit.default_after == audit.default_before;
    audit.failure = "isolated audit device ID is already registered";
    return audit;
  }

  alignas(16) std::array<std::byte, kDeviceBytes> device_storage = {};
  void *const device = device_storage.data();
  trace_checkpoint(trace_path, "CALL_BASE_CONSTRUCTOR");
  audit.constructed = base_constructor(device) == device;
  trace_checkpoint(trace_path, "RETURN_BASE_CONSTRUCTOR");
  if (!audit.constructed) {
    audit.default_after = read_default_device_id(manager);
    audit.default_unchanged = audit.default_after == audit.default_before;
    audit.failure = "base device construction failed";
    return audit;
  }

  auto *const *links = reinterpret_cast<void *const *>(
      device_storage.data() + 0x08);
  audit.constructor_self_linked = links[0] == device && links[1] == device;

  ProbeStatus status = {};
  trace_checkpoint(trace_path, "CALL_SET_DEVICE_ID");
  set_device_id(device, &status, kAuditDeviceId);
  trace_checkpoint(trace_path, "RETURN_SET_DEVICE_ID");
  audit.set_id_status = status.value;
  audit.id_set = status.value == kSuccess &&
      *reinterpret_cast<const std::uint32_t *>(
          device_storage.data() + 0x40) == kAuditDeviceId;

  trace_checkpoint(trace_path, "VERIFY_AND_CLONE_BASE_VTABLE");
  audit.private_vtable_verified = clone_verified_vtable(module, layout);
  if (audit.private_vtable_verified) {
    InterlockedExchange(&g_get_id_calls, 0);
    InterlockedExchange(&g_file_info_calls, 0);
    InterlockedExchange(&g_actual_open_calls, 0);
    InterlockedExchange(&g_actual_open_successes, 0);
    InterlockedExchange(&g_transfer_calls, 0);
    InterlockedExchange(&g_close_calls, 0);
    InterlockedExchange(&g_resource_handles, 0);
    InterlockedExchange(&g_callbacks_in_flight, 0);
    g_bound_file_handle = nullptr;
    g_last_close_status = kNotFound;
    g_file_handle.fill(std::byte{0});
    InterlockedExchange(&g_provider_query_calls, 0);
    InterlockedExchange(&g_provider_read_calls, 0);
    InterlockedExchange64(&g_provider_bytes, 0);
    *reinterpret_cast<std::uintptr_t **>(device) = g_audit_vtable.data();
    const auto module_base = reinterpret_cast<std::uintptr_t>(module);
    audit.stock_exists_inherited =
        g_audit_vtable[kExistsSlot] ==
            module_base + layout.base_vtable_targets[kExistsSlot] &&
        layout.base_vtable_targets[kExistsSlot] == layout.base_exists;
    trace_checkpoint(trace_path, "PRIVATE_ID_AND_FILE_INFO_VTABLE_INSTALLED");
  } else {
    trace_checkpoint(trace_path, "PRIVATE_GET_ID_VTABLE_REJECTED");
  }

  if (audit.constructor_self_linked && audit.id_set &&
      audit.private_vtable_verified) {
    status.value = 0;
    audit.register_called = true;
    trace_checkpoint(trace_path, "CALL_MANAGER_REGISTER");
    manager_register(manager, &status, device);
    trace_checkpoint(trace_path, "RETURN_MANAGER_REGISTER");
    audit.registration_status = status.value;
    links = reinterpret_cast<void *const *>(device_storage.data() + 0x08);
    audit.registered = status.value == kSuccess &&
                       links[0] != device && links[1] != device;
  }

  if (audit.registered) {
    trace_checkpoint(trace_path, "CALL_MANAGER_FIND_DURING");
    void *found = manager_find(manager, kAuditDeviceId);
    trace_checkpoint(trace_path, "RETURN_MANAGER_FIND_DURING");
    audit.find_during = found == device;
    if (found != nullptr) {
      trace_checkpoint(trace_path, "CALL_REFERENCE_RELEASE_DURING");
      audit.find_during_released =
          release_device_reference(module, layout, found);
      trace_checkpoint(trace_path, "RETURN_REFERENCE_RELEASE_DURING");
    }

    const auto exists = reinterpret_cast<ExistsCallback>(
        g_audit_vtable[kExistsSlot]);
    trace_checkpoint(trace_path, "CALL_BASE_EXISTS_TARGET");
    audit.exists_hit = exists(device, g_provider_path.data());
    trace_checkpoint(trace_path, "RETURN_BASE_EXISTS_TARGET");
    trace_checkpoint(trace_path, "CALL_BASE_EXISTS_MISS");
    audit.exists_miss_rejected = !exists(device, kExistsMiss);
    trace_checkpoint(trace_path, "RETURN_BASE_EXISTS_MISS");
    audit.exists_file_info_calls = static_cast<std::uint32_t>(
        InterlockedCompareExchange(&g_file_info_calls, 0, 0));

    void *const file_handle = g_file_handle.data();
    trace_checkpoint(trace_path, "CALL_FILE_HANDLE_CONSTRUCTOR");
    audit.file_handle_constructed =
        file_handle_constructor(file_handle) == file_handle;
    trace_checkpoint(trace_path, "RETURN_FILE_HANDLE_CONSTRUCTOR");
    if (audit.file_handle_constructed) {
      auto *readback = static_cast<std::uint8_t *>(VirtualAlloc(
          nullptr, static_cast<SIZE_T>(g_expected_asset_size),
          MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE));
      ProbeStatus open_status{kNotFound};
      trace_checkpoint(trace_path, "CALL_MANAGER_OPEN_TEST_STREAM");
      manager_open(manager, &open_status, kAuditDeviceId, file_handle,
                   g_provider_path.data(), 1, 4, 3);
      trace_checkpoint(trace_path, "RETURN_MANAGER_OPEN_TEST_STREAM");
      audit.manager_open_status = open_status.value;

      if (open_status.value == kSuccess && readback != nullptr) {
        ProbeStatus read_status{kNotFound};
        std::uint64_t bytes_read = 0;
        trace_checkpoint(trace_path, "CALL_FILE_HANDLE_READ_TEST_STREAM");
        file_handle_read(file_handle, &read_status, readback, 0,
                         g_expected_asset_size, &bytes_read, 0x7F7FFFFFu);
        trace_checkpoint(trace_path, "RETURN_FILE_HANDLE_READ_TEST_STREAM");
        audit.read_status = read_status.value;
        audit.bytes_read = bytes_read;
        audit.payload_match = bytes_read == g_expected_asset_size &&
            sha256_matches(readback, bytes_read);
      }

      trace_checkpoint(trace_path, "CALL_FILE_HANDLE_DESTRUCTOR");
      file_handle_destructor(file_handle);
      trace_checkpoint(trace_path, "RETURN_FILE_HANDLE_DESTRUCTOR");
      audit.close_status = g_last_close_status;
      audit.resource_released = g_bound_file_handle == nullptr;
      if (g_bound_file_handle != nullptr) {
        auto **const token_slot = reinterpret_cast<ProbeResource **>(
            reinterpret_cast<std::byte *>(file_handle) +
            kNativeHandleOffset);
        if (*token_slot == &g_probe_resource) *token_slot = nullptr;
        g_bound_file_handle = nullptr;
        InterlockedExchange(&g_resource_handles, 0);
      }
      if (readback != nullptr) VirtualFree(readback, 0, MEM_RELEASE);
    }
  }

  trace_checkpoint(trace_path, "CALL_BASE_DESTRUCTOR");
  base_destructor(device);
  trace_checkpoint(trace_path, "RETURN_BASE_DESTRUCTOR");
  audit.destructor_called = true;
  links = reinterpret_cast<void *const *>(device_storage.data() + 0x08);
  audit.self_linked_after = links[0] == device && links[1] == device;

  trace_checkpoint(trace_path, "CALL_MANAGER_FIND_AFTER");
  void *found_after = manager_find(manager, kAuditDeviceId);
  trace_checkpoint(trace_path, "RETURN_MANAGER_FIND_AFTER");
  audit.absent_after = found_after == nullptr;
  if (found_after != nullptr) {
    release_device_reference(module, layout, found_after);
  }
  audit.default_after = read_default_device_id(manager);
  audit.default_unchanged = audit.default_after == audit.default_before;
  audit.get_id_calls = static_cast<std::uint32_t>(
      InterlockedCompareExchange(&g_get_id_calls, 0, 0));
  audit.file_info_calls = static_cast<std::uint32_t>(
      InterlockedCompareExchange(&g_file_info_calls, 0, 0));
  audit.actual_open_calls = static_cast<std::uint32_t>(
      InterlockedCompareExchange(&g_actual_open_calls, 0, 0));
  audit.actual_open_successes = static_cast<std::uint32_t>(
      InterlockedCompareExchange(&g_actual_open_successes, 0, 0));
  audit.transfer_calls = static_cast<std::uint32_t>(
      InterlockedCompareExchange(&g_transfer_calls, 0, 0));
  audit.close_calls = static_cast<std::uint32_t>(
      InterlockedCompareExchange(&g_close_calls, 0, 0));
  audit.resource_handles = static_cast<std::uint32_t>(
      InterlockedCompareExchange(&g_resource_handles, 0, 0));
  audit.provider_query_calls = static_cast<std::uint32_t>(
      InterlockedCompareExchange(&g_provider_query_calls, 0, 0));
  audit.provider_read_calls = static_cast<std::uint32_t>(
      InterlockedCompareExchange(&g_provider_read_calls, 0, 0));
  audit.provider_bytes = static_cast<std::uint64_t>(
      InterlockedCompareExchange64(&g_provider_bytes, 0, 0));
  g_file_handle_initialize = nullptr;
  audit.callbacks_quiesced =
      InterlockedCompareExchange(&g_callbacks_in_flight, 0, 0) == 0;
  audit.passed = audit.manager_ready && audit.runtime_identity_matches &&
                 audit.provider_configured && audit.audit_id_available &&
                 audit.constructed && audit.constructor_self_linked &&
                 audit.id_set && audit.private_vtable_verified &&
                 audit.stock_exists_inherited &&
                 audit.registered && audit.find_during &&
                 audit.find_during_released && audit.destructor_called &&
                 audit.self_linked_after && audit.absent_after &&
                 audit.default_unchanged && audit.get_id_calls > 0 &&
                 audit.exists_file_info_calls == 2 && audit.exists_hit &&
                 audit.exists_miss_rejected && audit.file_handle_constructed &&
                 audit.manager_open_status == kSuccess &&
                 audit.actual_open_calls == 1 &&
                 audit.actual_open_successes == 1 &&
                 audit.read_status == kSuccess &&
                 audit.transfer_calls > 0 &&
                 audit.bytes_read == audit.asset_size &&
                 audit.provider_query_calls == 1 &&
                 audit.provider_read_calls > 0 &&
                 audit.provider_bytes == audit.asset_size &&
                 audit.payload_match && audit.close_calls == 1 &&
                 audit.close_status == kSuccess && audit.resource_released &&
                 audit.resource_handles == 0 &&
                 audit.callbacks_quiesced;
  if (audit.passed) {
    audit.failure = "none";
  } else if (!audit.constructor_self_linked) {
    audit.failure = "constructed device was not self-linked";
  } else if (!audit.id_set) {
    audit.failure = "device ID assignment failed";
  } else if (!audit.private_vtable_verified) {
    audit.failure = "private base-vtable clone failed verification";
  } else if (!audit.stock_exists_inherited) {
    audit.failure = "stock Exists dispatch was not inherited";
  } else if (!audit.registered) {
    audit.failure = "manager registration failed";
  } else if (!audit.find_during) {
    audit.failure = "manager could not find registered device";
  } else if (!audit.find_during_released) {
    audit.failure = "manager-find reference release failed";
  } else if (!audit.self_linked_after) {
    audit.failure = "device was not self-linked after destruction";
  } else if (!audit.absent_after) {
    audit.failure = "device remained registered after destruction";
  } else if (!audit.default_unchanged) {
    audit.failure = "stock default device ID changed";
  } else if (audit.get_id_calls == 0) {
    audit.failure = "custom Get ID callback was not dispatched";
  } else if (audit.exists_file_info_calls != 2) {
    audit.failure = "base Exists did not dispatch both File Info probes";
  } else if (!audit.exists_hit) {
    audit.failure = "configured path was not reported present";
  } else if (!audit.exists_miss_rejected) {
    audit.failure = "unconfigured path was not rejected";
  } else if (!audit.file_handle_constructed) {
    audit.failure = "file handle construction failed";
  } else if (audit.manager_open_status != kSuccess ||
             audit.actual_open_calls != 1 ||
             audit.actual_open_successes != 1) {
    audit.failure = "manager-owned stream open failed";
  } else if (audit.read_status != kSuccess || audit.transfer_calls == 0 ||
             audit.bytes_read != audit.asset_size ||
             audit.provider_query_calls != 1 ||
             audit.provider_read_calls == 0 ||
             audit.provider_bytes != audit.asset_size ||
             !audit.payload_match) {
    audit.failure = "manager-owned stream payload read failed";
  } else if (audit.close_calls != 1 || audit.close_status != kSuccess ||
             !audit.resource_released) {
    audit.failure = "manager-owned stream close failed";
  } else if (!audit.callbacks_quiesced) {
    audit.failure = "custom callback did not quiesce";
  } else {
    audit.failure = "registration lifecycle check failed";
  }
  trace_checkpoint(trace_path, audit.passed ? "AUDIT_FINISHED_PASS"
                                            : "AUDIT_FINISHED_FAIL");
  return audit;
}

void reset_live_route_state() {
  InterlockedExchange(&g_get_id_calls, 0);
  InterlockedExchange(&g_callbacks_in_flight, 0);
  InterlockedExchange(&g_resource_handles, 0);
  InterlockedExchange(&g_provider_query_calls, 0);
  InterlockedExchange(&g_provider_read_calls, 0);
  InterlockedExchange64(&g_provider_bytes, 0);
  InterlockedExchange(&g_live_file_info_calls, 0);
  InterlockedExchange(&g_live_stock_file_info_successes, 0);
  InterlockedExchange(&g_live_local_file_info_successes, 0);
  InterlockedExchange(&g_live_actual_open_calls, 0);
  InterlockedExchange(&g_live_stock_open_successes, 0);
  InterlockedExchange(&g_live_local_open_successes, 0);
  InterlockedExchange(&g_live_transfer_calls, 0);
  InterlockedExchange(&g_live_close_calls, 0);
  InterlockedExchange(&g_binding_allocations, 0);
  InterlockedExchange(&g_binding_releases, 0);
  InterlockedExchange(&g_binding_active, 0);
  InterlockedExchange(&g_binding_peak, 0);
  InterlockedExchange(&g_target_read, 0);
  InterlockedExchange(&g_target_completed, 0);
  InterlockedExchange64(&g_last_local_activity,
                        static_cast<LONGLONG>(GetTickCount64()));
  g_live_device.fill(std::byte{0});
  for (auto &binding : g_bindings) {
    binding.file_handle = nullptr;
    binding.provider_resource.magic = kProbeResourceMagic;
    binding.provider_resource.size = 0;
    binding.provider_path.fill('\0');
    InterlockedExchange(&binding.target, 0);
    InterlockedExchange(&binding.transferred, 0);
    InterlockedExchange(&binding.state, kBindingFree);
  }
}

void snapshot_live_route(LiveRouteResult *result) {
  if (result == nullptr) return;
  result->target_read =
      InterlockedCompareExchange(&g_target_read, 0, 0) == 1;
  result->target_completed =
      InterlockedCompareExchange(&g_target_completed, 0, 0) == 1;
  result->file_info_calls = static_cast<std::uint32_t>(
      InterlockedCompareExchange(&g_live_file_info_calls, 0, 0));
  result->stock_file_info_successes = static_cast<std::uint32_t>(
      InterlockedCompareExchange(&g_live_stock_file_info_successes, 0, 0));
  result->local_file_info_successes = static_cast<std::uint32_t>(
      InterlockedCompareExchange(&g_live_local_file_info_successes, 0, 0));
  result->actual_open_calls = static_cast<std::uint32_t>(
      InterlockedCompareExchange(&g_live_actual_open_calls, 0, 0));
  result->stock_open_successes = static_cast<std::uint32_t>(
      InterlockedCompareExchange(&g_live_stock_open_successes, 0, 0));
  result->local_open_successes = static_cast<std::uint32_t>(
      InterlockedCompareExchange(&g_live_local_open_successes, 0, 0));
  result->transfer_calls = static_cast<std::uint32_t>(
      InterlockedCompareExchange(&g_live_transfer_calls, 0, 0));
  result->close_calls = static_cast<std::uint32_t>(
      InterlockedCompareExchange(&g_live_close_calls, 0, 0));
  result->binding_allocations = static_cast<std::uint32_t>(
      InterlockedCompareExchange(&g_binding_allocations, 0, 0));
  result->binding_releases = static_cast<std::uint32_t>(
      InterlockedCompareExchange(&g_binding_releases, 0, 0));
  result->binding_peak = static_cast<std::uint32_t>(
      InterlockedCompareExchange(&g_binding_peak, 0, 0));
  result->binding_active = static_cast<std::uint32_t>(
      InterlockedCompareExchange(&g_binding_active, 0, 0));
  result->provider_query_calls = static_cast<std::uint32_t>(
      InterlockedCompareExchange(&g_provider_query_calls, 0, 0));
  result->provider_read_calls = static_cast<std::uint32_t>(
      InterlockedCompareExchange(&g_provider_read_calls, 0, 0));
  result->provider_bytes = static_cast<std::uint64_t>(
      InterlockedCompareExchange64(&g_provider_bytes, 0, 0));
}

LiveRouteResult run_live_route(const nba2k27::ResolverReport &resolver,
                               const wchar_t *trace_path) {
  LiveRouteResult result = {};
  trace_checkpoint(trace_path, "LIVE_ROUTE_ENTERED");
  if (!resolver.complete || has_anti_cheat_module()) {
    result.failure = "resolver incomplete or anti-cheat present";
    return result;
  }
  const auto module_handle = GetModuleHandleW(nullptr);
  if (!runtime_identity_matches(module_handle, resolver)) {
    result.failure = "runtime identity changed before live route";
    return result;
  }
  g_live_module = reinterpret_cast<const std::byte *>(module_handle);
  g_live_image_size = resolver.image_size;
  g_live_timestamp = resolver.timestamp;
  const auto &layout = resolver.layout;
  const auto manager_getter = reinterpret_cast<ManagerGetter>(
      const_cast<std::byte *>(g_live_module) + layout.manager_getter);
  g_live_manager = manager_getter();
  if (g_live_manager == nullptr) {
    result.failure = "resource manager unavailable";
    return result;
  }
  result.default_before = read_default_device_id(g_live_manager);
  result.default_after = result.default_before;
  if (result.default_before != kExpectedStockDeviceId) {
    result.failure = "stock device is not the current default";
    return result;
  }

  reset_live_route_state();
  if (!clone_live_vtable(g_live_module, layout)) {
    result.failure = "live vtable verification failed";
    return result;
  }
  const auto base_constructor = reinterpret_cast<BaseConstructor>(
      const_cast<std::byte *>(g_live_module) + layout.base_constructor);
  const auto set_device_id = reinterpret_cast<SetDeviceId>(
      const_cast<std::byte *>(g_live_module) + layout.set_device_id);
  const auto manager_register = reinterpret_cast<ManagerRegister>(
      const_cast<std::byte *>(g_live_module) + layout.manager_register);
  const auto manager_find = reinterpret_cast<ManagerFind>(
      const_cast<std::byte *>(g_live_module) + layout.manager_find);
  const auto base_destructor = reinterpret_cast<BaseNonDeletingDestructor>(
      const_cast<std::byte *>(g_live_module) + layout.base_destructor);
  g_file_handle_initialize = reinterpret_cast<FileHandleInitialize>(
      const_cast<std::byte *>(g_live_module) + layout.file_handle_initialize);

  void *const device = g_live_device.data();
  trace_checkpoint(trace_path, "LIVE_CALL_BASE_CONSTRUCTOR");
  const bool constructed = base_constructor(device) == device;
  trace_checkpoint(trace_path, "LIVE_RETURN_BASE_CONSTRUCTOR");
  if (!constructed) {
    result.failure = "live device construction failed";
    return result;
  }
  ProbeStatus status{};
  trace_checkpoint(trace_path, "LIVE_CALL_SET_DEVICE_ID");
  set_device_id(device, &status, kAuditDeviceId);
  trace_checkpoint(trace_path, "LIVE_RETURN_SET_DEVICE_ID");
  if (status.value != kSuccess ||
      *reinterpret_cast<const std::uint32_t *>(
          g_live_device.data() + 0x40) != kAuditDeviceId) {
    base_destructor(device);
    result.failure = "live device ID assignment failed";
    return result;
  }
  *reinterpret_cast<std::uintptr_t **>(device) = g_live_vtable.data();

  trace_checkpoint(trace_path, "LIVE_CAPTURE_STOCK_DEVICE");
  g_stock_device = manager_find(g_live_manager, result.default_before);
  if (g_stock_device == nullptr || g_stock_device == device ||
      stock_callback<FileInfoCallback>(kFileInfoSlot) == nullptr ||
      stock_callback<ActualOpenCallback>(kActualOpenSlot) == nullptr ||
      stock_callback<TransferCallback>(kTransferSlot) == nullptr ||
      stock_callback<CloseCallback>(kCloseSlot) == nullptr) {
    if (g_stock_device != nullptr) {
      release_device_reference(g_live_module, layout, g_stock_device);
      g_stock_device = nullptr;
    }
    base_destructor(device);
    result.failure = "stock delegate capture failed";
    return result;
  }

  status.value = 0;
  trace_checkpoint(trace_path, "LIVE_CALL_MANAGER_REGISTER");
  manager_register(g_live_manager, &status, device);
  trace_checkpoint(trace_path, "LIVE_RETURN_MANAGER_REGISTER");
  result.registration_status = status.value;
  const auto *device_bytes = reinterpret_cast<const std::byte *>(device);
  auto *const *links = reinterpret_cast<void *const *>(device_bytes + 0x08);
  const bool registered = status.value == kSuccess &&
                          links[0] != device && links[1] != device;
  if (!registered) {
    release_device_reference(g_live_module, layout, g_stock_device);
    g_stock_device = nullptr;
    base_destructor(device);
    result.failure = "live manager registration failed";
    return result;
  }

  auto *default_slot = reinterpret_cast<volatile LONG *>(
      reinterpret_cast<std::byte *>(g_live_manager) + kDefaultDeviceIdOffset);
  const auto previous = static_cast<std::uint32_t>(InterlockedCompareExchange(
      default_slot, static_cast<LONG>(kAuditDeviceId),
      static_cast<LONG>(result.default_before)));
  result.default_during = read_default_device_id(g_live_manager);
  if (previous != result.default_before ||
      result.default_during != kAuditDeviceId) {
    InterlockedCompareExchange(default_slot,
                               static_cast<LONG>(result.default_before),
                               static_cast<LONG>(kAuditDeviceId));
    result.default_after = read_default_device_id(g_live_manager);
    base_destructor(device);
    release_device_reference(g_live_module, layout, g_stock_device);
    g_stock_device = nullptr;
    result.failure = "temporary default activation failed";
    return result;
  }
  result.started = true;
  trace_checkpoint(trace_path, "LIVE_ROUTE_ACTIVE");
  snapshot_live_route(&result);
  result.default_after = read_default_device_id(g_live_manager);
  result.callbacks_quiesced =
      InterlockedCompareExchange(&g_callbacks_in_flight, 0, 0) == 0;
  result.session_active = result.default_after == kAuditDeviceId;
  result.passed = result.session_active && result.callbacks_quiesced;
  result.failure = result.passed
      ? "none"
      : "session provider registration did not remain active";
  trace_checkpoint(trace_path,
                   result.passed ? "SESSION_ROUTE_CONFIRMED_ACTIVE"
                                 : "SESSION_ROUTE_ACTIVE_CHECK_FAILED");
  return result;
}

DWORD WINAPI write_load_report(void *parameter) {
#ifdef NBA2K27_STARTUP_MOVIE_TEST
  (void)parameter;
  return startup_movie_test::worker(&has_anti_cheat_module);
#endif
#ifdef NBA2K27_COACH_CLONE_AUDIT
  (void)parameter;
  return coach_clone_audit::worker(&has_anti_cheat_module);
#endif
#ifdef NBA2K27_SESSION_OUTFITS
  HANDLE outfits=CreateThread(nullptr,0,[](void *)->DWORD {
    const auto base=reinterpret_cast<std::uintptr_t>(GetModuleHandleW(nullptr));
    if(character_queue_sep11::exact_build(base))
      return character_description_clone_sep11::worker(&has_anti_cheat_module);
    return character_description_clone_audit::worker(&has_anti_cheat_module);
  },nullptr,0,nullptr);
  if(outfits)CloseHandle(outfits);
#ifdef NBA2K27_SESSION_OUTFITS_ONLY
  return 0; // Isolated automatic-outfit test: no provider/configuration side effects.
#endif
#endif
#ifdef NBA2K27_DESCRIPTION_CLONE_AUDIT
  (void)parameter;
#ifdef NBA2K27_DESCRIPTION_CLONE_SEP11
  return character_description_clone_sep11::worker(&has_anti_cheat_module);
#else
  return character_description_clone_audit::worker(&has_anti_cheat_module);
#endif
#endif
#ifdef NBA2K27_CHARACTER_QUEUE_NOOP_TEST
  return character_queue_noop::worker(reinterpret_cast<HMODULE>(parameter), &has_anti_cheat_module);
#endif
  const auto proxy_module = reinterpret_cast<HMODULE>(parameter);
  wchar_t executable_path[32768] = {};
  wchar_t proxy_path[32768] = {};
  if (GetModuleFileNameW(nullptr, executable_path, ARRAYSIZE(executable_path)) == 0 ||
      GetModuleFileNameW(proxy_module, proxy_path, ARRAYSIZE(proxy_path)) == 0) {
    return 1;
  }

  wchar_t log_path[32768] = {};
  wchar_t armed_path[32768] = {};
  wchar_t trigger_path[32768] = {};
  wchar_t trace_path[32768] = {};
  wchar_t provider_config_path[32768] = {};
  if (!make_sibling_path(executable_path, kLogName, log_path,
                         ARRAYSIZE(log_path)) ||
      !make_sibling_path(executable_path, kArmedName, armed_path,
                         ARRAYSIZE(armed_path)) ||
      !make_sibling_path(executable_path, kTriggerName, trigger_path,
                         ARRAYSIZE(trigger_path)) ||
      !make_sibling_path(executable_path, kTraceName, trace_path,
                         ARRAYSIZE(trace_path)) ||
      !make_sibling_path(executable_path, kProviderConfigName,
                         provider_config_path,
                         ARRAYSIZE(provider_config_path))) {
    return 2;
  }

  char executable_utf8[32768] = {};
  char proxy_utf8[32768] = {};
  WideCharToMultiByte(CP_UTF8, 0, executable_path, -1, executable_utf8,
                      static_cast<int>(sizeof(executable_utf8)), nullptr, nullptr);
  WideCharToMultiByte(CP_UTF8, 0, proxy_path, -1, proxy_utf8,
                      static_cast<int>(sizeof(proxy_utf8)), nullptr, nullptr);

  nba2k27::ResolverReport resolver = {};
  const bool resolved =
      nba2k27::resolve_provider_layout(executable_path, &resolver);
  const bool anti_cheat_present = has_anti_cheat_module();
  RegistrationAudit audit = {};
  if (resolved && resolver.complete && !anti_cheat_present) {
    char armed_report[4096] = {};
    std::size_t armed_used = 0;
    append(armed_report, sizeof(armed_report), &armed_used,
           "SCHEMA=%s\r\nPID=%lu\r\nTARGET=%s\r\nPROXY=%s\r\n"
           "IDENTITY=PASS\r\nEXE_TIMESTAMP=0x%08X\r\n"
           "IMAGE_SIZE=0x%08X\r\nANTI_CHEAT=ABSENT\r\n"
           "STRUCTURAL_RESOLVER=PASS\r\nAUDIT_STATE=ARMED\r\n"
           "GAME_MEMORY_WRITES=0\r\nGAME_CODE_HOOKS=0\r\n"
           "PROVIDER_REGISTRATION=OFF\r\nASSET_REPLACEMENT=OFF\r\n",
           kSchema, GetCurrentProcessId(), executable_utf8, proxy_utf8,
           resolver.timestamp, resolver.image_size);
    if (!write_text_file(armed_path, armed_report)) return 3;

    bool triggered = false;
    for (std::uint32_t attempt = 0; attempt < 600; ++attempt) {
      if (GetFileAttributesW(trigger_path) != INVALID_FILE_ATTRIBUTES) {
        triggered = true;
        break;
      }
      Sleep(1000);
    }
    if (!triggered) return 0;
    DeleteFileW(trigger_path);
    trace_checkpoint(trace_path, "MANUAL_TRIGGER_DETECTED");
    audit = run_registration_audit(
        resolver, has_anti_cheat_module(), trace_path,
        provider_config_path);
  } else {
    audit = run_registration_audit(
        resolver, anti_cheat_present, trace_path, provider_config_path);
  }
  LiveRouteResult live = {};
  if (audit.passed) {
    live = run_live_route(resolver, trace_path);
  } else {
    live.failure = "prerequisite real-asset stream audit failed";
  }
  if(live.session_active) {
    outfit_match_v2::bridge::start(proxy_module,&has_anti_cheat_module);
    // Dormant QA source test. An exact-session approval file is mandatory
    // before this watcher can open native authority or call the WAD API.
    wnba_priority_v2::bridge::start(proxy_module,&has_anti_cheat_module);
    roster_appearance_v2::bridge::start(proxy_module,&has_anti_cheat_module);
  }
  char report[24576] = {};
  std::size_t used = 0;
  append(report, sizeof(report), &used,
         "SCHEMA=%s\r\nPID=%lu\r\nTARGET=%s\r\nPROXY=%s\r\n"
         "IDENTITY=%s\r\nEXE_TIMESTAMP=0x%08X\r\nIMAGE_SIZE=0x%08X\r\n"
         "ANTI_CHEAT=%s\r\nSYSTEM_VERSION_FORWARDING=ON\r\n"
         "GAME_MEMORY_WRITES=%s\r\nGAME_CODE_HOOKS=0\r\n"
         "PROVIDER_REGISTRATION=%s\r\nASSET_REPLACEMENT=%s\r\n"
         "READONLY_FILE_ANALYSIS=ON\r\n"
         "STRUCTURAL_RESOLVER=%s\r\nRESOLVER_FAILURE=%s\r\n"
         "MANAGER_ANCHOR_MATCHES=%u\r\nMANAGER_CLUSTER_CANDIDATES=%u\r\n"
         "BASE_ANCHOR_MATCHES=%u\r\nBASE_CLUSTER_CANDIDATES=%u\r\n"
         "PATH_CANDIDATES=%u\r\nREFERENCE_RELEASE_CANDIDATES=%u\r\n"
         "EXECUTABLE_VTABLE_TARGETS=%u/%zu\r\n",
         kSchema, GetCurrentProcessId(), executable_utf8, proxy_utf8,
         resolver.image_parsed ? "PASS" : "FAIL", resolver.timestamp,
         resolver.image_size,
         anti_cheat_present ? "PRESENT" : "ABSENT",
         live.session_active
             ? "SESSION_PROVIDER_REGISTRATION_AND_DEFAULT_SELECTION"
             : (audit.register_called
                    ? "REVERSIBLE_MANAGER_REGISTRATION_ONLY"
                    : "0"),
         live.session_active ? "ACTIVE" : "OFF",
         live.session_active
             ? (g_forced_override_path[0] != '\0' ||
                    g_forced_override_path2[0] != '\0' ||
                    g_forced_override_path3[0] != '\0' ||
                    g_forced_override_path4[0] != '\0' ||
                    g_audio_index_override || g_audio_slot_count != 0
                    ? "EXPLICIT_ALLOWLIST_PLUS_MISSING_FALLBACK"
                    : "STOCK_FIRST_MISSING_ONLY")
             : "OFF",
         resolved && resolver.complete ? "PASS" : "FAIL", resolver.failure,
         resolver.manager_anchor_matches, resolver.manager_cluster_candidates,
         resolver.base_anchor_matches, resolver.base_cluster_candidates,
         resolver.path_candidates, resolver.reference_release_candidates,
         resolver.executable_vtable_targets, nba2k27::kBaseVtableSlots);

  append(report, sizeof(report), &used,
         "REGISTRATION_AUDIT=%s\r\nAUDIT_FAILURE=%s\r\n"
         "RUNTIME_IDENTITY_MATCH=%s\r\nMANAGER_READY=%s\r\n"
         "DEFAULT_BEFORE=0x%08X\r\nDEFAULT_AFTER=0x%08X\r\n"
         "AUDIT_DEVICE_ID=0x%08X\r\nAUDIT_DEVICE_ID_AVAILABLE=%s\r\n"
         "BASE_CONSTRUCTOR=%s\r\nCONSTRUCTOR_SELF_LINKED=%s\r\n"
         "SET_ID_STATUS=0x%08X\r\nDEVICE_ID_SET=%s\r\n"
         "PRIVATE_VTABLE_VERIFIED=%s\r\nGET_ID_SLOT=%zu\r\n"
         "EXISTS_SLOT=%zu\r\nFILE_INFO_SLOT=%zu\r\n"
         "ACTUAL_OPEN_SLOT=%zu\r\nTRANSFER_SLOT=%zu\r\nCLOSE_SLOT=%zu\r\n"
         "STOCK_EXISTS_INHERITED=%s\r\n"
         "REGISTER_STATUS=0x%08X\r\nREGISTERED_LINKS=%s\r\n"
         "MANAGER_FIND_DURING=%s\r\nFIND_REFERENCE_RELEASED=%s\r\n"
         "BASE_DESTRUCTOR_CALLED=%s\r\nSELF_LINKED_AFTER=%s\r\n"
         "MANAGER_FIND_AFTER=%s\r\nDEFAULT_UNCHANGED=%s\r\n"
         "GET_ID_CALLBACK_CALLS=%u\r\nCALLBACKS_QUIESCED=%s\r\n"
         "FILE_INFO_CALLBACK_CALLS=%u\r\nEXISTS_TARGET_RESULT=%s\r\n"
         "EXISTS_MISS_REJECTED=%s\r\nEXISTS_FILE_INFO_CALLS=%u\r\n"
         "FILE_HANDLE_CONSTRUCTED=%s\r\n"
         "MANAGER_OPEN_STATUS=0x%08X\r\n"
         "ACTUAL_OPEN_CALLS=%u\r\nACTUAL_OPEN_SUCCESSES=%u\r\n"
         "READ_STATUS=0x%08X\r\nTRANSFER_CALLS=%u\r\n"
         "PROVIDER_CONFIGURED=%s\r\nASSET_PATH=%s\r\n"
         "ASSET_SIZE=%llu\r\nPROVIDER_QUERY_CALLS=%u\r\n"
         "PROVIDER_READ_CALLS=%u\r\nPROVIDER_BYTES=%llu\r\n"
         "BYTES_READ=%llu\r\nSHA256_MATCH=%s\r\n"
         "CLOSE_STATUS=0x%08X\r\nCLOSE_CALLS=%u\r\n"
         "RESOURCE_HANDLES=%u\r\n",
         audit.passed ? "PASS" : "FAIL", audit.failure,
         audit.runtime_identity_matches ? "PASS" : "FAIL",
         audit.manager_ready ? "PASS" : "FAIL", audit.default_before,
         audit.default_after, kAuditDeviceId,
         audit.audit_id_available ? "PASS" : "FAIL",
         audit.constructed ? "PASS" : "FAIL",
         audit.constructor_self_linked ? "PASS" : "FAIL",
         audit.set_id_status, audit.id_set ? "PASS" : "FAIL",
         audit.private_vtable_verified ? "PASS" : "FAIL", kGetIdSlot,
         kExistsSlot, kFileInfoSlot,
         kActualOpenSlot, kTransferSlot, kCloseSlot,
         audit.stock_exists_inherited ? "PASS" : "FAIL",
         audit.registration_status, audit.registered ? "PASS" : "FAIL",
         audit.find_during ? "PASS" : "FAIL",
         audit.find_during_released ? "PASS" : "FAIL",
         audit.destructor_called ? "YES" : "NO",
         audit.self_linked_after ? "PASS" : "FAIL",
         !audit.destructor_called
             ? "NOT_RUN"
             : (audit.absent_after ? "ABSENT" : "STILL PRESENT"),
         audit.default_unchanged ? "PASS" : "FAIL", audit.get_id_calls,
         audit.callbacks_quiesced ? "PASS" : "FAIL",
         audit.file_info_calls, audit.exists_hit ? "PRESENT" : "ABSENT",
         audit.exists_miss_rejected ? "PASS" : "FAIL",
         audit.exists_file_info_calls,
         audit.file_handle_constructed ? "PASS" : "FAIL",
         audit.manager_open_status, audit.actual_open_calls,
         audit.actual_open_successes, audit.read_status,
         audit.transfer_calls,
         audit.provider_configured ? "PASS" : "FAIL",
         audit.provider_configured ? g_provider_path.data() : "NOT_CONFIGURED",
         static_cast<unsigned long long>(audit.asset_size),
         audit.provider_query_calls, audit.provider_read_calls,
         static_cast<unsigned long long>(audit.provider_bytes),
         static_cast<unsigned long long>(audit.bytes_read),
         audit.payload_match ? "MATCH" : "MISMATCH", audit.close_status,
         audit.close_calls, audit.resource_handles);

  append(report, sizeof(report), &used,
         "LIVE_MISSING_ASSET_ROUTE=%s\r\nLIVE_FAILURE=%s\r\n"
         "LIVE_STARTED=%s\r\nLIVE_SESSION_ACTIVE=%s\r\n"
         "LIVE_COUNTER_SNAPSHOT=AT_ACTIVATION\r\n"
         "LIVE_TARGET_READ=%s\r\nLIVE_TARGET_COMPLETED=%s\r\n"
         "LIVE_TIMED_OUT=%s\r\nLIVE_DEFAULT_BEFORE=0x%08X\r\n"
         "LIVE_DEFAULT_DURING=0x%08X\r\n"
         "LIVE_DEFAULT_AFTER=0x%08X\r\n"
         "LIVE_DEFAULT_RESTORED=%s\r\n"
         "LIVE_REGISTRATION_STATUS=0x%08X\r\n"
         "LIVE_UNREGISTERED=%s\r\nLIVE_CALLBACKS_QUIESCED=%s\r\n"
         "LIVE_HANDLES_RELEASED=%s\r\n"
         "LIVE_FILE_INFO_CALLS=%u\r\n"
         "LIVE_STOCK_FILE_INFO_SUCCESSES=%u\r\n"
         "LIVE_LOCAL_FILE_INFO_SUCCESSES=%u\r\n"
         "LIVE_ACTUAL_OPEN_CALLS=%u\r\n"
         "LIVE_STOCK_OPEN_SUCCESSES=%u\r\n"
         "LIVE_LOCAL_OPEN_SUCCESSES=%u\r\n"
         "LIVE_TRANSFER_CALLS=%u\r\nLIVE_CLOSE_CALLS=%u\r\n"
         "LIVE_BINDINGS=%u/%zu\r\nLIVE_BINDING_RELEASES=%u\r\n"
         "LIVE_BINDING_PEAK=%u\r\nLIVE_BINDING_ACTIVE=%u\r\n"
         "LIVE_PROVIDER_QUERY_CALLS=%u\r\n"
         "LIVE_PROVIDER_READ_CALLS=%u\r\n"
         "LIVE_PROVIDER_BYTES=%llu\r\n"
         "LIVE_FORCED_OVERRIDE=%s\r\n",
         live.session_active ? "ACTIVE" : (live.passed ? "PASS" : "FAIL"),
         live.failure,
         live.started ? "YES" : "NO",
         live.session_active ? "YES" : "NO",
         live.target_read ? "YES" : "NO",
         live.target_completed ? "YES" : "NO",
         live.timed_out ? "YES" : "NO", live.default_before,
         live.default_during, live.default_after,
         live.session_active
             ? "DEFERRED_UNTIL_GAME_EXIT"
             : (live.default_restored ? "PASS" : "FAIL"),
         live.registration_status,
         live.session_active
             ? "DEFERRED_UNTIL_GAME_EXIT"
             : (live.unregistered ? "YES" : "NO"),
         live.callbacks_quiesced ? "PASS" : "FAIL",
         live.session_active
             ? "CACHE_HANDLES_ALLOWED"
             : (live.handles_released ? "PASS" : "FAIL"),
         live.file_info_calls, live.stock_file_info_successes,
         live.local_file_info_successes, live.actual_open_calls,
         live.stock_open_successes, live.local_open_successes,
         live.transfer_calls, live.close_calls,
         live.binding_allocations, kMaxHandleBindings,
         live.binding_releases, live.binding_peak, live.binding_active,
         live.provider_query_calls, live.provider_read_calls,
         static_cast<unsigned long long>(live.provider_bytes),
         g_forced_override_path[0] != '\0'
             ? g_forced_override_path.data()
             : "NONE");
  append(report, sizeof(report), &used, "LIVE_AUDIO_INDEX_OVERRIDE=%s\r\n",
         g_audio_index_override ? nba2k27::kJukeboxIndexPath : "NONE");
  append(report, sizeof(report), &used, "LIVE_FORCED_OVERRIDE_2=%s\r\n",
         g_forced_override_path2[0] != '\0'
             ? g_forced_override_path2.data()
             : "NONE");
  append(report, sizeof(report), &used, "LIVE_FORCED_OVERRIDE_3=%s\r\n",
         g_forced_override_path3[0] != '\0'
             ? g_forced_override_path3.data()
             : "NONE");
  append(report, sizeof(report), &used, "LIVE_FORCED_OVERRIDE_4=%s\r\n",
         g_forced_override_path4[0] != '\0' ? g_forced_override_path4.data() : "NONE");
  append(report, sizeof(report), &used, "LIVE_AUDIO_SLOT_COUNT=%u\r\n", g_audio_slot_count);
  append(report, sizeof(report), &used, "LIVE_SELECTOR_OVERRIDE=%s\r\n", g_selector_override ? "ON" : "OFF");
  append(report,sizeof(report),&used,"LIVE_PLAYLIST_FILTER=%s\r\n",g_audio_main_mask[0]?"ON":"OFF");
  append(report,sizeof(report),&used,"FREESTYLE_EVENT_TEST=%s\r\nFREESTYLE_WINDOW_SECONDS=600\r\nFREESTYLE_REDIRECT_OPENS=%ld\r\nFREESTYLE_REDIRECT_FAILURES=%ld\r\n",
         g_freestyle_test?"992":"OFF",InterlockedCompareExchange(&g_freestyle_opens,0,0),InterlockedCompareExchange(&g_freestyle_failures,0,0));
  append(report,sizeof(report),&used,"FREESTYLE_PLAYLIST=%s\r\nFREESTYLE_PLAYLIST_LIFETIME=%s\r\n",g_freestyle_playlist?"ON":"OFF",g_freestyle_playlist?"GAME_SESSION":"DISABLED");
  if(g_freestyle_playlist)append(report,sizeof(report),&used,"FREESTYLE_SELECTION=%s\r\n",g_freestyle_selection.data());

  const auto &layout = resolver.layout;
  append(report, sizeof(report), &used,
         "RVA_MANAGER_GETTER=0x%08X\r\n"
         "RVA_MANAGER_REGISTER=0x%08X\r\n"
         "RVA_MANAGER_FIND=0x%08X\r\n"
         "RVA_MANAGER_SELECT=0x%08X\r\n"
         "RVA_REFERENCE_RELEASE=0x%08X\r\n"
         "RVA_BASE_CONSTRUCTOR=0x%08X\r\n"
         "RVA_SET_DEVICE_ID=0x%08X\r\n"
         "RVA_BASE_DESTRUCTOR=0x%08X\r\n"
         "RVA_BASE_VTABLE=0x%08X\r\n"
         "RVA_PATH_BUILDER=0x%08X\r\n"
         "RVA_RESOURCE_EXISTS_BY_ID=0x%08X\r\n"
         "RVA_BASE_EXISTS=0x%08X\r\n"
         "RVA_BASE_EXISTS_OPEN_CALL=0x%08X\r\n"
         "RVA_OPEN_RESULT_CONSTRUCTOR=0x%08X\r\n"
         "RVA_MANAGER_OPEN=0x%08X\r\n"
         "RVA_MANAGER_OPEN_DEVICE_CALL=0x%08X\r\n"
         "RVA_FILE_HANDLE_CONSTRUCTOR=0x%08X\r\n"
         "RVA_FILE_HANDLE_INITIALIZE=0x%08X\r\n"
         "RVA_FILE_HANDLE_READ=0x%08X\r\n"
         "RVA_FILE_HANDLE_DESTRUCTOR=0x%08X\r\n",
         layout.manager_getter, layout.manager_register, layout.manager_find,
         layout.manager_select, layout.reference_release,
         layout.base_constructor, layout.set_device_id, layout.base_destructor,
         layout.base_vtable, layout.path_builder,
         layout.resource_exists_by_id, layout.base_exists,
         layout.base_exists_open_call, layout.open_result_constructor,
         layout.manager_open, layout.manager_open_device_call,
         layout.file_handle_constructor, layout.file_handle_initialize,
         layout.file_handle_read, layout.file_handle_destructor);
  for (std::size_t index = 0; index < nba2k27::kBaseVtableSlots; ++index) {
    append(report, sizeof(report), &used, "VTABLE_SLOT_%02zu=0x%08X\r\n",
           index, layout.base_vtable_targets[index]);
  }

  const HANDLE file = CreateFileW(log_path, GENERIC_WRITE,
                                  FILE_SHARE_READ | FILE_SHARE_WRITE |
                                      FILE_SHARE_DELETE,
                                  nullptr, CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL,
                                  nullptr);
  if (file == INVALID_HANDLE_VALUE) return 5;
  DWORD written = 0;
  const DWORD requested = static_cast<DWORD>(used);
  const BOOL ok = WriteFile(file, report, requested, &written, nullptr);
  CloseHandle(file);
  return ok != FALSE && written == requested ? 0 : 6;
}

using VerInstallFileAProc = DWORD(WINAPI *)(DWORD, LPSTR, LPSTR, LPSTR, LPSTR,
                                            LPSTR, LPSTR, PUINT);
using VerInstallFileWProc = DWORD(WINAPI *)(DWORD, LPWSTR, LPWSTR, LPWSTR,
                                            LPWSTR, LPWSTR, LPWSTR, PUINT);

INIT_ONCE g_version_once = INIT_ONCE_STATIC_INIT;
HMODULE g_system_version = nullptr;
VerInstallFileAProc g_ver_install_file_a = nullptr;
VerInstallFileWProc g_ver_install_file_w = nullptr;

BOOL CALLBACK load_system_version(PINIT_ONCE, PVOID, PVOID *) {
  wchar_t system_path[MAX_PATH] = {};
  const UINT length = GetSystemDirectoryW(system_path, ARRAYSIZE(system_path));
  if (length == 0 || length >= ARRAYSIZE(system_path) - 13) return FALSE;
  if (system_path[length - 1] != L'\\') {
    system_path[length] = L'\\';
    system_path[length + 1] = L'\0';
  }
  if (wcscat_s(system_path, L"version.dll") != 0) return FALSE;
  g_system_version = LoadLibraryW(system_path);
  if (g_system_version == nullptr) return FALSE;
  g_ver_install_file_a = reinterpret_cast<VerInstallFileAProc>(
      GetProcAddress(g_system_version, "VerInstallFileA"));
  g_ver_install_file_w = reinterpret_cast<VerInstallFileWProc>(
      GetProcAddress(g_system_version, "VerInstallFileW"));
  return g_ver_install_file_a != nullptr && g_ver_install_file_w != nullptr;
}

bool ensure_system_version() {
  return InitOnceExecuteOnce(&g_version_once, load_system_version, nullptr,
                             nullptr) != FALSE;
}

}  // namespace

extern "C" DWORD WINAPI ProxyVerInstallFileA(
    DWORD flags, LPSTR source_name, LPSTR destination_name, LPSTR source_dir,
    LPSTR destination_dir, LPSTR current_dir, LPSTR temporary_file,
    PUINT temporary_file_length) {
  if (!ensure_system_version()) return VIF_CANNOTREADSRC;
  return g_ver_install_file_a(flags, source_name, destination_name, source_dir,
                              destination_dir, current_dir, temporary_file,
                              temporary_file_length);
}

extern "C" DWORD WINAPI ProxyVerInstallFileW(
    DWORD flags, LPWSTR source_name, LPWSTR destination_name, LPWSTR source_dir,
    LPWSTR destination_dir, LPWSTR current_dir, LPWSTR temporary_file,
    PUINT temporary_file_length) {
  if (!ensure_system_version()) return VIF_CANNOTREADSRC;
  return g_ver_install_file_w(flags, source_name, destination_name, source_dir,
                              destination_dir, current_dir, temporary_file,
                              temporary_file_length);
}

extern "C" BOOL WINAPI DllMain(HMODULE module, DWORD reason, LPVOID) {
  if (reason == DLL_PROCESS_ATTACH) {
    DisableThreadLibraryCalls(module);
    const HANDLE thread =
        CreateThread(nullptr, 0, &write_load_report, module, 0, nullptr);
    if (thread != nullptr) CloseHandle(thread);
  }
  return TRUE;
}


