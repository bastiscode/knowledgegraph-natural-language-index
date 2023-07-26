use rayon::prelude::*;
use std::{
    collections::{hash_map::Entry, HashMap, HashSet},
    fs,
    io::{BufWriter, Write},
    path::PathBuf,
    sync::{Arc, Mutex},
};

use clap::Parser;
use itertools::Itertools;
use regex::Regex;
use sparql_data_preparation::{
    line_iter, progress_bar, Ent, KnowledgeGraph, KnowledgeGraphProcessor,
};
use text_correction_utils::edit::distance;

#[derive(Parser, Debug)]
struct Args {
    #[clap(short, long)]
    file: PathBuf,

    #[clap(short, long)]
    output: PathBuf,

    #[clap(short, long)]
    redirects: Option<PathBuf>,

    #[clap(short, long)]
    progress: bool,

    #[clap(short, long)]
    keep_most_common_non_unique: bool,

    #[clap(short, long)]
    check_for_popular_aliases: bool,

    #[clap(short, long)]
    knowledge_base: String,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let kg = KnowledgeGraph::try_from(args.knowledge_base.as_str())?;
    let kg = KnowledgeGraphProcessor::new(kg)?;

    let redirects = if let Some(path) = args.redirects {
        let ent_pattern = Regex::new(r"http://www.wikidata.org/entity/(Q\d+)")?;
        let pbar = progress_bar("loading entity redirects", u64::MAX, !args.progress);
        let lines: Vec<_> = pbar
            .wrap_iter(line_iter(path)?)
            .collect::<anyhow::Result<_>>()?;
        let mut redirects = HashMap::new();
        let pbar = progress_bar(
            "processing entity redirects",
            lines.len() as u64,
            !args.progress,
        );
        for line in lines {
            pbar.inc(1);
            let splits: Vec<_> = line.split_terminator('\t').collect();
            assert!(splits.len() >= 2);
            let ent = if let Some(ent) = ent_pattern.captures(splits[0].trim()) {
                ent.get(1).unwrap().as_str().to_string()
            } else {
                continue;
            };
            let redirs: Vec<_> = splits[1..]
                .iter()
                .map(|s| {
                    ent_pattern
                        .captures(s.trim())
                        .unwrap()
                        .get(1)
                        .unwrap()
                        .as_str()
                        .to_string()
                })
                .collect();
            if redirs.is_empty() {
                continue;
            }
            redirects.insert(ent, redirs);
        }
        pbar.finish_and_clear();
        redirects
    } else {
        HashMap::new()
    };
    let mut ent_infos = HashMap::new();
    let mut label_to_ents = HashMap::new();
    let mut aliases_to_ents = HashMap::new();

    let pbar = progress_bar("loading wikidata entities", u64::MAX, !args.progress);
    let mut lines = pbar.wrap_iter(line_iter(&args.file)?);
    let header = lines.next().expect("file should have at least 1 line")?;
    let lines: Vec<_> = lines.collect::<anyhow::Result<_>>()?;
    assert_eq!(header.split_terminator('\t').collect::<Vec<_>>().len(), 5);
    pbar.finish_and_clear();
    let pbar = progress_bar(
        "processing wikidata entities",
        lines.len() as u64,
        !args.progress,
    );
    for line in &lines {
        pbar.inc(1);
        let (ent, mut info) = kg.parse_entity(line)?;

        label_to_ents
            .entry(info.label)
            .or_insert_with(Vec::new)
            .push(ent.clone());

        if args.check_for_popular_aliases {
            for &alias in &info.aliases {
                aliases_to_ents
                    .entry(alias)
                    .or_insert_with(Vec::new)
                    .push(ent.as_str());
            }
        }

        info.redirects = redirects.get(ent.as_str());
        let existing = ent_infos.insert(ent.as_str(), info);
        assert!(existing.is_none(), "entities should be unique");
    }
    pbar.finish_and_clear();

    let num_ents = ent_infos.len();

    // filter out aliases that are aliases for multiple entities
    aliases_to_ents.retain(|_, ents| ents.len() <= 1);

