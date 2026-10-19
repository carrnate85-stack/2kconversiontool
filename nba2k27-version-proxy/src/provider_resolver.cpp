#include "provider_resolver.hpp"

#include <Windows.h>
#include <winnt.h>

#include <cstddef>
#include <cstdint>
#include <cstring>

namespace nba2k27 {
namespace {

struct Signature {
  const std::uint8_t *bytes;
  const char *mask;
  std::size_t size;
};

constexpr std::uint8_t kManagerGetterBytes[] = {
    0x55, 0x56, 0x48, 0x83, 0xEC, 0x58, 0x48, 0x8D,
    0x6C, 0x24, 0x50, 0x48, 0xC7, 0x45, 0x00, 0xFE};
constexpr std::uint8_t kManagerRegisterBytes[] = {
    0x55, 0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54,
    0x56, 0x57, 0x53, 0x48, 0x81, 0xEC, 0x98, 0x00, 0x00};
constexpr std::uint8_t kManagerFindBytes[] = {
    0x55, 0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83,
    0xEC, 0x40, 0x48, 0x8D, 0x6C, 0x24, 0x40, 0x48};
constexpr std::uint8_t kManagerSelectBytes[] = {
    0x41, 0x56, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83,
    0xEC, 0x20, 0x4C, 0x89, 0xCF, 0x4C, 0x89, 0xC6};
constexpr std::uint8_t kReferenceReleaseBytes[] = {
    0x56, 0x57, 0x48, 0x83, 0xEC, 0x28, 0x48, 0x89,
    0xCE, 0xE8, 0x00, 0x00, 0x00, 0x00, 0x39, 0x46};
constexpr std::uint8_t kBaseConstructorBytes[] = {
    0x56, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xCE,
    0x48, 0x8D, 0x05, 0x00, 0x00, 0x00, 0x00, 0x48};
constexpr std::uint8_t kSetDeviceIdBytes[] = {
    0x48, 0x89, 0xD0, 0x45, 0x85, 0xC0, 0x74, 0x0C,
    0x44, 0x89, 0x41, 0x40, 0xB9, 0xA8, 0x21, 0x45,
    0x50, 0x89, 0x08, 0xC3};
constexpr std::uint8_t kBaseDestructorBytes[] = {
    0x55, 0x56, 0x48, 0x83, 0xEC, 0x28, 0x48, 0x8D,
    0x6C, 0x24, 0x20, 0x48, 0xC7, 0x45, 0x00, 0xFE};
constexpr std::uint8_t kPathBuilderBytes[] = {
    0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54,
    0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x38,
    0x45, 0x89, 0xCE, 0x44, 0x89, 0xC5, 0x89, 0xD3,
    0x8B, 0xBC, 0x24, 0xA8, 0x00, 0x00, 0x00, 0x48,
    0x8B, 0xB4, 0x24, 0xA0, 0x00, 0x00, 0x00, 0x83,
    0xFA, 0x01};
constexpr std::uint8_t kResourceExistsBytes[] = {
    0x55, 0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56,
    0x57, 0x53, 0x48, 0x81, 0xEC, 0xF0, 0x00, 0x00};
constexpr std::uint8_t kBaseExistsBytes[] = {
    0x56, 0x57, 0x53, 0x48, 0x81, 0xEC, 0x60, 0x01,
    0x00, 0x00, 0x48, 0x89, 0xD6, 0x48, 0x85, 0xC9};
constexpr std::uint8_t kBaseExistsOpenCallBytes[] = {
    0xFF, 0x90, 0x08, 0x01, 0x00, 0x00, 0x81,
    0x7C, 0x24, 0x2C, 0xA8, 0x21, 0x45, 0x50};
constexpr std::uint8_t kOpenResultConstructorBytes[] = {
    0x48, 0xC7, 0x01, 0x00, 0x00, 0x00, 0x00, 0xC5,
    0xF8, 0x57, 0xC0, 0xC5, 0xFC, 0x11, 0x41, 0x18};
constexpr std::uint8_t kManagerOpenBytes[] = {
    0x55, 0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41,
    0x54, 0x56, 0x57, 0x53, 0x48, 0x81, 0xEC, 0x68};
constexpr std::uint8_t kManagerOpenDeviceCallBytes[] = {
    0x48, 0x8B, 0x80, 0x18, 0x01, 0x00, 0x00,
    0x44, 0x89, 0x6C, 0x24, 0x30, 0x8B};
constexpr std::uint8_t kFileHandleConstructorBytes[] = {
    0x56, 0x48, 0x83, 0xEC, 0x30, 0xC5, 0xF8, 0x29,
    0x74, 0x24, 0x20, 0x48, 0x89, 0xCE, 0x48, 0xC7};
constexpr std::uint8_t kFileHandleInitializeBytes[] = {
    0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xEC,
    0x28, 0x4C, 0x89, 0xCE, 0x4C, 0x89, 0xC3, 0x49};
constexpr std::uint8_t kFileHandleReadBytes[] = {
    0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54,
    0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x58};
constexpr std::uint8_t kFileHandleDestructorBytes[] = {
    0x55, 0x56, 0x48, 0x83, 0xEC, 0x38, 0x48, 0x8D,
    0x6C, 0x24, 0x30, 0x48, 0xC7, 0x45, 0x00, 0xFE};

constexpr Signature exact(const std::uint8_t *bytes, std::size_t size) {
  return {bytes, nullptr, size};
}

constexpr Signature kManagerGetter = exact(kManagerGetterBytes, sizeof(kManagerGetterBytes));
constexpr Signature kManagerRegister = exact(kManagerRegisterBytes, sizeof(kManagerRegisterBytes));
constexpr Signature kManagerFind = exact(kManagerFindBytes, sizeof(kManagerFindBytes));
constexpr Signature kManagerSelect = exact(kManagerSelectBytes, sizeof(kManagerSelectBytes));
constexpr Signature kReferenceRelease = {kReferenceReleaseBytes, "xxxxxxxxxx????xx", sizeof(kReferenceReleaseBytes)};
constexpr Signature kBaseConstructor = {kBaseConstructorBytes, "xxxxxxxxxxx????x", sizeof(kBaseConstructorBytes)};
constexpr Signature kSetDeviceId = exact(kSetDeviceIdBytes, sizeof(kSetDeviceIdBytes));
constexpr Signature kBaseDestructor = exact(kBaseDestructorBytes, sizeof(kBaseDestructorBytes));
constexpr Signature kPathBuilder = exact(kPathBuilderBytes, sizeof(kPathBuilderBytes));
constexpr Signature kResourceExists = exact(kResourceExistsBytes, sizeof(kResourceExistsBytes));
constexpr Signature kBaseExists = exact(kBaseExistsBytes, sizeof(kBaseExistsBytes));
constexpr Signature kBaseExistsOpenCall = exact(kBaseExistsOpenCallBytes, sizeof(kBaseExistsOpenCallBytes));
constexpr Signature kOpenResultConstructor = exact(kOpenResultConstructorBytes, sizeof(kOpenResultConstructorBytes));
constexpr Signature kManagerOpen = exact(kManagerOpenBytes, sizeof(kManagerOpenBytes));
constexpr Signature kManagerOpenDeviceCall = exact(kManagerOpenDeviceCallBytes, sizeof(kManagerOpenDeviceCallBytes));
constexpr Signature kFileHandleConstructor = exact(kFileHandleConstructorBytes, sizeof(kFileHandleConstructorBytes));
constexpr Signature kFileHandleInitialize = exact(kFileHandleInitializeBytes, sizeof(kFileHandleInitializeBytes));
constexpr Signature kFileHandleRead = exact(kFileHandleReadBytes, sizeof(kFileHandleReadBytes));
constexpr Signature kFileHandleDestructor = exact(kFileHandleDestructorBytes, sizeof(kFileHandleDestructorBytes));

class ExecutableFile {
 public:
  ~ExecutableFile() { close(); }

