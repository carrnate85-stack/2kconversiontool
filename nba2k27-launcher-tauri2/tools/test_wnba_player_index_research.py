import unittest
from wnba_player_index_research import rebuild,verify,lookup,native_mix
class NativeIndexTests(unittest.TestCase):
 def test_collisions_and_unsigned_sorted_ties(self):
  # Deliberately share a bucket, and cover unsigned keys above INT_MAX.
  ids=[]
  for value in range(65535):
   if native_mix(value)%17==3:ids.append(value)
   if len(ids)==9:break
  audio=[0xffffffff,0,0x80000000,4,4,4,1,7,7];signature=list(reversed(audio))
  slots=rebuild(ids,audio,signature,17);self.assertEqual(verify(slots,ids,audio,signature)['uniqueIdLookupPass'],9)
  self.assertTrue(all(lookup(slots,ids,key)==i for i,key in enumerate(ids)));self.assertIsNone(lookup(slots,ids,65535))
  for region in [0,1,2]:
   bad=slots.copy();at=[3,17,26][region];bad[at]=65535
   with self.assertRaises(ValueError):verify(bad,ids,audio,signature)
  bad=slots.copy();bad[17],bad[18]=bad[18],bad[17]
  with self.assertRaises(ValueError):verify(bad,ids,audio,signature)
 def test_bounds_and_duplicate_ids_refuse(self):
  for ids,audio,signature,buckets in [([],[],[],4),([1,1],[2,3],[4,5],4),([1,65535],[2,3],[4,5],4),([1,2],[2,3],[4,5],2),([1],[0x100000000],[4],4),([1],[2],[],4)]:
   with self.assertRaises(ValueError):rebuild(ids,audio,signature,buckets)
if __name__=='__main__':unittest.main()
