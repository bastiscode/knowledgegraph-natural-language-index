OUT_DIR=.
CARGO=cargo

TIMEOUT=1h

WD_ENDPOINT=https://qlever.cs.uni-freiburg.de/api/wikidata
WD_ACCESS_TOKEN=null

DB_ENDPOINT=https://qlever.cs.uni-freiburg.de/api/dbpedia
DB_ACCESS_TOKEN=null

FB_ENDPOINT=https://qlever.cs.uni-freiburg.de/api/freebase
FB_ACCESS_TOKEN=null

.PHONY: index
index: download compute

.PHONY: download_properties
download_properties:
	@mkdir -p $(OUT_DIR)
	@curl -s $(WD_ENDPOINT) -H "Accept: text/tab-separated-values" \
	--data-urlencode query="PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> PREFIX skos: <http://www.w3.org/2004/02/skos/core#> PREFIX wdt: <http://www.wikidata.org/prop/direct/> PREFIX wd: <http://www.wikidata.org/entity/> PREFIX wikibase: <http://wikiba.se/ontology#> PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> SELECT ?p ?p_label ?p_count (GROUP_CONCAT(DISTINCT ?p_alias; SEPARATOR = \"; \") AS ?p_aliases) (GROUP_CONCAT(DISTINCT ?p_inv; SEPARATOR = \"; \") AS ?p_invs) WHERE { ?p wikibase:directClaim ?claim . ?p rdfs:label ?p_label . FILTER(LANG(?p_label) = \"en\") . BIND(0 AS ?p_count) . OPTIONAL { ?p skos:altLabel ?p_alias . FILTER(LANG(?p_alias) = \"en\") } OPTIONAL { ?p wdt:P1696 ?p_inv } } GROUP BY ?p ?p_label ?p_count" \
	--data-urlencode access-token=$(WD_ACCESS_TOKEN) --data-urlencode timeout=$(TIMEOUT) \
	> $(OUT_DIR)/wikidata-properties.tsv
	@curl -s $(FB_ENDPOINT) -H "Accept: text/tab-separated-values" \
	--data-urlencode query="PREFIX fb: <http://rdf.freebase.com/ns/> SELECT DISTINCT ?p ?p_label ?p_count ?domain WHERE { { SELECT ?p (COUNT(?p) as ?p_count) WHERE { ?s ?p ?o } GROUP BY ?p } ?p fb:type.object.name ?p_label . FILTER(LANG(?p_label) = \"en\") . ?p fb:type.object.type fb:type.property . OPTIONAL { ?p fb:type.property.schema ?domain_ . ?domain_ fb:type.object.name ?domain . FILTER(LANG(?domain) = \"en\") } } GROUP BY ?p ?p_label ?p_count ?domain ORDER BY DESC(?p_count)" \
	--data-urlencode access-token=$(FB_ACCESS_TOKEN) --data-urlencode timeout=$(TIMEOUT) \
	> $(OUT_DIR)/freebase-properties.tsv
	# @curl -s $(DB_ENDPOINT) -H "Accept: text/tab-separated-values" \
	# --data-urlencode query="PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> PREFIX dbo: <http://dbpedia.org/ontology/> SELECT ?p ?p_label ?p_count (GROUP_CONCAT(DISTINCT ?alias; SEPARATOR = \"; \") AS ?aliases) (GROUP_CONCAT(DISTINCT ?inv_p; SEPARATOR = \"; \") AS ?inverse) WHERE { { SELECT ?p (COUNT(?p) as ?p_count) WHERE { ?s ?p ?o } GROUP BY ?p } ?p rdfs:label ?p_label . FILTER(LANG(?p_label) = \"en\") . OPTIONAL { ?p dbo:alias ?alias . FILTER (LANG(?alias) = \"en\") } OPTIONAL { ?inv_p dbo:inverseOf ?p } } GROUP BY ?p ?p_label ?p_count ORDER BY DESC(?p_count)" \
	# --data-urlencode access-token=$(DB_ACCESS_TOKEN) --data-urlencode timeout=$(TIMEOUT) \
	# > $(OUT_DIR)/dbpedia-properties.tsv

.PHONY: compute_properties
compute_properties:
	@mkdir -p $(OUT_DIR)/wikidata-properties
	@$(CARGO) run --bin kg-properties --release -- \
		--file $(OUT_DIR)/wikidata-properties.tsv \
		--output $(OUT_DIR)/wikidata-properties \
		--include-wikidata-qualifiers \
		--knowledge-base wikidata \
		> $(OUT_DIR)/wikidata-properties/output.txt
	mkdir -p $(OUT_DIR)/freebase-properties
	@$(CARGO) run --bin kg-properties --release -- \
		--file $(OUT_DIR)/freebase-properties.tsv \
		--output $(OUT_DIR)/freebase-properties \
		--knowledge-base freebase \
		> $(OUT_DIR)/freebase-properties/output.txt
	# @mkdir -p $(OUT_DIR)/dbpedia-properties
	# @$(CARGO) run --bin kg-properties --release -- \
	# 	--file $(OUT_DIR)/dbpedia-properties.tsv \
	# 	--output $(OUT_DIR)/dbpedia-properties \
	# 	--knowledge-base dbpedia \
	# 	> $(OUT_DIR)/dbpedia-properties/output.txt

