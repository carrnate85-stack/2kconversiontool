# Shared NBA 2K Asset Reader

Canonical Rust modules for manifest indexing, VCZ/ZIP decoding, binary resource
metadata and DDS reconstruction. Used by Archive Studio and Character Mod Tool.

Character Mod Tool uses this reader only for preview resources and dependencies.
Its existing geometry/tangent parser and Python/Blender conversion engine remain
in place. Unsupported preview resources can use the original reader.

The original Archive Studio catalog.rs, codec.rs and textures.rs files remain
as source references but are no longer compiled; active imports use this crate.
The released Archive Studio executable is not changed by this source integration.
