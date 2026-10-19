#pragma once

#include <cstddef>
#include <cstdint>

namespace nba2k27 {

constexpr std::size_t kBaseVtableSlots = 47;

struct ProviderLayout {
  std::uint32_t manager_getter = 0;
  std::uint32_t manager_register = 0;
  std::uint32_t manager_find = 0;
  std::uint32_t manager_select = 0;
  std::uint32_t reference_release = 0;
  std::uint32_t base_constructor = 0;
  std::uint32_t set_device_id = 0;
  std::uint32_t base_destructor = 0;
  std::uint32_t base_vtable = 0;
  std::uint32_t path_builder = 0;
  std::uint32_t resource_exists_by_id = 0;
  std::uint32_t base_exists = 0;
  std::uint32_t base_exists_open_call = 0;
  std::uint32_t open_result_constructor = 0;
  std::uint32_t manager_open = 0;
  std::uint32_t manager_open_device_call = 0;
  std::uint32_t file_handle_constructor = 0;
  std::uint32_t file_handle_initialize = 0;
  std::uint32_t file_handle_read = 0;
  std::uint32_t file_handle_destructor = 0;
  std::uint32_t base_vtable_targets[kBaseVtableSlots] = {};
};

struct ResolverReport {
  bool image_parsed = false;
  bool manager_cluster_resolved = false;
  bool base_cluster_resolved = false;
  bool supporting_anchors_resolved = false;
  bool vtable_valid = false;
  bool complete = false;
  std::uint32_t timestamp = 0;
  std::uint32_t image_size = 0;
  std::uint32_t manager_anchor_matches = 0;
  std::uint32_t manager_cluster_candidates = 0;
  std::uint32_t base_anchor_matches = 0;
  std::uint32_t base_cluster_candidates = 0;
  std::uint32_t path_candidates = 0;
  std::uint32_t reference_release_candidates = 0;
  std::uint32_t executable_vtable_targets = 0;
  const char *failure = "not run";
  ProviderLayout layout = {};
};

bool resolve_provider_layout(const wchar_t *executable_path,
                             ResolverReport *report);

}  // namespace nba2k27
