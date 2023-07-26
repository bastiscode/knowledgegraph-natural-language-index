OUT_DIR=.
CARGO=cargo

.PHONY: download_properties
download_properties:
	@mkdir -p $(OUT_DIR)
	@curl -s https://qlever.cs.uni-freiburg.de/api/wikidata -H "Accept: text/tab-separated-values" -H "Content-type: application/sparql-query" --data "PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> PREFIX skos: <http://www.w3.org/2004/02/skos/core#> PREFIX wdt: <http://www.wikidata.org/prop/direct/> PREFIX wd: <http://www.wikidata.org/entity/> PREFIX wikibase: <http://wikiba.se/ontology#> PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> SELECT ?p ?p_label ?p_count (GROUP_CONCAT(DISTINCT ?p_alias; SEPARATOR = \"; \") AS ?p_aliases) (GROUP_CONCAT(DISTINCT ?p_inv; SEPARATOR = \"; \") AS ?p_invs) WHERE { ?p wikibase:directClaim ?claim . ?p rdfs:label ?p_label . FILTER(LANG(?p_label) = \"en\") . BIND(0 AS ?p_count) . OPTIONAL { ?p skos:altLabel ?p_alias . FILTER(LANG(?p_alias) = \"en\") } OPTIONAL { ?p wdt:P1696 ?p_inv } } GROUP BY ?p ?p_label ?p_count" \
	> $(OUT_DIR)/wikidata-properties.tsv
	@curl -s https://qlever.informatik.uni-freiburg.de/api/freebase -H "Accept: text/tab-separated-values" -H "Content-type: application/sparql-query" --data "PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> PREFIX dbo: <http://dbpedia.org/ontology/> SELECT ?p ?p_label ?p_count (GROUP_CONCAT(DISTINCT ?alias; SEPARATOR = \"; \") AS ?aliases) ?inv_p WHERE { { SELECT ?p (COUNT(?p) as ?p_count) WHERE { ?s ?p ?o } GROUP BY ?p } ?p rdfs:label ?p_label . FILTER(LANG(?p_label) = \"en\") . OPTIONAL { ?p dbo:alias ?alias . FILTER (LANG(?alias) = \"en\") } BIND(\"\" AS ?inv_p) } GROUP BY ?p ?p_label ?p_count ?inv_p ORDER BY DESC(?p_count)" \
	> $(OUT_DIR)/freebase-properties.tsv
	@curl -s https://qlever.cs.uni-freiburg.de/api/dbpedia -H "Accept: text/tab-separated-values" -H "Content-type: application/sparql-query" --data "PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> PREFIX dbo: <http://dbpedia.org/ontology/> SELECT ?p ?p_label ?p_count (GROUP_CONCAT(DISTINCT ?alias; SEPARATOR = \"; \") AS ?aliases) (GROUP_CONCAT(DISTINCT ?inv_p; SEPARATOR = \"; \") AS ?inverse) WHERE { { SELECT ?p (COUNT(?p) as ?p_count) WHERE { ?s ?p ?o } GROUP BY ?p } ?p rdfs:label ?p_label . FILTER(LANG(?p_label) = \"en\") . OPTIONAL { ?p dbo:alias ?alias . FILTER (LANG(?alias) = \"en\") } OPTIONAL { ?inv_p dbo:inverseOf ?p } } GROUP BY ?p ?p_label ?p_count ORDER BY DESC(?p_count)" \
	> $(OUT_DIR)/dbpedia-properties.tsv

.PHONY: compute_properties
compute_properties:
	@$(CARGO) run --bin kg-properties --release -- \
		--file $(OUT_DIR)/wikidata-properties.tsv \
		--output $(OUT_DIR)/wikidata-properties-index.tsv \
		--inverse-output $(OUT_DIR)/wikidata-properties-inverse-index.tsv \
		--knowledge-base wikidata \
		> $(OUT_DIR)/wikidata-properties-output.txt
	@$(CARGO) run --bin kg-properties --release -- \
		--file $(OUT_DIR)/wikidata-properties.tsv \
		--output $(OUT_DIR)/wikidata-properties-full-index.tsv \
		--include-wikidata-qualifiers \
		--knowledge-base wikidata \
		> $(OUT_DIR)/wikidata-properties-full-output.txt
	@$(CARGO) run --bin kg-properties --release -- \
		--file $(OUT_DIR)/freebase-properties.tsv \
		--output $(OUT_DIR)/freebase-properties-index.tsv \
		--knowledge-base freebase \
		> $(OUT_DIR)/freebase-properties-output.txt
	@$(CARGO) run --bin kg-properties --release -- \
		--file $(OUT_DIR)/dbpedia-properties.tsv \
		--output $(OUT_DIR)/dbpedia-properties-index.tsv \
		--knowledge-base dbpedia \
		> $(OUT_DIR)/dbpedia-properties-output.txt

