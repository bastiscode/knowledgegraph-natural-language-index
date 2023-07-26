use std::cmp::Ordering;
use std::fmt::Display;
use std::io::BufRead;
use std::path::Path;
use std::{fs, io::BufReader};

use anyhow::{anyhow, bail};

use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use regex::Regex;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord)]
pub enum Ent<'a> {
    Label(&'a str),
    LabelDesc(&'a str),
    Alias(&'a str),
    AliasDesc(&'a str),
}

impl<'s> Ent<'s> {
    pub fn as_str(&self) -> &'s str {
        match self {
            Ent::Label(s) | Ent::LabelDesc(s) => s,
            Ent::Alias(s) | Ent::AliasDesc(s) => s,
        }
    }
}

impl PartialOrd for Ent<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(match (self, other) {
            (Ent::Label(_), Ent::LabelDesc(_) | Ent::Alias(_) | Ent::AliasDesc(_)) => {
                Ordering::Less
            }
            (Ent::LabelDesc(_), Ent::Alias(_) | Ent::AliasDesc(_)) => Ordering::Less,
            (Ent::Alias(_), Ent::AliasDesc(_)) => Ordering::Less,
            (Ent::AliasDesc(_), Ent::Label(_) | Ent::LabelDesc(_) | Ent::Alias(_)) => {
                Ordering::Greater
            }
            (Ent::Alias(_), Ent::Label(_) | Ent::LabelDesc(_)) => Ordering::Greater,
            (Ent::LabelDesc(_), Ent::Label(_)) => Ordering::Greater,
            _ => Ordering::Equal,
        })
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
    pub count: usize,
    pub redirects: Option<&'a Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord)]
pub enum Prop<'a> {
    Label(&'a str),
    Alias(&'a str),
}

impl PartialOrd for Prop<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(match (self, other) {
            (Prop::Label(_), Prop::Alias(_)) => Ordering::Less,
            (Prop::Alias(_), Prop::Label(_)) => Ordering::Greater,
            _ => Ordering::Equal,
        })
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
    label_pattern: Regex,
    prop_pattern: Regex,
    ent_pattern: Regex,
    kg: KnowledgeGraph,
}

impl KnowledgeGraphProcessor {
    pub fn new(kg: KnowledgeGraph) -> anyhow::Result<Self> {
        let prop_pattern = Regex::new(match kg {
            KnowledgeGraph::Wikidata => r"<?http://www.wikidata.org/entity/(P\d+)>?",
            KnowledgeGraph::Freebase => r"<?http://rdf.freebase.com/ns/([^>]+)>?",
            KnowledgeGraph::DBPedia => r"<?http://dbpedia.org/((?:property|ontology)/[^>]+)>?",
        })?;
        let label_pattern = Regex::new("^\"(.*)\"@en$")?;
        let ent_pattern = Regex::new(match kg {
            KnowledgeGraph::Wikidata => r"<?http://www.wikidata.org/entity/(Q\d+)>?",
            KnowledgeGraph::Freebase => r"<?http://rdf.freebase.com/ns/(m\.[^>]+)>?",
            KnowledgeGraph::DBPedia => r"<?http://dbpedia.org/resource/[^>]+)>?",
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
        let Some(prop) = self.prop_pattern.captures(splits[0]) else {
            bail!("failed to capture property in {}", splits[0]);
        };
        let prop = prop.get(1).unwrap().as_str().trim();
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
        let aliases = splits[3].split_terminator(';').map(str::trim).collect();
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
    pub fn parse_entity<'s>(&self, line: &'s str) -> anyhow::Result<(Ent<'s>, EntityInfo<'s>)> {
        let splits: Vec<_> = line.split_terminator('\t').collect();
        if splits.len() < 2 || splits.len() > 5 {
            bail!("invalid entity line: {}", line);
        }
        let Some(ent) = self.ent_pattern.captures(splits[0]) else {
            bail!("failed to capture entity in {}", splits[0]);
        };
        let ent = ent.get(1).unwrap().as_str().trim();
        let Some(label) = self.label_pattern.captures(splits[1]) else {
            bail!("failed to capture label in {}", splits[1]);
        };
        let label = label.get(1).unwrap().as_str().trim();
        let desc = if let Some(desc) = self.label_pattern.captures(splits[2]) {
            desc.get(1).unwrap().as_str().trim()
        } else {
            ""
        };
        let aliases = if splits.len() == 5 {
            splits[4].split_terminator(';').map(str::trim).collect()
        } else {
            vec![]
        };
        Ok((
            Ent::Label(ent),
            EntityInfo {
                label,
                desc,
                count: splits[3].parse()?,
                aliases,
                redirects: None,
            },
        ))
    }

    #[inline]
    pub fn format_property(&self, p: &str, pfx: Option<&str>) -> String {
        match self.kg {
            KnowledgeGraph::Wikidata => format!("{}:{p}", pfx.unwrap_or("wdt")),
            KnowledgeGraph::Freebase => format!("{}:{p}", pfx.unwrap_or("fb")),
            KnowledgeGraph::DBPedia => {
                let dbp_regex = Regex::new(r"(property|ontology)/([^>]+)").unwrap();
                let captures = dbp_regex.captures(p).unwrap();
                let p_type = captures.get(1).unwrap().as_str();
                let p = captures.get(2).unwrap().as_str();
                let pfx = if p_type == "ontology" { "dbo" } else { "dbp" };
                format!("{pfx}:{p}")
            }
        }
    }

    #[inline]
    pub fn format_entity(&self, e: &str) -> String {
        match self.kg {
            KnowledgeGraph::Wikidata => format!("wd:{}", e),
            KnowledgeGraph::Freebase => format!("fb:{}", e),
            KnowledgeGraph::DBPedia => format!("dbr:{}", e),
        }
    }
}

pub fn wikidata_qualifiers(label: &str) -> Vec<(String, String)> {
    vec![
        (format!("{label} (statement)"), "p".to_string()),
        (format!("{label} (qualifier)"), "pq".to_string()),
        (format!("{label} (value)"), "ps".to_string()),
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
