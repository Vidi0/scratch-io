```mermaid
---
title: Wharf Signature File
---
flowchart TB
    Magic["Signature magic bytes"] --> SignatureHeader["SignatureHeader (pwr::SignatureHeader protobuf)"]
    SignatureHeader --> ContainerNew
    subgraph Compressed["Compressed stream (per SignatureHeader.compression)"]
        ContainerNew["New Container (tlc::Container protobuf)"] --> BlockHash["BlockHash (protobuf)"]
        BlockHash --> BlockHash
    end
```