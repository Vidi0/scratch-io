```mermaid
---
title: Wharf Signature File
---
flowchart TB
    Magic["Signature Magic Bytes"] --> SignatureHeader["Signature Header (pwr::SignatureHeader protobuf)"]
    SignatureHeader --> ContainerNew
    subgraph Compressed["Compressed stream"]
        ContainerNew["New Container (tlc::Container protobuf)"] --> BlockHash["Block Hash (pwr::BlockHash protobuf)"]
        subgraph BlockHashLoop["Block Hash Loop"]
            BlockHash --> EOF
            EOF{"Maybe EOF"} .->|"Not EOF"| BlockHash
        end
    end
```