    let check_for_more_popular_alias = |label: &str, ent: &str| {
        let Some(alias_ents) = aliases_to_ents.get(label) else {
            return None;
        };
        let info = ent_infos.get(ent).unwrap();
        let Some((alias_ent, alias_count)) = alias_ents
            .iter()
            .filter_map(|&alias_ent| {
                if alias_ent == ent {
                    return None;
                }
                Some((alias_ent, ent_infos.get(alias_ent).unwrap().count))
            })
            .max_by_key(|&(_, count)| count) else {
            return None;
        };
        if alias_count > info.count {
            Some(alias_ent.to_string())
        } else {
            None
        }
    };

    // initialize the final label to entity mapping
    let mut label_to_ent: HashMap<(&str, Option<&str>), _> = HashMap::new();
    assert!(label_to_ents.values().map(|ents| ents.len()).sum::<usize>() == num_ents);
    let mut label_desc_to_ents = HashMap::new();
    let pbar = progress_bar(
        "adding unique labels",
        label_to_ents.len() as u64,
        !args.progress,
    );
    for (label, mut entities) in label_to_ents {
        pbar.inc(1);
        assert!(!entities.is_empty());
        if entities.len() <= 1 {
            let alias_ent = check_for_more_popular_alias(label, entities[0].as_str());
            if !args.check_for_popular_aliases || alias_ent.is_none() {
                let ent = entities.into_iter().next().unwrap();
                label_to_ent.insert((label, None), ent);
                continue;
            }
        } else if args.keep_most_common_non_unique {
            // if we have multiple entities with the same label, we keep the most common one
            // as the one being identified by just the label
            entities.sort_by_key(|ent| ent_infos.get(ent.as_str()).unwrap().count);
            // keep the most popular one only if its label is not an alias
            // of a more popular entity

            let alias_ent = check_for_more_popular_alias(label, entities.last().unwrap().as_str());
            if !args.check_for_popular_aliases || alias_ent.is_none() {
                let ent = entities.pop().unwrap();
                label_to_ent.insert((label, None), ent);
            }
        }
        // if the label alone is not unique, we add the description to it and try again
        for ent in entities {
            let desc = ent_infos.get(ent.as_str()).unwrap().desc;
            if desc.is_empty() {
                continue;
            }
            let label_desc = format!("{label} ({desc})");
            if label_to_ent.contains_key(&(label_desc.as_str(), None)) {
                continue;
            }
            label_desc_to_ents
                .entry((label, desc))
                .or_insert_with(Vec::new)
                .push(ent);
        }
    }
    pbar.finish_and_clear();
    drop(aliases_to_ents);
    let num_label_unique = label_to_ent.len();
    // assert!(label_to_ent.iter().unique_by(|&(_, ent)| ent).count() == label_to_ent.len());

    let mut ents_left: HashSet<_> = HashSet::new();
    let pbar = progress_bar(
        "adding label-description pairs",
        label_desc_to_ents.len() as u64,
        !args.progress,
    );
    for ((label, desc), entities) in &mut label_desc_to_ents {
        pbar.inc(1);
        if entities.len() <= 1 {
            label_to_ent.insert(
                (label, Some(desc)),
                Ent::LabelDesc(entities.iter_mut().next().unwrap().as_str()),
            );
            continue;
        } else if args.keep_most_common_non_unique {
            // same as above
            entities.sort_by_key(|ent| ent_infos.get(ent.as_str()).unwrap().count);
            label_to_ent.insert(
                (label, Some(desc)),
                Ent::LabelDesc(entities.last().unwrap().as_str()),
            );
        }
        // if the label and description are not unique
        // record the entities with no entry for statistics
        ents_left.extend(entities.iter().map(|e| e.as_str()).take(entities.len() - 1));
    }
    pbar.finish_and_clear();
    let num_label_desc_unique = label_to_ent.len();
    // assert!(label_to_ent.iter().unique_by(|&(_, ent)| ent).count() == label_to_ent.len());

    println!("Wikidata entities");
    println!("#################");
    println!("entities:                 {}", num_ents);
    println!("unique by label:          {}", num_label_unique);
    println!(
        "label coverage:           {:.2}%",
        100.0 * num_label_unique as f32 / num_ents as f32
    );
    println!("unique by label and desc: {}", num_label_desc_unique);
    println!(
        "label and desc coverage:  {:.2}%",
        100.0 * num_label_desc_unique as f32 / num_ents as f32
    );
    println!("entities left:            {}", ents_left.len());
    // free memory after logging
    drop(ents_left);

