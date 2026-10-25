"""Ensure offline relocation comparison cannot bless changed record layouts."""
import struct,unittest
from compare_roster_relocations import compare,Cs,CS_ARCH_X86,CS_MODE_64

class RelocationProof(unittest.TestCase):
    def setUp(self):
        self.md=Cs(CS_ARCH_X86,CS_MODE_64);self.md.detail=True
    def check(self,a,b,start=0x200000,new=0x2000b0):
        return compare(self.md,bytes.fromhex(a),bytes.fromhex(b),start,new)
    def test_identical_record_load_and_mask(self):
        self.assertIsNotNone(self.check('0fb781200f000025ff7f0000c3','0fb781200f000025ff7f0000c3'))
    def test_record_offset_mask_and_destination_changes_are_refused(self):
        old='0fb781200f000025ff7f0000c3'
        for changed in ('0fb781220f000025ff7f0000c3','0fb781200f000025ffff0000c3','0fb783200f000025ff7f0000c3'):
            self.assertIsNone(self.check(old,changed))
    def test_internal_branch_must_keep_same_relative_destination(self):
        self.assertIsNotNone(self.check('eb0190c3','eb0190c3'))
        self.assertIsNone(self.check('eb0190c3','eb0090c3'))
    def test_external_call_requires_supported_relocation(self):
        def call(start,target):return 'e8'+struct.pack('<i',target-start-5).hex()+'c3'
        old=call(0x200000,0x300000)
        self.assertIsNotNone(self.check(old,call(0x2000b0,0x3000b0)))
        self.assertIsNone(self.check(old,call(0x2000b0,0x3000b1)))
    def test_truncated_instruction_rejected(self):
        self.assertIsNone(self.check('488b','488b'))

if __name__=='__main__':unittest.main()
