## Knowledgegraph Natural Language Index

This repository allows to automatically generate
natural language based indices (mapping from text to id) for entities
and properties from Wikidata, Freebase, and DBPedia.

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

We host weekly updated data and indices to download [here](https://ad-wikidata-index.cs.uni-freiburg.de/):
- `wikidata-entities.tsv`: raw Wikidata entities dump
- `wikidata-entities-index.tsv`: label --> entity index (with aliases/descriptions)
- `wikidata-properties.tsv`: raw Wikidata properties dump
- `wikidata-properties-index.tsv`: label --> property index (with aliases)
- `wikidata-properties-inverse-index.tsv`: property --> inverse property index

The same files exist for DBPedia and Freebase.
