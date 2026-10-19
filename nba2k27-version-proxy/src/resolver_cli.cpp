#include <Windows.h>

#include <cstddef>
#include <cstdio>

#include "provider_resolver.hpp"

namespace {

void print_rva(const char *name, std::uint32_t rva, bool comma = true) {
  std::printf("    \"%s\": \"0x%08X\"%s\n", name, rva,
              comma ? "," : "");
}

}  // namespace

int wmain(int argc, wchar_t **argv) {
  if (argc != 2) {
    std::fputs("Usage: NBA2K27ProviderResolver.exe <NBA2K27.exe>\n", stderr);
    return 2;
  }
  nba2k27::ResolverReport report = {};
  const bool ok = nba2k27::resolve_provider_layout(argv[1], &report);
  std::printf("{\n");
  std::printf("  \"schema\": \"nba2k27-provider-layout-v1\",\n");
  std::printf("  \"ok\": %s,\n", ok && report.complete ? "true" : "false");
  std::printf("  \"failure\": \"%s\",\n", report.failure);
  std::printf("  \"timestamp\": \"0x%08X\",\n", report.timestamp);
  std::printf("  \"imageSize\": \"0x%08X\",\n", report.image_size);
  std::printf("  \"checks\": {\n");
  std::printf("    \"managerAnchorMatches\": %u,\n", report.manager_anchor_matches);
  std::printf("    \"managerClusterCandidates\": %u,\n", report.manager_cluster_candidates);
  std::printf("    \"baseAnchorMatches\": %u,\n", report.base_anchor_matches);
  std::printf("    \"baseClusterCandidates\": %u,\n", report.base_cluster_candidates);
  std::printf("    \"pathCandidates\": %u,\n", report.path_candidates);
  std::printf("    \"referenceReleaseCandidates\": %u,\n", report.reference_release_candidates);
  std::printf("    \"executableVtableTargets\": %u\n", report.executable_vtable_targets);
  std::printf("  },\n");
  std::printf("  \"layout\": {\n");
  const auto &layout = report.layout;
  print_rva("managerGetter", layout.manager_getter);
  print_rva("managerRegister", layout.manager_register);
  print_rva("managerFind", layout.manager_find);
  print_rva("managerSelect", layout.manager_select);
  print_rva("referenceRelease", layout.reference_release);
  print_rva("baseConstructor", layout.base_constructor);
  print_rva("setDeviceId", layout.set_device_id);
  print_rva("baseDestructor", layout.base_destructor);
  print_rva("baseVtable", layout.base_vtable);
  print_rva("pathBuilder", layout.path_builder);
  print_rva("resourceExistsById", layout.resource_exists_by_id);
  print_rva("baseExists", layout.base_exists);
  print_rva("baseExistsOpenCall", layout.base_exists_open_call);
  print_rva("openResultConstructor", layout.open_result_constructor);
  print_rva("managerOpen", layout.manager_open);
  print_rva("managerOpenDeviceCall", layout.manager_open_device_call);
  print_rva("fileHandleConstructor", layout.file_handle_constructor);
  print_rva("fileHandleInitialize", layout.file_handle_initialize);
  print_rva("fileHandleRead", layout.file_handle_read);
  print_rva("fileHandleDestructor", layout.file_handle_destructor, false);
  std::printf("  },\n");
  std::printf("  \"vtable\": [\n");
  for (std::size_t index = 0; index < nba2k27::kBaseVtableSlots; ++index) {
    std::printf("    \"0x%08X\"%s\n", layout.base_vtable_targets[index],
                index + 1 == nba2k27::kBaseVtableSlots ? "" : ",");
  }
  std::printf("  ]\n");
  std::printf("}\n");
  return ok && report.complete ? 0 : 1;
}