.PHONY: download_entities
download_entities:
	@mkdir -p $(OUT_DIR)
	@curl -s https://qlever.informatik.uni-freiburg.de/api/freebase -H "Accept: text/tab-separated-values" -H "Content-type: application/sparql-query" --data "PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> PREFIX fb: <http://rdf.freebase.com/ns/> PREFIX skos: <http://www.w3.org/2004/02/skos/core#> SELECT ?ent ?ent_name ?ent_description ?ent_count (GROUP_CONCAT(DISTINCT ?alias; SEPARATOR = \"; \") AS ?aliases) WHERE { { SELECT ?ent (COUNT(?ent) as ?ent_count) WHERE { ?ent ?p ?obj } GROUP BY ?ent } ?ent rdfs:label ?ent_name . FILTER (LANG(?ent_name) = \"en\") . FILTER (REGEX(str(?ent), \"^http://rdf.freebase.com/ns/m\\\\..*\$\")) BIND(\"\" AS ?ent_description) OPTIONAL { ?ent fb:common.topic.alias ?alias . FILTER (LANG(?alias) = \"en\") } } GROUP BY ?ent ?ent_name ?ent_description ?ent_count ORDER BY DESC(?ent_count)" \
	> $(OUT_DIR)/freebase-entities.tsv
	@curl -s https://qlever.cs.uni-freiburg.de/api/dbpedia -H "Accept: text/tab-separated-values" -H "Content-type: application/sparql-query" --data "PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> PREFIX dbo: <http://dbpedia.org/ontology/> PREFIX dbr: <http://dbpedia.org/resource/> SELECT ?ent ?ent_name ?ent_description ?ent_count (GROUP_CONCAT(DISTINCT ?alias; SEPARATOR = \"; \") AS ?aliases) WHERE { { SELECT ?ent (COUNT(?ent) AS ?ent_count) WHERE { ?ent ?p ?obj } GROUP BY ?ent } ?ent rdfs:label ?ent_name . FILTER (LANG(?ent_name) = \"en\") . BIND(\"\" AS ?ent_description) OPTIONAL { ?ent dbo:alias ?alias . FILTER (LANG(?alias) = \"en\") } } GROUP BY ?ent ?ent_name ?ent_description ?ent_count ORDER BY DESC(?ent_count)" \
	> $(OUT_DIR)/dbpedia-entities.tsv
	# @curl -s https://qlever.cs.uni-freiburg.de/api/wikidata -H "Accept: text/tab-separated-values" -H "Content-type: application/sparql-query" --data "PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> PREFIX skos: <http://www.w3.org/2004/02/skos/core#> PREFIX wikibase: <http://wikiba.se/ontology#> PREFIX schema: <http://schema.org/> PREFIX wdt: <http://www.wikidata.org/prop/direct/> SELECT ?ent ?ent_name ?ent_description (MAX(?sitelinks) AS ?links) (GROUP_CONCAT(DISTINCT ?alias; SEPARATOR = \"; \") AS ?aliases) WHERE { { SELECT DISTINCT ?ent WHERE { ?ent wdt:P279*/wdt:P18 ?pic } } UNION { SELECT DISTINCT ?ent WHERE { ?ent wdt:P31*/wdt:P18 ?pic } } UNION { SELECT DISTINCT ?ent WHERE { ?ent ^schema:about/schema:isPartOf ?wiki . FILTER(REGEX(STR(?wiki), \"^https?://.*.wikipedia.org\")) } } ?ent rdfs:label ?ent_name . FILTER (LANG(?ent_name) = \"en\") . FILTER(REGEX(STR(?ent), \"entity/Q\\\\d+\")) . OPTIONAL { ?ent ^schema:about/wikibase:sitelinks ?sitelinks } OPTIONAL { ?ent schema:description ?ent_description . FILTER (LANG(?ent_description) = \"en\") } OPTIONAL { ?ent skos:altLabel ?alias . FILTER (LANG(?alias) = \"en\") } } GROUP BY ?ent ?ent_name ?ent_description ORDER BY DESC(?links)" \
	# > $(OUT_DIR)/wikidata-entities.tsv
	# @curl -s https://qlever.cs.uni-freiburg.de/api/wikidata -H "Accept: text/tab-separated-values" -H "Content-type: application/sparql-query" --data "PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> PREFIX skos: <http://www.w3.org/2004/02/skos/core#> PREFIX wikibase: <http://wikiba.se/ontology#> PREFIX schema: <http://schema.org/> PREFIX wdt: <http://www.wikidata.org/prop/direct/> SELECT ?ent ?ent_name ?ent_description (MAX(?sitelinks) AS ?links) (GROUP_CONCAT(DISTINCT ?alias; SEPARATOR = \"; \") AS ?aliases) WHERE { { SELECT ?ent WHERE { ?ent wdt:P18 ?pic } GROUP BY ?ent } UNION { SELECT ?ent WHERE { ?ent ^schema:about/schema:isPartOf ?wiki . FILTER(REGEX(STR(?wiki), \"^https?://.*.wikipedia.org\")) } GROUP BY ?ent } ?ent rdfs:label ?ent_name . FILTER (LANG(?ent_name) = \"en\") . FILTER(REGEX(STR(?ent), \"entity/Q\\\\d+\")) . OPTIONAL { ?ent ^schema:about/wikibase:sitelinks ?sitelinks } OPTIONAL { ?ent schema:description ?ent_description . FILTER (LANG(?ent_description) = \"en\") } OPTIONAL { ?ent skos:altLabel ?alias . FILTER (LANG(?alias) = \"en\") } } GROUP BY ?ent ?ent_name ?ent_description ORDER BY DESC(?links)" \
	# > $(OUT_DIR)/wikidata-entities-small.tsv