    // now we have all unique entities
    // go over aliases to make sure one entitiy can be found by multiple names
    let mut total_aliases = 0;
    let pbar = progress_bar("adding aliases", ent_infos.len() as u64, !args.progress);
    ent_infos
        .iter()
        .sorted_by_key(|&(_, info)| (info.count))
        .rev()
        .for_each(|(&ent, info)| {
            pbar.inc(1);
            total_aliases += info.aliases.len();
            for &alias in &info.aliases {
                if let Entry::Vacant(entry) = label_to_ent.entry((alias, None)) {
                    entry.insert(Ent::Alias(ent));
                    continue;
                } else if info.desc.is_empty() {
                    continue;
                }
                if let Entry::Vacant(entry) = label_to_ent.entry((alias, Some(info.desc))) {
                    entry.insert(Ent::AliasDesc(ent));
                }
            }
        });
    pbar.finish_and_clear();

    println!(
        "added unique aliases:     {} ({:.2}% of all aliases)",
        label_to_ent.len() - num_label_desc_unique,
        100.0 * (label_to_ent.len() - num_label_desc_unique) as f32 / total_aliases as f32
    );
    println!("final index size:         {}", label_to_ent.len());
    println!(
        "final index coverage:     {:.2}%",
        100.0
            * label_to_ent
                .iter()
                .unique_by(|&(_, ent)| ent.as_str())
                .count() as f32
            / num_ents as f32
    );

    let mut output_dict = HashMap::new();
    for (label, ent) in &label_to_ent {
        output_dict
            .entry(ent.as_str())
            .or_insert_with(Vec::new)
            .push((label, matches!(ent, Ent::Alias(_) | Ent::AliasDesc(_))));
    }

    let output = Arc::new(Mutex::new(BufWriter::new(fs::File::create(args.output)?)));
    let pbar = progress_bar("creating outputs", output_dict.len() as u64, !args.progress);
    output_dict.into_par_iter().try_for_each(|(ent, labels)| {
        pbar.inc(1);
        let org_label: Vec<_> = labels
            .iter()
            .filter_map(|&(&(label, desc), is_alias)| if desc.is_none() && !is_alias {
                Some(label)
            } else {
                None
            })
            .collect();
        let desc_label: Vec<_> = labels
            .iter()
            .filter_map(|&(&(label, desc), is_alias)| if desc.is_some() && !is_alias {
                Some(format!("{} ({})", label, desc.unwrap()))
            } else {
                None
            })
            .collect();
        let mut aliases = labels
            .iter()
            .filter_map(|&(&(label, desc), is_alias)| if desc.is_none() && is_alias {Some(label)} else {None})
            .collect::<Vec<_>>();
        let mut alias_descs = labels
            .iter()
            .filter_map(|&(&(label, desc), is_alias)| if desc.is_some() && is_alias {
                Some(format!("{} ({})", label, desc.unwrap()))
            } else {
                None
                })
            .collect::<Vec<_>>();
        assert_eq!(org_label.len() + desc_label.len() + aliases.len() + alias_descs.len(), labels.len());
        assert!(
            org_label.len() + desc_label.len() <= 1,
            "expected either an original label or a label + descprition, but got {org_label:#?} and {desc_label:#?}"
        );
        let info = ent_infos.get(&ent).unwrap();
        let label = if let Some(&label) = org_label.first() {
            label.to_string()
        } else if let Some(label) = desc_label.first() {
            label.clone()
        } else if !aliases.is_empty() || info.desc.is_empty() {
            info.label.to_string()
        } else {
            format!("{} ({})", info.label, info.desc)
        };
        aliases.sort_by_key(|&alias| {
            distance(&label, alias, true, false, false, false) as usize
        });
        alias_descs.sort_by_key(|alias| {
            distance(&label, alias, true, false, false, false) as usize
        });
        writeln!(
            output.lock().unwrap(),
            "{}\t{}\t{}",
            kg.format_entity(ent),
            if let Some(redirs) = info.redirects {
                redirs.iter().map(|r| kg.format_entity(r)).join(";")
            } else {
                "".to_string()
            },
            org_label
                .into_iter()
                .chain(desc_label.iter().map(|s| s.as_str()))
                .chain(aliases)
                .chain(alias_descs.iter().map(|s| s.as_str()))
                .join("\t")
        )
    })?;
    pbar.finish_and_clear();

    Ok(())
}
