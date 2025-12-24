# Wharf Patch Format

Excluding the magic bytes, the binary patch consists only on protobuf
messages.

## Magic bytes

The first four bytes of a wharf patch are the magic bytes, which are
represented as `0x0FEF_5F00` in little-endian format.

## Header

The first protobuf message is a header that determines the compression
used for the remaining data. Currently, the only supported compressions
in wharf are:

 - Brotli (used on default patches)
 - Zstandard (used on optimized patches)
 - gzip (not currently used by itch.io, but still supported)

Additionally, the header message also specifies the compression quality,
but this information is not useful for decompressing the patch.

## Containers

Following the header are the two protobuf messages indicating the old
and new containers. These messages indicate the files, folders, and
symlinks of the old and new build folders and their permissions. They
also indicate the size of each file.

## Patch Operations Loop

At this point, the patch operations begin. Each header indicates the file
index in the new container to patch and the algorithm used for patching it.

### Rsync Patching

If the algorithm is rsync, a sync op loop begins. Each sync operation can
be of three different types:

 - BlockRange:
   - read the fields file_index, block_index and block_span
   - open the file with index file_index in the old container
   - copy from the old file into the new one the bytes from block_index *
BLOCK_SIZE, the next block_span * BLOCK_SIZE bytes
 - Data: copy the raw bytes in the data field directly to the new file
 - HeyYouDidIt: this sync op loop has ended

### Bsdiff Patching

If the algorithm is bsdiff, then the next protobuf message after the sync
header will be a bsdiff header, which indicates which old file will be used
for the patching.

Then, the control operation loop begins. Each control bsdiff operation
has four fields:

 - add (raw bytes): add these bytes (arithmetic add) to the old file's
ones at the current position and copy them into the new file
 - copy (raw bytes): copy these bytes directly to the new file
 - seek (int64): move the old file cursor forward (positive number) or
backwards (negative number)
 - eof (bool): when true, the control operation loop ends

After the last control operation, a SyncOp protobuf message with the type
field set to HEY_YOU_DID_IT will be placed.

```mermaid
---
title: Wharf Patch
---
flowchart TB
    Magic["Patch Magic Bytes"] --> PatchHeader["Patch Header (pwr::PatchHeader protobuf)"]
    PatchHeader --> ContainerOld
    subgraph Compressed["Compressed stream"]
        ContainerOld["Old Container (tlc::Container protobuf)"] --> ContainerNew["New Container (tlc::Container protobuf)"]
        ContainerNew --> SyncHeader["SyncHeader (pwr::SyncHeader protobuf)"]
        subgraph SyncHeaderLoop["File Patch Loop"]
            SyncHeader -->|"type = BSDIFF"| BsdiffHeader["BsdiffHeader (pwr::BsdiffHeader protobuf)"]
            subgraph BsdiffLoop["Bsdiff File Patch"]
                BsdiffHeader --> Control["Bsdiff Control Operation (bsdiff::Control protobuf)"]
                Control -->|"eof = false"| Control
                Control -->|"eof = true"| HeySyncOp["SyncOp(type = HEY_YOU_DID_IT) (pwr::SyncOp protobuf)"]
            end
            SyncHeader -->|"type = RSYNC"| SyncOp["Rsync Sync Operation (pwr::SyncOp protobuf)"]
            SyncOp -->|"type = HEY_YOU_DID_IT"| EOF
            subgraph RsyncLoop["Rsync File Patch"]
                SyncOp -->|"type != HEY_YOU_DID_IT"| SyncOp
            end
            HeySyncOp --> EOF

            EOF{"End of stream?"} -->|"no → next file"| SyncHeader
        end
    end
```
