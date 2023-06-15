OUT_DIR=.
CARGO=cargo

.PHONY: download_properties
download_properties:
	@mkdir -p $(OUT_DIR)
	@curl -s https://qlever.cs.uni-freiburg.de/api/wikidata -H "Accept: text/tab-separated-values" -H "Content-type: application/sparql-query" --data "PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> PREFIX skos: <http://www.w3.org/2004/02/skos/core#> PREFIX wdt: <http://www.wikidata.org/prop/direct/> PREFIX wd: <http://www.wikidata.org/entity/> PREFIX wikibase: <http://wikiba.se/ontology#> SELECT ?p ?prop_label (GROUP_CONCAT(DISTINCT ?prop_alias; SEPARATOR = \"; \") AS ?prop_aliases) (GROUP_CONCAT(DISTINCT ?inv_prop; SEPARATOR = \"; \") AS ?inv_props) WHERE { ?prop wikibase:directClaim ?p . ?prop rdfs:label ?prop_label . FILTER(LANG(?prop_label) = \"en\") . OPTIONAL { ?prop skos:altLabel ?prop_alias . FILTER(LANG(?prop_alias) = \"en\") } OPTIONAL { ?prop wdt:P1696 ?inv_prop } } GROUP BY ?p ?prop_label" \
	> $(OUT_DIR)/wikidata-properties.tsv

.PHONY: compute_properties
compute_properties:
	@$(CARGO) run --bin wikidata-properties --release -- \
		--file $(OUT_DIR)/wikidata-properties.tsv \
		--output $(OUT_DIR)/wikidata-properties-index.tsv \
		--inverse-output $(OUT_DIR)/wikidata-properties-inverse-index.tsv \
		--keep-most-common-non-unique > $(OUT_DIR)/wikidata-properties-output.txt

.PHONY: download_entities
download_entities:
	@mkdir -p $(OUT_DIR)
	@curl -s https://qlever.cs.uni-freiburg.de/api/wikidata -H "Accept: text/tab-separated-values" -H "Content-type: application/sparql-query" --data "PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> PREFIX wikibase: <http://wikiba.se/ontology#> PREFIX schema: <http://schema.org/> PREFIX wdt: <http://www.wikidata.org/prop/direct/> PREFIX skos: <http://www.w3.org/2004/02/skos/core#> SELECT ?entity ?entity_name ?entity_description (MAX(?sitelinks) AS ?links) (GROUP_CONCAT(DISTINCT ?alias; SEPARATOR = \"; \") AS ?aliases) WHERE { ?entity ^schema:about/wikibase:sitelinks ?sitelinks . ?entity wdt:P18 ?pic . ?entity rdfs:label ?entity_name . FILTER (LANG(?entity_name) = \"en\") . OPTIONAL { ?entity schema:description ?entity_description . FILTER (LANG(?entity_description) = \"en\") } OPTIONAL { ?entity skos:altLabel ?alias . FILTER (LANG(?alias) = \"en\") } } GROUP BY ?entity ?entity_name ?entity_description ORDER BY DESC(?links)" \
	> $(OUT_DIR)/wikidata-entities.tsv

.PHONY: compute_entities
compute_entities:
	@$(CARGO) run --bin wikidata-entities --release -- \
		--file $(OUT_DIR)/wikidata-entities.tsv \
		--output $(OUT_DIR)/wikidata-entities-index.tsv \
		--keep-most-common-non-unique > $(OUT_DIR)/wikidata-entities-output.txt
	@$(CARGO) run --bin wikidata-entities --release -- \
		--file $(OUT_DIR)/wikidata-entities.tsv \
		--output $(OUT_DIR)/wikidata-entities-popular-index.tsv \
		--keep-most-common-non-unique \
		--check-for-popular-aliases > $(OUT_DIR)/wikidata-entities-popular-output.txt

.PHONY: download
download: download_properties download_entities

.PHONY: compute
compute: compute_properties compute_entities

.PHONY: index
index: download compute
