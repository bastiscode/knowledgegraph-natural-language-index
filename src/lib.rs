use std::cmp::Ordering;
use std::fmt::Display;
use std::io::BufRead;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::{fs, io::BufReader};

use anyhow::{anyhow, bail};

use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use regex::Regex;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd)]
pub enum Ent<'a> {
    Label(&'a str),
    LabelInfo(&'a str),
    Alias(&'a str),
    AliasInfo(&'a str),
}

impl<'s> Ent<'s> {
    pub fn as_str(&self) -> &'s str {
        match self {
            Ent::Label(s) | Ent::LabelInfo(s) => s,
            Ent::Alias(s) | Ent::AliasInfo(s) => s,
        }
    }
}

impl Ord for Ent<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Ent::Label(_), Ent::LabelInfo(_) | Ent::Alias(_) | Ent::AliasInfo(_)) => {
                Ordering::Less
            }
            (Ent::LabelInfo(_), Ent::Alias(_) | Ent::AliasInfo(_)) => Ordering::Less,
            (Ent::Alias(_), Ent::AliasInfo(_)) => Ordering::Less,
            (Ent::AliasInfo(_), Ent::Label(_) | Ent::LabelInfo(_) | Ent::Alias(_)) => {
                Ordering::Greater
            }
            (Ent::Alias(_), Ent::Label(_) | Ent::LabelInfo(_)) => Ordering::Greater,
            (Ent::LabelInfo(_), Ent::Label(_)) => Ordering::Greater,
            _ => Ordering::Equal,
        }
    }
}

impl Display for Ent<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

pub struct EntityInfo<'a> {
    pub label: &'a str,
    pub desc: &'a str,
    pub aliases: Vec<&'a str>,
    pub types: Arc<Mutex<Vec<&'a str>>>,
    pub count: usize,
    pub redirects: Option<&'a Vec<String>>,
}

impl<'a> EntityInfo<'a> {
    pub fn info(&self) -> &str {
        self.types
            .lock()
            .unwrap()
            .last()
            .copied()
            .unwrap_or(self.desc)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd)]
pub enum Prop<'a> {
    Label(&'a str),
    Alias(&'a str),
}

impl Ord for Prop<'_> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match (self, other) {
            (Prop::Label(_), Prop::Alias(_)) => Ordering::Less,
            (Prop::Alias(_), Prop::Label(_)) => Ordering::Greater,
            _ => Ordering::Equal,
        }
    }
}

impl<'s> Prop<'s> {
    pub fn as_str(&self) -> &'s str {
        match self {
            Prop::Label(s) => s,
            Prop::Alias(s) => s,
        }
    }
}

impl Display for Prop<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

pub struct PropInfo<'a> {
    pub label: String,
    pub aliases: Vec<&'a str>,
    pub inverses: Vec<&'a str>,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum KnowledgeGraph {
    Wikidata,
    Freebase,
    DBPedia,
}

impl TryFrom<&str> for KnowledgeGraph {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Ok(match value {
            "wikidata" => KnowledgeGraph::Wikidata,
            "freebase" => KnowledgeGraph::Freebase,
            "dbpedia" => KnowledgeGraph::DBPedia,
            _ => return Err(anyhow!("invalid knowledge base {}", value)),
        })
    }
}

pub struct KnowledgeGraphProcessor {
    pub label_pattern: Regex,
    pub prop_pattern: Regex,
    pub ent_pattern: Regex,
    pub kg: KnowledgeGraph,
}

impl KnowledgeGraphProcessor {
    pub fn new(kg: KnowledgeGraph) -> anyhow::Result<Self> {
        let prop_pattern = Regex::new(match kg {
            KnowledgeGraph::Wikidata => r"<?http://www.wikidata.org/entity/(P\d+)>?",
            KnowledgeGraph::Freebase => r"<?http://rdf.freebase.com/ns/(.+)>?",
            KnowledgeGraph::DBPedia => r"<?http://dbpedia.org/((?:property|ontology)/.+)>?",
        })?;
        let label_pattern = Regex::new("^\"(.*)\"@en$")?;
        let ent_pattern = Regex::new(match kg {
            KnowledgeGraph::Wikidata => r"<?http://www.wikidata.org/entity/(Q\d+)>?",
            KnowledgeGraph::Freebase => r"<?http://rdf.freebase.com/ns/(m\..+)>?",
            KnowledgeGraph::DBPedia => r"<?http://dbpedia.org/resource/(.+)>?",
        })?;

        Ok(Self {
            label_pattern,
            prop_pattern,
            ent_pattern,
            kg,
        })
    }

