# Wharf Protocol

The wharf protocol was created by
[Amos Wenger](https://docs.itch.zone/wharf/master/#authors) when working
for [itch.io](https://itch.io/) with the objective of keeping games up
to date. It specifies algorithms for diffing and patching old versions
of games to newer ones. It also provides an algorithm to generate
signatures for game versions, allowing users to check for corrupted
installations.

The official wharf protocol specification can be found here:
<https://docs.itch.zone/wharf/master/>

The reference implementation can be found here:
<https://github.com/itchio/wharf/>

## Binary Formats

The wharf protocol uses a binary format based on
[protobuf](https://protobuf.dev/) to represent the patches and signatures.
A protobuf message is a binary string that encodes structured data
according to rules defined in a .proto schema. Additionally, all protobuf
messages in wharf's binary formats are length-delimited. This means that,
before any protobuf message, a
[varint](https://protobuf.dev/programming-guides/encoding/#varints)
indicating the length of the following message will be placed. Wharf's
patches and signatures consist of protobuf messages of different kinds
following one another.

At the beginning of each binary, a 4-byte sequence of magic bytes
identifies the format. Generally, wharf's patches and signatures are
compressed. The header protobuf message specifies the compression
algorithm used for the remaining data.

### [Patch](patch.md)
### [Signature](signature.md)
