"""Synthetic corruption checks for the exact-code appearance audit, no process access."""
import importlib.util
from pathlib import Path
import struct
import unittest
path=Path(__file__).with_name("appearance_v2_disk_audit.py")
spec=importlib.util.spec_from_file_location("appearance_audit",path)
audit=importlib.util.module_from_spec(spec);spec.loader.exec_module(audit)
class Fixture:
 def __init__(self):
  self.bytes={}
  for _,rva,code in audit.PROOFS:self.put(rva,bytes.fromhex(code))
  for instruction,target,name in audit.NAMES:
   self.put(instruction,bytes.fromhex("4c8d0d")+struct.pack("<i",target-instruction-7));self.put(target,name.encode()+b"\0")
 def put(self,rva,data):
  for index,value in enumerate(data):self.bytes[rva+index]=value
 def read(self,rva,size):return bytes(self.bytes[rva+index] for index in range(size))
class ProofCorruption(unittest.TestCase):
 def test_exact_anchors_and_assertion_references_accept(self):
  self.assertEqual(len(audit.verify_code(Fixture())),len(audit.PROOFS))
 def test_every_code_anchor_rejects_a_changed_byte(self):
  for label,rva,_ in audit.PROOFS:
   with self.subTest(label=label):
    fixture=Fixture();fixture.bytes[rva]^=1
    with self.assertRaises(ValueError):audit.verify_code(fixture)
 def test_named_unit_and_rip_target_are_required(self):
  for instruction,target,_ in audit.NAMES:
   for changed in [instruction+3,target]:
    with self.subTest(changed=changed):
     fixture=Fixture();fixture.bytes[changed]^=1
     with self.assertRaises(ValueError):audit.verify_code(fixture)
if __name__=="__main__":unittest.main()
