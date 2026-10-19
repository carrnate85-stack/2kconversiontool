#include <Windows.h>

#include <array>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>

namespace {
HANDLE process_handle = nullptr;

bool read_memory(std::uintptr_t address, void *out, std::size_t size) {
  SIZE_T read = 0;
  return address >= 0x10000 &&
         ReadProcessMemory(process_handle, reinterpret_cast<const void *>(address), out, size, &read) &&
         read == size;
}

template <class T> bool read_value(std::uintptr_t address, T &out) {
  return read_memory(address, &out, sizeof(out));
}

std::uintptr_t read_pointer(std::uintptr_t address) {
  std::uintptr_t value = 0;
  read_value(address, value);
  return value;
}

unsigned read_unsigned(std::uintptr_t address) {
  unsigned value = 0;
  read_value(address, value);
  return value;
}

std::uintptr_t character_description(std::uintptr_t actor) {
  if (!actor) return 0;
  const auto interface_address = actor + 0x78;
  const auto interface_vtable = read_pointer(interface_address);
  const auto getter = interface_vtable ? read_pointer(interface_vtable + 0x158) : 0;
  std::array<unsigned char, 8> code{};
  std::int32_t displacement = 0;
  if (!getter || !read_memory(getter, code.data(), code.size()) || code[0] != 0x48 ||
      code[1] != 0x8D || code[2] != 0x81 || code[7] != 0xC3)
    return 0;
  std::memcpy(&displacement, code.data() + 3, sizeof(displacement));
  return interface_address + displacement;
}

bool supported_build(std::uintptr_t base) {
  IMAGE_DOS_HEADER dos{};
  IMAGE_NT_HEADERS64 nt{};
  return read_value(base, dos) && dos.e_magic == IMAGE_DOS_SIGNATURE && dos.e_lfanew > 0 &&
         dos.e_lfanew < 0x1000 && read_value(base + dos.e_lfanew, nt) &&
         nt.Signature == IMAGE_NT_SIGNATURE && nt.FileHeader.Machine == IMAGE_FILE_MACHINE_AMD64 &&
         nt.FileHeader.TimeDateStamp == 0x6AA2013D && nt.OptionalHeader.SizeOfImage == 0x36558000;
}

bool finite_scale(float value) {
  return std::isfinite(value) && value > 0.05f && value < 20.0f;
}
} // namespace

