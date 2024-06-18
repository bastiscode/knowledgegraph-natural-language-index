use rayon::prelude::*;
use std::{
    cmp::Reverse,
    collections::{hash_map::Entry, HashMap, HashSet},
    fs::{create_dir_all, File},
    io::{BufWriter, Write},
    path::PathBuf,
    sync::{Arc, Mutex},
};

use clap::Parser;
use itertools::Itertools;
use sparql_data_preparation::{
    line_iter, progress_bar, Ent, KnowledgeGraph, KnowledgeGraphProcessor,
};

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
    ignore_types: bool,

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
            assert!(splits.len() == 2);
            let ent = if let Some(ent) = kg.ent_pattern.captures(splits[0].trim()) {
                ent.get(1).unwrap().as_str().to_string()
            } else {
                continue;
            };
            let redirs: Vec<_> = splits[1]
                .split_terminator("; ")
                .map(|s| {
                    kg.ent_pattern
                        .captures(s.trim())
                        .unwrap_or_else(|| {
                            panic!(
                                "could not find entity with pattern {} in {s}",
                                kg.ent_pattern
                            )
                        })
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

    let pbar = progress_bar(
        &format!("loading {} entities", &args.knowledge_base),
        u64::MAX,
        !args.progress,
    );
    let mut lines = pbar.wrap_iter(line_iter(&args.file)?);
    let header = lines.next().expect("file should have at least 1 line")?;
    let lines: Vec<_> = lines.collect::<anyhow::Result<_>>()?;
    assert_eq!(header.split_terminator('\t').collect::<Vec<_>>().len(), 6);
    pbar.finish_and_clear();
    let pbar = progress_bar(
        &format!("processing {} entities", &args.knowledge_base),
        lines.len() as u64,
        !args.progress,
    );
    for line in &lines {
        pbar.inc(1);
        let (ent, mut info) = kg.parse_entity(line, args.ignore_types)?;

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

    ent_infos.values().for_each(|info| {
        let mut types = info.types.lock().unwrap();
        types.sort_by_key(|&type_id| ent_infos.get(type_id).map(|info| info.count).unwrap_or(0));
        *types = types
            .iter()
            .filter_map(|type_id| ent_infos.get(type_id).map(|info| info.label))
            .collect();
    });

    let num_ents = ent_infos.len();

    // filter out aliases that are aliases for multiple entities
    aliases_to_ents.retain(|_, ents| ents.len() <= 1);

    let check_for_more_popular_alias = |label: &str, ent: &str| {
        let alias_ents = aliases_to_ents.get(label)?;
        let info = ent_infos.get(ent).unwrap();
        let (alias_ent, alias_count) = alias_ents
            .iter()
            .filter_map(|&alias_ent| {
                if alias_ent == ent {
                    return None;
                }
                Some((alias_ent, ent_infos.get(alias_ent).unwrap().count))
            })
            .max_by_key(|&(_, count)| count)?;
        if alias_count > info.count {
            Some(alias_ent.to_string())
        } else {
            None
        }
    };

    // initialize the final label to entity mapping
    let mut label_to_ent: HashMap<(&str, Option<&str>), _> = HashMap::new();
    assert!(label_to_ents.values().map(|ents| ents.len()).sum::<usize>() == num_ents);
    let mut label_info_to_ents = HashMap::new();
    let pbar = progress_bar(
        "adding unique labels",
        label_to_ents.len() as u64,
        !args.progress,
    );
    for (label, entities) in label_to_ents {
        pbar.inc(1);
        assert!(!entities.is_empty());
        if entities.len() <= 1 {
            let alias_ent = check_for_more_popular_alias(label, entities[0].as_str());
            if !args.check_for_popular_aliases || alias_ent.is_none() {
                let ent = entities.into_iter().next().unwrap();
                assert!(label_to_ent.insert((label, None), ent).is_none());
                continue;
            }
        }
        // if the label alone is not unique, we add the type or description to it and try again
        for ent in entities {
            let ent_info = ent_infos.get(ent.as_str()).unwrap();
            let info = ent_info.info();
            if info.is_empty() {
                continue;
            }
            let label_info = format!("{label} ({info})");
            if label_to_ent.contains_key(&(label_info.as_str(), None)) {
                continue;
            }
            label_info_to_ents
                .entry((label, info))
                .or_insert_with(Vec::new)
                .push((ent_info.count, ent));
        }
    }
    pbar.finish_and_clear();
    let num_label_unique = label_to_ent.len();
    // assert!(label_to_ent.iter().unique_by(|&(_, ent)| ent).count() == label_to_ent.len());

    let mut ents_left: HashSet<_> = HashSet::new();
    let pbar = progress_bar(
        "adding label-info pairs",
        label_info_to_ents.len() as u64,
        !args.progress,
    );
    for ((label, info), mut entities) in
        label_info_to_ents
            .into_iter()
            .sorted_by_key(|(key, entities)| {
                let max = entities.iter().map(|(c, _)| c).max().copied().unwrap_or(0);
                (Reverse(max), entities.len(), *key)
            })
    {
        pbar.inc(1);
        if entities.len() <= 1 {
            let ent = entities.iter_mut().next().unwrap().1.as_str();
            let alias_ent = check_for_more_popular_alias(label, ent);
            if label_to_ent.contains_key(&(label, None))
                || (args.check_for_popular_aliases && alias_ent.is_some())
            {
                assert!(label_to_ent
                    .insert((label, Some(info)), Ent::LabelInfo(ent))
                    .is_none());
            } else {
                assert!(label_to_ent
                    .insert((label, None), Ent::Label(ent))
                    .is_none());
            }
            continue;
        } else if args.keep_most_common_non_unique {
            entities.sort_by_key(|(c, _)| *c);

            let ent = entities.pop().unwrap().1.as_str();
            let alias_ent = check_for_more_popular_alias(label, ent);
            if label_to_ent.contains_key(&(label, None))
                || (args.check_for_popular_aliases && alias_ent.is_some())
            {
                assert!(label_to_ent
                    .insert((label, Some(info)), Ent::LabelInfo(ent))
                    .is_none());
            } else {
                assert!(label_to_ent
                    .insert((label, None), Ent::Label(ent))
                    .is_none());
            }
        }
        // if the label and type/description are not unique
        // record the entities with no entry for statistics
        ents_left.extend(entities.iter().map(|(_, e)| e.as_str()));
    }
    pbar.finish_and_clear();
    drop(aliases_to_ents);
    let num_label_info_unique = label_to_ent.len();
    // assert!(label_to_ent.iter().unique_by(|&(_, ent)| ent).count() == label_to_ent.len());

    println!("{} entities", args.knowledge_base);
    println!("#################");
    println!("entities:                 {}", num_ents);
    println!("unique by label:          {}", num_label_unique);
    println!(
        "label coverage:           {:.2}%",
        100.0 * num_label_unique as f32 / num_ents as f32
    );
    println!("unique by label and info: {}", num_label_info_unique);
    println!(
        "label and info coverage:  {:.2}%",
        100.0 * num_label_info_unique as f32 / num_ents as f32
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
        .sorted_by_key(|&(key, info)| (Reverse(info.count), key))
        .for_each(|(&ent, info)| {
            pbar.inc(1);
            total_aliases += info.aliases.len();
            for &alias in &info.aliases {
                if let Entry::Vacant(entry) = label_to_ent.entry((alias, None)) {
                    entry.insert(Ent::Alias(ent));
                    continue;
                } else if info.info().is_empty() {
                    continue;
                }
                if let Entry::Vacant(entry) = label_to_ent.entry((alias, Some(info.info()))) {
                    entry.insert(Ent::AliasInfo(ent));
                }
            }
        });
    pbar.finish_and_clear();

    println!(
        "added unique aliases:     {} ({:.2}% of all aliases)",
        label_to_ent.len() - num_label_info_unique,
        100.0 * (label_to_ent.len() - num_label_info_unique) as f32 / total_aliases as f32
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
            .push((label, matches!(ent, Ent::Alias(_) | Ent::AliasInfo(_))));
    }

    create_dir_all(&args.output)?;
    let output = Arc::new(Mutex::new(BufWriter::new(File::create(
        args.output.join("index.tsv"),
    )?)));
    let mut prefix_output_file = BufWriter::new(File::create(args.output.join("prefixes.tsv"))?);
    for (short, long) in kg.entity_prefixes() {
        writeln!(prefix_output_file, "{short}\t{long}")?;
    }

    let redirect_output = Arc::new(Mutex::new(BufWriter::new(File::create(
        args.output.join("redirects.tsv"),
    )?)));

    let pbar = progress_bar("creating outputs", output_dict.len() as u64, !args.progress);
    output_dict.into_par_iter().try_for_each(|(ent, labels)| {
        pbar.inc(1);
        let org_label: Vec<_> = labels
            .iter()
            .filter_map(|&(&(label, info), is_alias)| if info.is_none() && !is_alias {
                Some(label)
            } else {
                None
            })
            .collect();
        let info_label: Vec<_> = labels
            .iter()
            .filter_map(|&(&(label, info), is_alias)| if info.is_some() && !is_alias {
                Some(format!("{} ({})", label, info.unwrap()))
            } else {
                None
            })
            .collect();
        let aliases = labels
            .iter()
            .filter_map(|&(&(label, info), is_alias)| if info.is_none() && is_alias {Some(label)} else {None})
            .collect::<Vec<_>>();
        let alias_infos = labels
            .iter()
            .filter_map(|&(&(label, info), is_alias)| if info.is_some() && is_alias {
                Some(format!("{} ({})", label, info.unwrap()))
            } else {
                None
                })
            .collect::<Vec<_>>();
        assert_eq!(org_label.len() + info_label.len() + aliases.len() + alias_infos.len(), labels.len());
        assert!(
            org_label.len() + info_label.len() <= 1,
            "expected either an original label or a label + info, but got {org_label:#?} and {info_label:#?}"
        );
        let info = ent_infos.get(&ent).unwrap();
        if let Some(redirs) = info.redirects {
            writeln!(
                redirect_output.lock().unwrap(),
                "{}\t{}",
                kg.format_entity(ent),
                redirs.iter().map(|r| kg.format_entity(r)).join("\t")
            )?;
        }
        writeln!(
            output.lock().unwrap(),
            "{}\t{}",
            kg.format_entity(ent),
            org_label
                .into_iter()
                .chain(info_label.iter().map(|s| s.as_str()))
                .chain(aliases)
                .chain(alias_infos.iter().map(|s| s.as_str()))
                .join("\t")
        )
    })?;
    pbar.finish_and_clear();

    Ok(())
}
