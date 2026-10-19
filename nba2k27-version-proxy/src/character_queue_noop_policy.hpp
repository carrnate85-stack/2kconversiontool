#pragma once
#include <cstdint>

namespace character_queue_noop {
struct Preconditions {
  bool exact_build{}, native_code{}, anti_cheat_absent{}, in_blacktop{};
  bool worker_thread{}, queue_class{}, lock_initialized{}, heap_ready{};
  bool queue_empty{}, already_submitted{};
};
inline bool may_submit(const Preconditions &p) {
  return p.exact_build && p.native_code && p.anti_cheat_absent && p.in_blacktop &&
      p.worker_thread && p.queue_class && p.lock_initialized && p.heap_ready &&
      p.queue_empty && !p.already_submitted;
}
inline bool packet_is_noop(std::uintptr_t target, std::uintptr_t callback,
                           unsigned flag) {
  return target == 0 && callback == 0 && flag == 0;
}
inline bool may_report_dequeued(bool witnessed_packet, bool same_queue,
                               bool queue_empty, bool timed_out) {
  return witnessed_packet && same_queue && queue_empty && !timed_out;
}
}  // namespace character_queue_noop
