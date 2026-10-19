"""RESEARCH ONLY: CRC32 delta for an equal-length prefix and unchanged unknown suffix.
The API accepts only the suffix length; no suffix bytes, keys or game files.
"""
import zlib

def replace_equal_prefix_crc(old_crc,old_prefix,new_prefix,suffix_length):
 if not 0<=old_crc<=0xffffffff or not 0<=suffix_length<=256:raise ValueError('Bounded CRC/suffix length required')
 if len(old_prefix)!=len(new_prefix):raise ValueError('Equal prefix byte lengths required')
 difference=zlib.crc32(old_prefix)^zlib.crc32(new_prefix)
 zeros=bytes(suffix_length)
 # XOR of two affine CRC updates cancels the common zero-suffix constant.
 shifted=zlib.crc32(zeros,difference)^zlib.crc32(zeros,0)
 return old_crc^shifted