  bool open(const wchar_t *path) {
    file_ = CreateFileW(path, GENERIC_READ,
                        FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                        nullptr, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, nullptr);
    if (file_ == INVALID_HANDLE_VALUE) return false;
    LARGE_INTEGER size = {};
    if (GetFileSizeEx(file_, &size) == FALSE || size.QuadPart < 4096) return false;
    file_size_ = static_cast<std::uint64_t>(size.QuadPart);
    mapping_ = CreateFileMappingW(file_, nullptr, PAGE_READONLY, 0, 0, nullptr);
    if (mapping_ == nullptr) return false;
    base_ = static_cast<const std::uint8_t *>(
        MapViewOfFile(mapping_, FILE_MAP_READ, 0, 0, 0));
    if (base_ == nullptr) return false;
    if (file_size_ < sizeof(IMAGE_DOS_HEADER)) return false;
    const auto dos = reinterpret_cast<const IMAGE_DOS_HEADER *>(base_);
    if (dos->e_magic != IMAGE_DOS_SIGNATURE || dos->e_lfanew < 0) return false;
    const auto nt_offset = static_cast<std::uint64_t>(dos->e_lfanew);
    if (nt_offset + sizeof(IMAGE_NT_HEADERS64) > file_size_) return false;
    nt_ = reinterpret_cast<const IMAGE_NT_HEADERS64 *>(base_ + nt_offset);
    if (nt_->Signature != IMAGE_NT_SIGNATURE ||
        nt_->OptionalHeader.Magic != IMAGE_NT_OPTIONAL_HDR64_MAGIC) return false;
    sections_ = IMAGE_FIRST_SECTION(nt_);
    const auto section_offset =
        static_cast<std::uint64_t>(reinterpret_cast<const std::uint8_t *>(sections_) - base_);
    if (section_offset +
            static_cast<std::uint64_t>(nt_->FileHeader.NumberOfSections) *
                sizeof(IMAGE_SECTION_HEADER) >
        file_size_) return false;
    return true;
  }