    #[inline]
    pub fn parse_property<'s>(&self, line: &'s str) -> anyhow::Result<(Prop<'s>, PropInfo<'s>)> {
        let splits: Vec<_> = line.split_terminator('\t').collect();
        if splits.len() < 2 || splits.len() > 5 {
            bail!("invalid property line: {}", line);
        }
        let prop = splits[0].trim_start_matches('<').trim_end_matches('>');
        let Some(label) = self.label_pattern.captures(splits[1]) else {
            bail!("failed to capture label in {}", splits[1]);
        };
        let label = label.get(1).unwrap().as_str().trim();

        let label = match self.kg {
            KnowledgeGraph::Wikidata => label.to_string(),
            KnowledgeGraph::DBPedia => {
                if prop.starts_with("ontology") {
                    format!("{label} (ontology)")
                } else {
                    label.to_string()
                }
            }
            KnowledgeGraph::Freebase => {
                let splits: Vec<_> = prop.split_terminator('.').collect();
                if splits.len() < 2 {
                    bail!("invalid freebase property: {}", prop);
                }
                format!("{label} ({})", splits[splits.len() - 2].replace('_', " "))
            }
        };
        let aliases = splits[3]
            .trim_start_matches('"')
            .trim_end_matches('"')
            .split_terminator(';')
            .map(str::trim)
            .collect();
        let inverses = if splits.len() == 5 {
            splits[4]
                .split_terminator(';')
                .filter_map(|s| {
                    self.prop_pattern
                        .captures(s.trim())?
                        .get(1)
                        .map(|m| m.as_str())
                })
                .collect()
        } else {
            vec![]
        };
        Ok((
            Prop::Label(prop),
            PropInfo {
                label,
                count: splits[2].parse()?,
                aliases,
                inverses,
            },
        ))
    }

    #[inline]
    pub fn parse_entity<'s>(
        &self,
        line: &'s str,
        ignore_types: bool,
    ) -> anyhow::Result<(Ent<'s>, EntityInfo<'s>)> {
        let splits: Vec<_> = line.split_terminator('\t').collect();
        if splits.len() < 2 || splits.len() > 6 {
            bail!("invalid entity line: {}", line);
        }
        let ent = splits[0].trim_start_matches('<').trim_end_matches('>');
        let Some(label) = self.label_pattern.captures(splits[1]) else {
            bail!("failed to capture label in {}", splits[1]);
        };
        let label = label.get(1).unwrap().as_str().trim();
        let desc = if let Some(desc) = self.label_pattern.captures(splits[2]) {
            desc.get(1).unwrap().as_str().trim()
        } else {
            ""
        };
        let types = Arc::new(Mutex::new(if ignore_types {
            vec![]
        } else {
            splits[4]
                .split_terminator(';')
                .filter_map(|s| {
                    let cap = self.ent_pattern.captures(s)?;
                    Some(cap.get(1).unwrap().as_str().trim())
                })
                .collect()
        }));
        let aliases = if splits.len() == 6 {
            splits[5]
                .trim_start_matches('"')
                .trim_end_matches('"')
                .split_terminator(';')
                .map(str::trim)
                .collect()
        } else {
            vec![]
        };
        Ok((
            Ent::Label(ent),
            EntityInfo {
                label,
                desc,
                count: splits[3].parse()?,
                types,
                aliases,
                redirects: None,
            },
        ))
    }

    #[inline]
    pub fn format_property(
        &self,
        p: &str,
        short: bool,
        pfx: Option<&str>,
    ) -> anyhow::Result<String> {
        if !short {
            return Ok(p.to_string());
        }
        let Some(p) = self.prop_pattern.captures(p) else {
            bail!("failed to capture property in {}", p);
        };
        let p = p.get(1).unwrap().as_str();
        Ok(match self.kg {
            KnowledgeGraph::Wikidata => format!("{}:{p}", pfx.unwrap_or("wdt")),
            KnowledgeGraph::Freebase => format!("{}:{p}", pfx.unwrap_or("fb")),
            KnowledgeGraph::DBPedia => {
                let dbp_regex = Regex::new(r"(property|ontology)/([^>]+)").unwrap();
                let captures = dbp_regex
                    .captures(p)
                    .ok_or_else(|| anyhow!("invalid dbpedia property: {}", p))?;
                let p_type = captures.get(1).unwrap().as_str();
                let p = captures.get(2).unwrap().as_str();
                let pfx = if p_type == "ontology" { "dbo" } else { "dbp" };
                format!("{pfx}:{p}")
            }
        })
    }

    pub fn entity_prefixes(&self) -> Vec<(&str, &str)> {
        match self.kg {
            KnowledgeGraph::Wikidata => vec![("wd:", "http://www.wikidata.org/entity/")],
            KnowledgeGraph::Freebase => vec![("fb:", "http://rdf.freebase.com/ns/")],
            KnowledgeGraph::DBPedia => vec![("dbr:", "http://dbpedia.org/resource/")],
        }
    }

    pub fn property_prefixes(&self) -> Vec<(&str, &str)> {
        match self.kg {
            KnowledgeGraph::Wikidata => vec![
                ("wdt:", "http://www.wikidata.org/prop/direct/"),
                ("p:", "http://www.wikidata.org/prop/"),
                ("pq:", "http://www.wikidata.org/prop/qualifier/"),
                (
                    "pqn:",
                    "http://www.wikidata.org/prop/qualifier/value-normalized/",
                ),
                ("ps:", "http://www.wikidata.org/prop/statement/"),
                (
                    "psn:",
                    "http://www.wikidata.org/prop/statement/value-normalized/",
                ),
            ],
            KnowledgeGraph::Freebase => vec![("fbp:", "http://rdf.freebase.com/ns/property.")],
            KnowledgeGraph::DBPedia => vec![
                ("dbp:", "http://dbpedia.org/property/"),
                ("dbo:", "http://dbpedia.org/ontology/"),
            ],
        }
    }

    #[inline]
    pub fn format_entity(&self, e: &str, short: bool) -> anyhow::Result<String> {
        if !short {
            return Ok(e.to_string());
        }
        let Some(e) = self.ent_pattern.captures(e) else {
            bail!("failed to capture entity in {}", e);
        };
        let e = e.get(1).unwrap().as_str();
        Ok(match self.kg {
            KnowledgeGraph::Wikidata => format!("wd:{}", e),
            KnowledgeGraph::Freebase => format!("fb:{}", e),
            KnowledgeGraph::DBPedia => format!("dbr:{}", e),
        })
    }
}

pub fn wikidata_qualifiers(label: &str) -> Vec<(String, String)> {
    vec![
        (format!("{label} (statement)"), "p".to_string()),
        (format!("{label} (qualifier)"), "pq".to_string()),
        (format!("{label} (normalized qualifier)"), "pqn".to_string()),
        (format!("{label} (value)"), "ps".to_string()),
        (format!("{label} (normalized value)"), "psn".to_string()),
    ]
}

pub fn line_iter(
    file: impl AsRef<Path>,
) -> anyhow::Result<impl Iterator<Item = anyhow::Result<String>>> {
    let file = fs::File::open(file)?;
    let file = BufReader::new(file);
    Ok(file.lines().map(|line| line.map_err(anyhow::Error::from)))
}

pub fn progress_bar(msg: &str, size: u64, hidden: bool) -> ProgressBar {
    let pb = ProgressBar::new(size)
        .with_style(
            ProgressStyle::with_template(
                "{msg}: {wide_bar} [{pos}/{len}] [{elapsed_precise}|{eta_precise}]",
            )
            .unwrap(),
        )
        .with_message(msg.to_string());
    if hidden {
        pb.set_draw_target(ProgressDrawTarget::hidden());
    }
    pb
}