.PHONY: download_redirects
download_redirects:
	@mkdir -p $(OUT_DIR)
	@curl -s https://qlever.cs.uni-freiburg.de/api/wikidata -H "Accept: text/tab-separated-values" -H "Content-type: application/sparql-query" --data "PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> PREFIX owl: <http://www.w3.org/2002/07/owl#> SELECT ?ent (GROUP_CONCAT(DISTINCT ?redir; SEPARATOR = \"\\t\") AS ?redirs) WHERE { ?redir owl:sameAs ?ent . FILTER(REGEX(STR(?ent), \"entity/Q\\\\d+\")) } GROUP BY ?ent" \
	> $(OUT_DIR)/wikidata-entity-redirects.tsv
	@curl -s https://qlever.cs.uni-freiburg.de/api/dbpedia -H "Accept: text/tab-separated-values" -H "Content-type: application/sparql-query" --data "PREFIX dbo: <http://dbpedia.org/ontology/> PREFIX dbr: <http://dbpedia.org/resource/> SELECT ?target (GROUP_CONCAT(DISTINCT ?source; SEPARATOR = \"\\t\") as ?sources) WHERE { ?source dbo:wikiPageRedirects ?target } GROUP BY ?target" \
	> $(OUT_DIR)/dbpedia-entity-redirects.tsv

.PHONY: compute_entities
compute_entities:
	@$(CARGO) run --bin kg-entities --release -- \
		--file $(OUT_DIR)/wikidata-entities.tsv \
		--output $(OUT_DIR)/wikidata-entities-index.tsv \
		--keep-most-common-non-unique \
		--full-ids \
		--redirects $(OUT_DIR)/wikidata-entity-redirects.tsv \
		--knowledge-base wikidata \
		> $(OUT_DIR)/wikidata-entities-output.txt
	@$(CARGO) run --bin kg-entities --release -- \
		--file $(OUT_DIR)/wikidata-entities.tsv \
		--output $(OUT_DIR)/wikidata-entities-popular-index.tsv \
		--keep-most-common-non-unique \
		--check-for-popular-aliases \
		--full-ids \
		--redirects $(OUT_DIR)/wikidata-entity-redirects.tsv \
		--knowledge-base wikidata \
		> $(OUT_DIR)/wikidata-entities-popular-output.txt
	@$(CARGO) run --bin kg-entities --release -- \
		--file $(OUT_DIR)/wikidata-entities-small.tsv \
		--output $(OUT_DIR)/wikidata-entities-small-index.tsv \
		--keep-most-common-non-unique \
		--full-ids \
		--redirects $(OUT_DIR)/wikidata-entity-redirects.tsv \
		--knowledge-base wikidata \
		> $(OUT_DIR)/wikidata-entities-small-output.txt
	@$(CARGO) run --bin kg-entities --release -- \
		--file $(OUT_DIR)/wikidata-entities-small.tsv \
		--output $(OUT_DIR)/wikidata-entities-small-popular-index.tsv \
		--keep-most-common-non-unique \
		--check-for-popular-aliases \
		--full-ids \
		--redirects $(OUT_DIR)/wikidata-entity-redirects.tsv \
		--knowledge-base wikidata \
		> $(OUT_DIR)/wikidata-entities-small-popular-output.txt
	@$(CARGO) run --bin kg-entities --release -- \
		--file $(OUT_DIR)/freebase-entities.tsv \
		--output $(OUT_DIR)/freebase-entities-index.tsv \
		--keep-most-common-non-unique \
		--full-ids \
		--knowledge-base freebase \
		> $(OUT_DIR)/freebase-entities-output.txt
	@$(CARGO) run --bin kg-entities --release -- \
		--file $(OUT_DIR)/freebase-entities.tsv \
		--output $(OUT_DIR)/freebase-entities-popular-index.tsv \
		--keep-most-common-non-unique \
		--check-for-popular-aliases \
		--full-ids \
		--knowledge-base freebase \
		> $(OUT_DIR)/freebase-entities-popular-output.txt


.PHONY: download
download: download_properties download_redirects download_entities

.PHONY: compute
compute: compute_properties compute_entities

.PHONY: index
index: download compute