  void close() {
    if (base_ != nullptr) UnmapViewOfFile(base_);
    if (mapping_ != nullptr) CloseHandle(mapping_);
    if (file_ != INVALID_HANDLE_VALUE) CloseHandle(file_);
    base_ = nullptr;
    mapping_ = nullptr;
    file_ = INVALID_HANDLE_VALUE;
  }

  std::uint32_t timestamp() const { return nt_->FileHeader.TimeDateStamp; }
  std::uint32_t image_size() const { return nt_->OptionalHeader.SizeOfImage; }
  std::uint64_t image_base() const { return nt_->OptionalHeader.ImageBase; }

  const std::uint8_t *at_rva(std::uint32_t rva, std::size_t bytes) const {
    if (rva < nt_->OptionalHeader.SizeOfHeaders &&
        static_cast<std::uint64_t>(rva) + bytes <= file_size_) {
      return base_ + rva;
    }
    for (std::uint16_t index = 0; index < nt_->FileHeader.NumberOfSections; ++index) {
      const auto &section = sections_[index];
      const std::uint64_t start = section.VirtualAddress;
      const std::uint64_t end = start + section.SizeOfRawData;
      if (rva < start || static_cast<std::uint64_t>(rva) + bytes > end) continue;
      const std::uint64_t raw = section.PointerToRawData + (rva - start);
      if (raw + bytes > file_size_) return nullptr;
      return base_ + raw;
    }
    return nullptr;
  }

  bool is_executable_rva(std::uint32_t rva) const {
    for (std::uint16_t index = 0; index < nt_->FileHeader.NumberOfSections; ++index) {
      const auto &section = sections_[index];
      const std::uint64_t extent = section.Misc.VirtualSize > section.SizeOfRawData
                                       ? section.Misc.VirtualSize
                                       : section.SizeOfRawData;
      if (rva >= section.VirtualAddress &&
          static_cast<std::uint64_t>(rva) < section.VirtualAddress + extent) {
        return (section.Characteristics & IMAGE_SCN_MEM_EXECUTE) != 0;
      }
    }
    return false;
  }