.PHONY: download_entities
download_entities:
	@mkdir -p $(OUT_DIR)
	@curl -s $(WD_ENDPOINT) -H "Accept: text/tab-separated-values" \
	--data-urlencode query="PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> PREFIX skos: <http://www.w3.org/2004/02/skos/core#> PREFIX wikibase: <http://wikiba.se/ontology#> PREFIX schema: <http://schema.org/> PREFIX wdt: <http://www.wikidata.org/prop/direct/> PREFIX wd: <http://www.wikidata.org/entity/> SELECT ?ent ?ent_name ?ent_description ?links (GROUP_CONCAT(DISTINCT ?type; SEPARATOR = \"; \") AS ?types) (GROUP_CONCAT(DISTINCT ?alias; SEPARATOR = \"; \") AS ?aliases) WHERE { { SELECT ?ent WHERE { ?ent wdt:P279*/wdt:P18 ?pic } GROUP BY ?ent } UNION { SELECT ?ent WHERE { ?ent wdt:P31*/wdt:P18 ?pic } GROUP BY ?ent } UNION { SELECT ?ent WHERE { ?ent ^schema:about/schema:isPartOf ?wiki . FILTER(REGEX(STR(?wiki), \"^https?://.*.wikipedia.org\")) } GROUP BY ?ent } MINUS { ?ent wdt:P31 wd:Q4167836 } ?ent rdfs:label ?ent_name . FILTER(LANG(?ent_name) = \"en\") . FILTER(REGEX(STR(?ent), \"entity/Q\\\\d+\")) . OPTIONAL { ?ent ^schema:about/wikibase:sitelinks ?links } OPTIONAL { ?ent schema:description ?ent_description . FILTER (LANG(?ent_description) = \"en\") } BIND(\"\" AS ?type) OPTIONAL { ?ent skos:altLabel ?alias . FILTER (LANG(?alias) = \"en\") } } GROUP BY ?ent ?ent_name ?ent_description ?links ORDER BY DESC(?links)" \
	--data-urlencode access-token=$(WD_ACCESS_TOKEN) --data-urlencode timeout=$(TIMEOUT) \
	> $(OUT_DIR)/wikidata-entities.tsv
	@curl -s $(FB_ENDPOINT) -H "Accept: text/tab-separated-values" \
	--data-urlencode query="PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> PREFIX fb: <http://rdf.freebase.com/ns/> PREFIX skos: <http://www.w3.org/2004/02/skos/core#> PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> SELECT DISTINCT ?ent ?ent_name ?ent_description ?links (GROUP_CONCAT(DISTINCT ?type; SEPARATOR=\"; \") AS ?types) (GROUP_CONCAT(DISTINCT ?notable; SEPARATOR=\"; \") AS ?notables) (GROUP_CONCAT(DISTINCT ?alias; SEPARATOR=\"; \") AS ?aliases) (GROUP_CONCAT(DISTINCT ?key; SEPARATOR=\"; \") AS ?keys) WHERE { ?ent fb:type.object.name ?ent_name . FILTER(LANG(?ent_name) = \"en\") OPTIONAL { ?ent fb:common.topic.description ?ent_description . FILTER(LANG(?ent_description) = \"en\") } OPTIONAL { ?ent fb:freebase.type_profile.instance_count ?links } OPTIONAL { ?ent fb:type.object.type ?type_ . ?type_ fb:type.object.name ?type . FILTER(LANG(?type) = \"en\") } OPTIONAL { ?ent fb:common.topic.notable_types ?notable_ . ?notable_ fb:type.object.name ?notable . FILTER(LANG(?notable) = \"en\") } OPTIONAL { ?ent fb:type.object.key ?key . FILTER(LANG(?key) = \"en\") } OPTIONAL { ?ent fb:common.topic.alias ?alias . FILTER(LANG(?alias) = \"en\") } } GROUP BY ?ent ?ent_name ?ent_description ?links ORDER BY DESC(?links)" \
	--data-urlencode access-token=$(FB_ACCESS_TOKEN) --data-urlencode timeout=$(TIMEOUT) \
	> $(OUT_DIR)/freebase-entities.tsv
	# @curl -s $(DB_ENDPOINT) -H "Accept: text/tab-separated-values" \
	# --data-urlencode query="PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> PREFIX dbo: <http://dbpedia.org/ontology/> PREFIX dbr: <http://dbpedia.org/resource/> SELECT ?ent ?ent_name ?ent_description ?ent_count (GROUP_CONCAT(DISTINCT ?type; SEPARATOR = \"; \") AS ?types) (GROUP_CONCAT(DISTINCT ?alias; SEPARATOR = \"; \") AS ?aliases) WHERE { { SELECT ?ent (COUNT(?ent) AS ?ent_count) WHERE { ?ent ?p ?obj } GROUP BY ?ent } ?ent rdfs:label ?ent_name . FILTER(LANG(?ent_name) = \"en\") . FILTER(REGEX(STR(?ent), \"^http://dbpedia.org/resource/\")) . BIND(\"\" AS ?ent_description) OPTIONAL { ?ent dbo:alias ?alias . FILTER (LANG(?alias) = \"en\") } OPTIONAL { { ?ent rdfs:subClassOf ?type } UNION { ?ent rdf:type ?type } FILTER(REGEX(STR(?type), \"^http://dbpedia.org/ontology/\")) } } GROUP BY ?ent ?ent_name ?ent_description ?ent_count ORDER BY DESC(?ent_count)" \
	# --data-urlencode access-token=$(DB_ACCESS_TOKEN) --data-urlencode timeout=$(TIMEOUT) \
	# > $(OUT_DIR)/dbpedia-entities.tsv

