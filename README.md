## Wikidata Natural Language Index

This repository allows to automatically generate
natural language based indices (mapping from text to id) for
Wikidata entities and properties.

Dependencies:
- Rust
- curl

Usage:

```bash
# download and compute the indices
# time: <2 minutes, longer on first use due to compilation
make index

# optional: specify output directory
make index OUT_DIR=path/to/dir
```

We host weekly updated data and indices to download here:
TODO: add URL