  std::uint32_t scan(const Signature &signature, std::uint32_t *matches,
                     std::size_t capacity) const {
    std::uint32_t count = 0;
    for (std::uint16_t section_index = 0;
         section_index < nt_->FileHeader.NumberOfSections; ++section_index) {
      const auto &section = sections_[section_index];
      if ((section.Characteristics & IMAGE_SCN_MEM_EXECUTE) == 0 ||
          section.SizeOfRawData < signature.size) continue;
      const std::uint64_t raw = section.PointerToRawData;
      const std::uint64_t raw_size = section.SizeOfRawData;
      if (raw + raw_size > file_size_) continue;
      const auto bytes = base_ + raw;
      const auto last_start = raw_size - signature.size;
      std::uint64_t search_offset = 0;
      while (search_offset <= last_start) {
        const auto remaining =
            static_cast<std::size_t>(last_start - search_offset + 1);
        const auto candidate = static_cast<const std::uint8_t *>(
            std::memchr(bytes + search_offset, signature.bytes[0], remaining));
        if (candidate == nullptr) break;
        const auto offset = static_cast<std::uint64_t>(candidate - bytes);
        search_offset = offset + 1;
        if (!matches_signature(candidate, signature)) continue;
        if (count < capacity) {
          matches[count] = section.VirtualAddress +
                           static_cast<std::uint32_t>(offset);
        }
        ++count;
      }
    }
    return count;
  }

  bool matches_at(std::uint32_t rva, const Signature &signature) const {
    const auto bytes = at_rva(rva, signature.size);
    return bytes != nullptr && matches_signature(bytes, signature);
  }

 private:
  static bool matches_signature(const std::uint8_t *candidate,
                                const Signature &signature) {
    for (std::size_t index = 0; index < signature.size; ++index) {
      if (signature.mask != nullptr && signature.mask[index] == '?') continue;
      if (candidate[index] != signature.bytes[index]) return false;
    }
    return true;
  }

