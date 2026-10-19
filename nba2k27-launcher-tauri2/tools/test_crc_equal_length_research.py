import random,unittest,zlib
from crc_equal_length_research import replace_equal_prefix_crc
class EqualPrefixCrcTests(unittest.TestCase):
 def test_reference_vector(self):
  self.assertEqual(zlib.crc32(b'123456789'),0xcbf43926)
  self.assertEqual(replace_equal_prefix_crc(0xcbf43926,b'1234',b'9234',5),zlib.crc32(b'923456789'))
 def test_unknown_random_suffix_every_length_zero_through_256(self):
  rng=random.Random(0x74534f46)
  for suffix_length in range(257):
   for prefix_length in [0,1,16,4096]:
    suffix=rng.randbytes(suffix_length);old=rng.randbytes(prefix_length);new=rng.randbytes(prefix_length)
    observed=zlib.crc32(old+suffix)
    # Production proposal function cannot inspect this generated suffix.
    changed=replace_equal_prefix_crc(observed,old,new,len(suffix))
    self.assertEqual(changed,zlib.crc32(new+suffix))
    self.assertEqual(replace_equal_prefix_crc(observed,old,old,len(suffix)),observed)
 def test_refuses_unproved_shapes(self):
  for old_crc,old,new,length in [(-1,b'a',b'b',22),(1<<32,b'a',b'b',22),(0,b'a',b'bb',22),(0,b'a',b'b',-1),(0,b'a',b'b',257)]:
   with self.assertRaises(ValueError):replace_equal_prefix_crc(old_crc,old,new,length)
if __name__=='__main__':unittest.main()
