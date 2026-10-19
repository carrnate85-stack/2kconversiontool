"""Pure offline native player-index reconstruction; no process, key, salt or archive writes."""
from bisect import bisect_left,bisect_right
from collections import Counter
import struct
from wnba_player_index_offline_audit import native_mix
EMPTY=65535

def keys_valid(unique,audio,signature,buckets):
 if not(0<len(unique)<EMPTY and len(unique)==len(audio)==len(signature) and len(unique)<buckets<EMPTY):raise ValueError('Invalid bounded native index shape')
 if len(set(unique))!=len(unique) or any(not 0<=v<EMPTY for v in unique):raise ValueError('UniqueId collision/null/out of range')
 if any(not 0<=v<=0xffffffff for values in [audio,signature]for v in values):raise ValueError('Scalar key out of range')

def rebuild(unique,audio,signature,buckets):
 keys_valid(unique,audio,signature,buckets);table=[EMPTY]*buckets
 for row,key in enumerate(unique):
  at=native_mix(key)%buckets;pending=row;distance=0
  for _ in range(buckets):
   old=table[at]
   if old==EMPTY:table[at]=pending;break
   olddistance=(at-native_mix(unique[old])%buckets)%buckets
   if distance>olddistance:table[at],pending=pending,old;distance=olddistance
   at=(at+1)%buckets;distance+=1
  else:raise ValueError('Native hash insertion exhausted')
 return table+sorted(range(len(unique)),key=lambda i:(audio[i],i))+sorted(range(len(unique)),key=lambda i:(signature[i],i))

def lookup(slots,unique,key):
 buckets=len(slots)-2*len(unique)
 if buckets<=0:raise ValueError('Invalid partition')
 at=native_mix(key)%buckets
 for distance in range(buckets):
  row=slots[(at+distance)%buckets]
  if row==EMPTY:return None
  if row>=len(unique):raise ValueError('Foreign row index')
  if unique[row]==key:return row
 return None

def verify(slots,unique,audio,signature):
 buckets=len(slots)-2*len(unique);keys_valid(unique,audio,signature,buckets)
 if any(not 0<=row<EMPTY and row!=EMPTY for row in slots):raise ValueError('Invalid u16')
 first=slots[:buckets];expected=Counter(range(len(unique)))
 if Counter(row for row in first if row!=EMPTY)!=expected:raise ValueError('Hash coverage differs')
 if any(lookup(slots,unique,key)!=row for row,key in enumerate(unique)):raise ValueError('UniqueId lookup differs')
 for n,values in enumerate([audio,signature]):
  rows=slots[buckets+n*len(unique):buckets+(n+1)*len(unique)]
  if Counter(rows)!=expected:raise ValueError('Sorted alias coverage differs')
  pairs=[(values[row],row)for row in rows]
  if pairs!=sorted(pairs):raise ValueError('Sorted alias order differs')
  # Every duplicate-key run must be contiguous and include each source row.
  keys=[values[row]for row in rows]
  for key in set(keys):
   lo,hi=bisect_left(keys,key),bisect_right(keys,key)
   if hi-lo!=values.count(key):raise ValueError('Sorted alias lookup range differs')
 return {'players':len(unique),'hashBuckets':buckets,'uniqueIdLookupPass':len(unique),'audioAliasRows':len(unique),'signatureAliasRows':len(unique),'emptyHashSlots':first.count(EMPTY)}

def encode(slots):return struct.pack('<'+str(len(slots))+'H',*slots)