  HANDLE file_ = INVALID_HANDLE_VALUE;
  HANDLE mapping_ = nullptr;
  const std::uint8_t *base_ = nullptr;
  std::uint64_t file_size_ = 0;
  const IMAGE_NT_HEADERS64 *nt_ = nullptr;
  const IMAGE_SECTION_HEADER *sections_ = nullptr;
};

bool checked_sub(std::uint32_t value, std::uint32_t amount,
                 std::uint32_t *result) {
  if (value < amount) return false;
  *result = value - amount;
  return true;
}

bool decode_relative_call(const ExecutableFile &file, std::uint32_t call_rva,
                          std::uint32_t *target_rva) {
  const auto bytes = file.at_rva(call_rva, 5);
  if (bytes == nullptr || bytes[0] != 0xE8) return false;
  std::int32_t displacement = 0;
  std::memcpy(&displacement, bytes + 1, sizeof(displacement));
  const auto target = static_cast<std::int64_t>(call_rva) + 5 + displacement;
  if (target < 0 || target > UINT32_MAX) return false;
  *target_rva = static_cast<std::uint32_t>(target);
  return true;
}

bool resolve_manager_candidate(const ExecutableFile &file,
                               std::uint32_t manager_select,
                               ProviderLayout *layout) {
  ProviderLayout candidate = {};
  candidate.manager_select = manager_select;
  if (!checked_sub(manager_select, 0x1080, &candidate.manager_getter) ||
      !checked_sub(manager_select, 0x07C0, &candidate.manager_register) ||
      !checked_sub(manager_select, 0x02E0, &candidate.manager_find) ||
      !checked_sub(manager_select, 0x56E0, &candidate.file_handle_constructor) ||
      !checked_sub(manager_select, 0x5230, &candidate.file_handle_destructor) ||
      !checked_sub(manager_select, 0x4DB0, &candidate.file_handle_initialize) ||
      !checked_sub(manager_select, 0x4AC0, &candidate.file_handle_read)) {
    return false;
  }
  candidate.resource_exists_by_id = manager_select + 0x2990;
  candidate.manager_open = manager_select + 0x3B20;
  candidate.manager_open_device_call = manager_select + 0x444E;

  if (!file.matches_at(candidate.manager_getter, kManagerGetter) ||
      !file.matches_at(candidate.manager_register, kManagerRegister) ||
      !file.matches_at(candidate.manager_find, kManagerFind) ||
      !file.matches_at(candidate.manager_select, kManagerSelect) ||
      !file.matches_at(candidate.resource_exists_by_id, kResourceExists) ||
      !file.matches_at(candidate.manager_open, kManagerOpen) ||
      !file.matches_at(candidate.manager_open_device_call,
                       kManagerOpenDeviceCall) ||
      !file.matches_at(candidate.file_handle_constructor,
                       kFileHandleConstructor) ||
      !file.matches_at(candidate.file_handle_destructor,
                       kFileHandleDestructor) ||
      !file.matches_at(candidate.file_handle_initialize,
                       kFileHandleInitialize) ||
      !file.matches_at(candidate.file_handle_read, kFileHandleRead)) {
    return false;
  }

  std::uint32_t call_target = 0;
  if (!decode_relative_call(file, manager_select + 0x26, &call_target) ||
      call_target != candidate.manager_find ||
      !decode_relative_call(file, manager_select + 0xC3, &call_target) ||
      call_target != candidate.manager_find) {
    return false;
  }
  *layout = candidate;
  return true;
}

bool resolve_base_candidate(const ExecutableFile &file,
                            std::uint32_t set_device_id,
                            const ProviderLayout &manager_layout,
                            ProviderLayout *layout) {
  ProviderLayout candidate = *layout;
  candidate.set_device_id = set_device_id;
  if (!checked_sub(set_device_id, 0x580, &candidate.base_constructor) ||
      !checked_sub(set_device_id, 0x330, &candidate.base_destructor) ||
      !checked_sub(set_device_id, 0x3C0, &candidate.base_exists)) {
    return false;
  }
  candidate.base_exists_open_call = candidate.base_exists + 0x55;
  candidate.open_result_constructor = set_device_id + 0xF0;
  if (!file.matches_at(candidate.base_constructor, kBaseConstructor) ||
      !file.matches_at(candidate.base_destructor, kBaseDestructor) ||
      !file.matches_at(candidate.base_exists, kBaseExists) ||
      !file.matches_at(candidate.base_exists_open_call, kBaseExistsOpenCall) ||
      !file.matches_at(candidate.open_result_constructor,
                       kOpenResultConstructor)) {
    return false;
  }
  std::uint32_t call_target = 0;
  if (!decode_relative_call(file, candidate.base_destructor + 0x20,
                            &call_target) ||
      call_target != manager_layout.manager_getter) {
    return false;
  }
  const auto lea = file.at_rva(candidate.base_constructor + 0x08, 7);
  if (lea == nullptr || lea[0] != 0x48 || lea[1] != 0x8D || lea[2] != 0x05) {
    return false;
  }
  std::int32_t displacement = 0;
  std::memcpy(&displacement, lea + 3, sizeof(displacement));
  const auto vtable = static_cast<std::int64_t>(candidate.base_constructor) +
                      0x0F + displacement;
  if (vtable < 0 || vtable > UINT32_MAX) return false;
  candidate.base_vtable = static_cast<std::uint32_t>(vtable);
  *layout = candidate;
  return true;
}

bool validate_vtable(const ExecutableFile &file, ProviderLayout *layout,
                     std::uint32_t *executable_targets) {
  const auto table = file.at_rva(layout->base_vtable,
                                 kBaseVtableSlots * sizeof(std::uint64_t));
  if (table == nullptr) return false;
  std::uint32_t executable = 0;
  for (std::size_t index = 0; index < kBaseVtableSlots; ++index) {
    std::uint64_t address = 0;
    std::memcpy(&address, table + index * sizeof(address), sizeof(address));
    if (address < file.image_base() ||
        address - file.image_base() > UINT32_MAX) return false;
    const auto rva = static_cast<std::uint32_t>(address - file.image_base());
    layout->base_vtable_targets[index] = rva;
    if (!file.is_executable_rva(rva)) return false;
    ++executable;
  }
  *executable_targets = executable;
  return layout->base_vtable_targets[20] == layout->base_constructor + 0x50 &&
         layout->base_vtable_targets[32] == layout->base_exists &&
         layout->base_vtable_targets[35] == layout->base_constructor + 0xB0 &&
         layout->base_vtable_targets[42] == layout->base_constructor + 0x240 &&
         layout->base_vtable_targets[43] == layout->base_constructor + 0x1A0;
}

}  // namespace

bool resolve_provider_layout(const wchar_t *executable_path,
                             ResolverReport *report) {
  if (report == nullptr) return false;
  *report = {};
  report->failure = "unable to open or parse executable";
  ExecutableFile file;
  if (executable_path == nullptr || !file.open(executable_path)) return false;
  report->image_parsed = true;
  report->timestamp = file.timestamp();
  report->image_size = file.image_size();

  constexpr std::size_t kMaxAnchorMatches = 1024;
  std::uint32_t matches[kMaxAnchorMatches] = {};
  report->manager_anchor_matches =
      file.scan(kManagerSelect, matches, kMaxAnchorMatches);
  if (report->manager_anchor_matches > kMaxAnchorMatches) {
    report->failure = "too many manager anchors";
    return false;
  }
  for (std::uint32_t index = 0; index < report->manager_anchor_matches; ++index) {
    ProviderLayout candidate = {};
    if (!resolve_manager_candidate(file, matches[index], &candidate)) continue;
    ++report->manager_cluster_candidates;
    report->layout = candidate;
  }
  if (report->manager_cluster_candidates != 1) {
    report->failure = "manager cluster is ambiguous or missing";
    return false;
  }
  report->manager_cluster_resolved = true;

  report->base_anchor_matches = file.scan(kSetDeviceId, matches, kMaxAnchorMatches);
  if (report->base_anchor_matches > kMaxAnchorMatches) {
    report->failure = "too many base-device anchors";
    return false;
  }
  for (std::uint32_t index = 0; index < report->base_anchor_matches; ++index) {
    ProviderLayout candidate = report->layout;
    if (!resolve_base_candidate(file, matches[index], report->layout,
                                &candidate)) continue;
    ++report->base_cluster_candidates;
    report->layout = candidate;
  }
  if (report->base_cluster_candidates != 1) {
    report->failure = "base-device cluster is ambiguous or missing";
    return false;
  }
  report->base_cluster_resolved = true;

  report->path_candidates = file.scan(kPathBuilder, matches, kMaxAnchorMatches);
  if (report->path_candidates == 1) report->layout.path_builder = matches[0];
  report->reference_release_candidates =
      file.scan(kReferenceRelease, matches, kMaxAnchorMatches);
  if (report->reference_release_candidates == 1) {
    report->layout.reference_release = matches[0];
  }
  if (report->path_candidates != 1 ||
      report->reference_release_candidates != 1) {
    report->failure = "supporting anchors are ambiguous or missing";
    return false;
  }
  std::uint32_t call_target = 0;
  if (!decode_relative_call(file, report->layout.path_builder + 0xD3,
                            &call_target) ||
      call_target != report->layout.manager_getter ||
      !decode_relative_call(file, report->layout.path_builder + 0xDE,
                            &call_target) ||
      call_target + 0x10 != report->layout.resource_exists_by_id) {
    report->failure = "path-builder relationships did not validate";
    return false;
  }
  report->supporting_anchors_resolved = true;

  report->vtable_valid = validate_vtable(
      file, &report->layout, &report->executable_vtable_targets);
  if (!report->vtable_valid) {
    report->failure = "base-device vtable did not validate";
    return false;
  }
  report->complete = true;
  report->failure = "none";
  return true;
}

}  // namespace nba2k27
