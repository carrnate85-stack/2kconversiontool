#pragma once
#include <Windows.h>
#include <array>
#include <atomic>
#include <cstdint>

// One private request-object hook, not a shared-vtable or code patch.
// The native consumer holds its queue lock throughout this callback. Never do
// I/O, waits, allocation, or player rebuilding here. Storage/code must outlive
// an uncertain request; the experimental DLL is pinned before installation.
namespace character_queue_thread_observer {
using Destructor=void *(*)(void *,unsigned);
static_assert(std::atomic<unsigned>::is_always_lock_free);
inline std::array<std::uintptr_t,21> table{}; // RTTI [-1], then slots 0..0x98.
inline std::uintptr_t stock_table{};
inline void *expected_node{};
inline Destructor original{};
inline std::atomic<unsigned> calls{},thread_id{},delete_flag{},identity_ok{};
inline std::atomic<unsigned> restored{},returned{};
inline bool installed=false;

inline void *observe_destructor(void *node,unsigned flags) noexcept {
  calls.fetch_add(1,std::memory_order_relaxed);
  thread_id.store(GetCurrentThreadId(),std::memory_order_relaxed);
  delete_flag.store(flags,std::memory_order_relaxed);
  identity_ok.store(node==expected_node,std::memory_order_relaxed);
  *static_cast<std::uintptr_t *>(node)=stock_table;
  restored.store(1,std::memory_order_release);
  void *result=original(node,flags);
  // The node may now be freed. Never dereference it again.
  returned.store(1,std::memory_order_release);
  return result;
}
inline bool pin_callback_module() {
  HMODULE module=nullptr;
  return GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS|GET_MODULE_HANDLE_EX_FLAG_PIN,
      reinterpret_cast<LPCWSTR>(&observe_destructor),&module)!=FALSE;
}
// Caller must own the verified queue lock and verify the complete stock packet.
inline bool install(void *node,std::uintptr_t stock,const std::array<std::uintptr_t,21> &copy) {
  if(installed||!node||!stock||!copy[1]||
      *static_cast<std::uintptr_t *>(node)!=stock)return false;
  table=copy;stock_table=stock;expected_node=node;
  original=reinterpret_cast<Destructor>(copy[1]);
  table[1]=reinterpret_cast<std::uintptr_t>(&observe_destructor);
  installed=true;
  *static_cast<std::uintptr_t *>(node)=reinterpret_cast<std::uintptr_t>(table.data()+1);
  return true;
}
inline bool completed(unsigned expected_thread) {
  return installed&&returned.load(std::memory_order_acquire)==1&&
      calls.load()==1&&restored.load()==1&&identity_ok.load()==1&&
      delete_flag.load()==1&&expected_thread!=0&&thread_id.load()==expected_thread;
}
inline const char *hook_state() {
  if(!installed)return "NOT_INSTALLED";
  if(restored.load(std::memory_order_acquire))return "REMOVED_FROM_REQUEST";
  return "UNCONFIRMED_CLOSE_GAME";
}
} // namespace character_queue_thread_observer
