// Included inside the audit namespace. Private-copy-only lifetime test.
inline bool expand_fingerprints(std::uintptr_t base) {
  return mem::q(base+0x683A4C8)==base+0x823830&&
    mem::match_hex(base+0x17B970,"488991c8000000448981d0000000c3")&&
    mem::match_hex(base+0x823830,"555657534883ec28488d6c242048c74500feffffff89d74889ce488d05776c0106488901837908007e1f31db0f1f4000488b4620488b0cd8e8d3ba2f0448ffc3486346084839c37ce7488b4e20e8beba2f04488b4e10e8b5ba2f0448c746200000000048c7461000000000c746080000000085ff74084889f1e892ba2f044889f04883c4285b5f5e5dc3")&&
    mem::match_hex(base+0x7AA2B0,"56574883ec4889d74889ce4c8941184d85c0750eb13ee8259647034989c04889461848c746100000000048c746200000000085ff0f84af00000083ff217c2fc5f857c0c5f811442438488d05da0009064889442430488d05d6fa08064889442420c744242805030000e9ac0000004d85c0747d897e08c1e7034863cfc644243801c644243000c744242808000000c7442420120300004c89c241b80020000041b9702eb9cae8764d370448894610488b561848634e0848c1e103c644243801c644243000c744242808000000c74424201303000041b80020000041b9702eb9cae83b4d3704488946204883c4485f5ec3c5f857c0c5f811442438488d05df0009064889442430488d0525fa08064889442420c74424280c030000488d0d360c4f05488d1552e904064c8d050e6605064c8d0d8c000906e8f53f370484c07401ccc74608000000004883c4485f5ec3");
}
inline bool expanded_copy_matches(const Snapshot &before,const Snapshot &after,const wchar_t *const *expected=nullptr,unsigned pieces=4) {
  if(pieces<3||pieces>7||(!expected&&pieces!=4)||before.count!=2||after.count!=pieces||before.address!=after.address||before.owner!=after.owner)return false;
  auto bytes=after.bytes;
  // Only exposed list pointer and count may change in description header.
  std::memcpy(bytes.data()+0xC8,before.bytes.data()+0xC8,8);
  std::memcpy(bytes.data()+0xD0,before.bytes.data()+0xD0,4);
  if(expected) {
    for(unsigned n=0;n<pieces;++n)if(std::wcscmp(after.names[n].data(),expected[n])!=0)return false;
    return bytes==before.bytes;
  }
  return bytes==before.bytes&&before.names[0]==after.names[0]&&before.names[1]==after.names[1]&&
    std::wcscmp(after.names[2].data(),L"CODEX_PRIVATE_EXTRA_2")==0&&
    std::wcscmp(after.names[3].data(),L"CODEX_PRIVATE_EXTRA_3")==0;
}