.PHONY: download_redirects
download_redirects:
	@mkdir -p $(OUT_DIR)
	@curl -s $(WD_ENDPOINT) -H "Accept: text/tab-separated-values" \
	--data-urlencode query="PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> PREFIX owl: <http://www.w3.org/2002/07/owl#> SELECT ?ent (GROUP_CONCAT(DISTINCT ?redir; SEPARATOR = \"; \") AS ?redirs) WHERE { ?redir owl:sameAs ?ent . FILTER(REGEX(STR(?ent), \"entity/Q\\\\d+\")) } GROUP BY ?ent" \
	--data-urlencode access-token=$(WD_ACCESS_TOKEN) --data-urlencode timeout=$(TIMEOUT) \
	> $(OUT_DIR)/wikidata-entity-redirects.tsv
	@curl -s $(DB_ENDPOINT) -H "Accept: text/tab-separated-values" \
	--data-urlencode query="PREFIX dbo: <http://dbpedia.org/ontology/> PREFIX dbr: <http://dbpedia.org/resource/> SELECT ?target (GROUP_CONCAT(DISTINCT ?source; SEPARATOR = \"; \") as ?sources) WHERE { ?source dbo:wikiPageRedirects ?target } GROUP BY ?target" \
	--data-urlencode access-token=$(DB_ACCESS_TOKEN) --data-urlencode timeout=$(TIMEOUT) \
	> $(OUT_DIR)/dbpedia-entity-redirects.tsv

.PHONY: compute_entities
compute_entities:
	@mkdir -p $(OUT_DIR)/wikidata-entities
	@$(CARGO) run --bin kg-entities --release -- \
		--file $(OUT_DIR)/wikidata-entities.tsv \
		--output $(OUT_DIR)/wikidata-entities \
		--check-for-popular-aliases \
		--keep-most-common-non-unique \
		--redirects $(OUT_DIR)/wikidata-entity-redirects.tsv \
		--knowledge-base wikidata \
		--ignore-types \
		> $(OUT_DIR)/wikidata-entities/output.txt
	@mkdir -p $(OUT_DIR)/freebase-entities
	@$(CARGO) run --bin kg-entities --release -- \
		--file $(OUT_DIR)/freebase-entities.tsv \
		--output $(OUT_DIR)/freebase-entities \
		--check-for-popular-aliases \
		--keep-most-common-non-unique \
		--knowledge-base freebase \
		--ignore-types \
		> $(OUT_DIR)/freebase-entities/output.txt
	# @mkdir -p $(OUT_DIR)/dbpedia-entities
	# @$(CARGO) run --bin kg-entities --release -- \
	# 	--file $(OUT_DIR)/dbpedia-entities.tsv \
	# 	--output $(OUT_DIR)/dbpedia-entities \
	# 	--check-for-popular-aliases \
	# 	--keep-most-common-non-unique \
	# 	--redirects $(OUT_DIR)/dbpedia-entity-redirects.tsv \
	# 	--knowledge-base dbpedia \
	# 	--ignore-types \
	# 	> $(OUT_DIR)/dbpedia-entities/output.txt

.PHONY: download
download: download_properties download_redirects download_entities

.PHONY: compute
compute: compute_properties compute_entities

.PHONY: code
code:
	cargo fmt --all
	cargo clippy -- -D warnings
	cargo test