// Read-only inventory for the September 11 NBA 2K27 build. This deliberately
// has no process-write access, injection path, game calls, or synchronization
// mutations. It identifies active character managers and compares the scale
// flag in their embedded descriptions with the resulting actor scale cache.
int main(int argc, char **argv) {
  if (argc != 4) return 2;
  const auto pid = static_cast<DWORD>(std::strtoul(argv[1], nullptr, 10));
  const auto expected_created = std::strtoull(argv[2], nullptr, 10);
  const auto base = static_cast<std::uintptr_t>(std::strtoull(argv[3], nullptr, 10));

  process_handle = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, FALSE, pid);
  if (!process_handle) return 3;

  FILETIME created{}, exited{}, kernel{}, user{};
  ULARGE_INTEGER creation_stamp{};
  const bool process_ok = GetProcessTimes(process_handle, &created, &exited, &kernel, &user) != FALSE;
  creation_stamp.LowPart = created.dwLowDateTime;
  creation_stamp.HighPart = created.dwHighDateTime;
  if (!process_ok || creation_stamp.QuadPart != expected_created || exited.dwLowDateTime ||
      exited.dwHighDateTime || !supported_build(base)) {
    CloseHandle(process_handle);
    return 4;
  }

  constexpr std::uintptr_t manager_list_rva = 0xA37FFB0;
  constexpr std::uintptr_t settings_rva = 0xAACB9A0;
  constexpr std::uintptr_t shared_scale_rva = 0xA39319C;
  const auto sentinel = base + manager_list_rva;
  const auto settings = read_pointer(base + settings_rva);
  const auto mode = settings ? read_unsigned(settings + 0xA04) : 0xffffffffu;
  float shared_scale = 0.0f;
  read_value(base + shared_scale_rva, shared_scale);
  constexpr std::uintptr_t player_actor_vtable_rva = 0x682C528;
  constexpr std::uintptr_t player_description_vtable_rva = 0x682DB58;
  // The coach classes moved by the same +0x3000 vtable delta as the verified
  // ordinary-character classes in this build. Report their first virtual
  // functions so a live match can confirm the candidate identities without
  // making a call through either table.
  constexpr std::uintptr_t coach_actor_vtable_rva = 0x6807E58;
  constexpr std::uintptr_t coach_description_vtable_rva = 0x6807818;
  std::printf("BUILD=6AA2013D:36558000 PID=%lu MODE=%u SHARED_SCALE=%.9g READ_ONLY=YES\n",
              static_cast<unsigned long>(pid), mode, shared_scale);
  std::printf("PLAYER_ACTOR_VT0_RVA=0x%llX PLAYER_DESCRIPTION_VT0_RVA=0x%llX "
              "COACH_ACTOR_CANDIDATE_VT0_RVA=0x%llX COACH_DESCRIPTION_CANDIDATE_VT0_RVA=0x%llX\n",
              static_cast<unsigned long long>(read_pointer(base + player_actor_vtable_rva) - base),
              static_cast<unsigned long long>(read_pointer(base + player_description_vtable_rva) - base),
              static_cast<unsigned long long>(read_pointer(base + coach_actor_vtable_rva) - base),
              static_cast<unsigned long long>(read_pointer(base + coach_description_vtable_rva) - base));

  auto node = read_pointer(sentinel + 0x10);
  auto previous = sentinel;
  unsigned total = 0;
  bool links_valid = true;
  for (; total < 256 && node && node != sentinel; ++total) {
    const auto prior = read_pointer(node + 8);
    if (prior != previous) {
      links_valid = false;
      break;
    }
    const auto manager = node + 0x78;
    const auto manager_vtable = read_pointer(manager);
    const auto actor = read_pointer(manager + 0x98);
    const auto actor_vtable = actor ? read_pointer(actor) : 0;
    const auto interface_vtable = actor ? read_pointer(actor + 0x78) : 0;
    const auto scale_method = interface_vtable ? read_pointer(interface_vtable + 0x188) : 0;
    const auto description = character_description(actor);
    const auto description_vtable = description ? read_pointer(description) : 0;
    unsigned char flag_7c = 0, flag_7d = 0, pending = 0xff;
    float scale = 0.0f, inverse = 0.0f;
    if (description) {
      read_value(description + 0x7C, flag_7c);
      read_value(description + 0x7D, flag_7d);
    }
    if (actor) {
      read_value(actor + 0xBCC, scale);
      read_value(actor + 0xBD0, inverse);
    }
    read_value(manager + 0xB8, pending);
    const bool reciprocal = finite_scale(scale) && finite_scale(inverse) &&
                            std::fabs(scale * inverse - 1.0f) < 0.002f;
    const char *kind = actor_vtable == base + player_actor_vtable_rva
                           ? "PLAYER"
                           : (actor_vtable == base + coach_actor_vtable_rva ? "COACH" : "OTHER");
    std::printf(
        "ENTRY=%u KIND=%s NODE=0x%llX MANAGER=0x%llX MANAGER_VT_RVA=0x%llX "
        "ACTOR=0x%llX ACTOR_VT_RVA=0x%llX DESCRIPTION_VT_RVA=0x%llX SCALE_METHOD_RVA=0x%llX "
        "FLAG_7C=0x%02X FLAG_7D=0x%02X SHARED_OPT_IN=%s SCALE=%.9g INVERSE=%.9g "
        "RECIPROCAL=%s PENDING=%u\n",
        total + 1, kind, static_cast<unsigned long long>(node),
        static_cast<unsigned long long>(manager),
        static_cast<unsigned long long>(manager_vtable ? manager_vtable - base : 0),
        static_cast<unsigned long long>(actor),
        static_cast<unsigned long long>(actor_vtable ? actor_vtable - base : 0),
        static_cast<unsigned long long>(description_vtable ? description_vtable - base : 0),
        static_cast<unsigned long long>(scale_method ? scale_method - base : 0), flag_7c,
        flag_7d, (flag_7d & 2) ? "YES" : "NO", scale, inverse, reciprocal ? "YES" : "NO",
        static_cast<unsigned>(pending));
    if (actor_vtable == base + coach_actor_vtable_rva) {
      const auto transform_owner = read_pointer(actor + 0xB80);
      const auto matrix_address = transform_owner ? read_pointer(transform_owner + 0x38) : 0;
      std::array<float, 16> matrix{};
      const bool matrix_read = matrix_address && read_memory(matrix_address, matrix.data(), sizeof(matrix));
      const auto length3 = [&](unsigned offset) {
        return std::sqrt(matrix[offset] * matrix[offset] + matrix[offset + 1] * matrix[offset + 1] +
                         matrix[offset + 2] * matrix[offset + 2]);
      };
      const float basis_x = matrix_read ? length3(0) : 0.0f;
      const float basis_y = matrix_read ? length3(4) : 0.0f;
      const float basis_z = matrix_read ? length3(8) : 0.0f;
      std::printf(
          "COACH_MATRIX ENTRY=%u OWNER=0x%llX ADDRESS=0x%llX READ=%s "
          "BASIS_X=%.9g BASIS_Y=%.9g BASIS_Z=%.9g CACHE_MATCH=%s "
          "TRANSLATION=(%.9g,%.9g,%.9g,%.9g)\n",
          total + 1, static_cast<unsigned long long>(transform_owner),
          static_cast<unsigned long long>(matrix_address), matrix_read ? "YES" : "NO", basis_x,
          basis_y, basis_z,
          matrix_read && std::fabs(basis_x - scale) < 0.0001f &&
                  std::fabs(basis_y - scale) < 0.0001f && std::fabs(basis_z - scale) < 0.0001f
              ? "YES"
              : "NO",
          matrix[12], matrix[13], matrix[14], matrix[15]);
      std::array<unsigned char, 0x1E60> actor_bytes{};
      const bool actor_read = read_memory(actor, actor_bytes.data(), actor_bytes.size());
      std::printf("COACH_SCALE_COPIES ENTRY=%u READ=%s SCALE_OFFSETS=", total + 1,
                  actor_read ? "YES" : "NO");
      unsigned matches = 0;
      if (actor_read) {
        for (unsigned offset = 0; offset + sizeof(float) <= actor_bytes.size(); offset += 4) {
          float candidate = 0.0f;
          std::memcpy(&candidate, actor_bytes.data() + offset, sizeof(candidate));
          if (std::isfinite(candidate) && std::fabs(candidate - scale) < 0.000001f) {
            if (matches++) std::printf(",");
            std::printf("0x%X", offset);
          }
        }
      }
      std::printf(" INVERSE_OFFSETS=");
      matches = 0;
      if (actor_read) {
        for (unsigned offset = 0; offset + sizeof(float) <= actor_bytes.size(); offset += 4) {
          float candidate = 0.0f;
          std::memcpy(&candidate, actor_bytes.data() + offset, sizeof(candidate));
          if (std::isfinite(candidate) && std::fabs(candidate - inverse) < 0.000001f) {
            if (matches++) std::printf(",");
            std::printf("0x%X", offset);
          }
        }
      }
      std::printf("\n");
      const auto context = read_pointer(manager + 0x20);
      std::array<std::uintptr_t, 32> manager_words{};
      std::array<std::uintptr_t, 32> context_words{};
      const bool manager_read = read_memory(manager, manager_words.data(), sizeof(manager_words));
      const bool context_read = context && read_memory(context, context_words.data(), sizeof(context_words));
      std::printf("COACH_POINTERS ENTRY=%u CONTEXT=0x%llX MANAGER=", total + 1,
                  static_cast<unsigned long long>(context));
      if (manager_read) {
        for (unsigned index = 0; index < manager_words.size(); ++index) {
          const auto candidate = manager_words[index];
          if (candidate >= 0x10000 && candidate <= 0x7FFFFFFFFFFFULL)
            std::printf("+0x%X:0x%llX,", index * 8,
                        static_cast<unsigned long long>(candidate));
        }
      }
      std::printf(" CONTEXT_WORDS=");
      if (context_read) {
        for (unsigned index = 0; index < context_words.size(); ++index) {
          const auto candidate = context_words[index];
          if (candidate >= 0x10000 && candidate <= 0x7FFFFFFFFFFFULL)
            std::printf("+0x%X:0x%llX,", index * 8,
                        static_cast<unsigned long long>(candidate));
        }
      }
      std::printf("\n");
    }
    previous = node;
    node = read_pointer(node + 0x10);
  }

  if (node != sentinel || read_pointer(sentinel + 8) != previous) links_valid = false;
  std::printf("ENTRIES=%u LINKS_VALID=%s PROCESS_WRITES=0 NATIVE_CALLS=0\n", total,
              links_valid ? "YES" : "NO");
  CloseHandle(process_handle);
  return links_valid ? 0 : 5;
}
