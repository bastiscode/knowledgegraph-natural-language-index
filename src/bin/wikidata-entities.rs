use std::{
    collections::{hash_map::Entry, HashMap, HashSet},
    fs,
    io::{BufWriter, Write},
    path::PathBuf,
};

use clap::Parser;
use itertools::Itertools;
use regex::Regex;
use sparql_data_preparation::{lines, progress_bar};

#[derive(Parser, Debug)]
struct Args {
    #[clap(short, long)]
    file: PathBuf,

    #[clap(short, long)]
    output: PathBuf,

    #[clap(short, long)]
    progress: bool,

    #[clap(short, long)]
    keep_most_common_non_unique: bool,

    #[clap(short, long)]
    check_for_popular_aliases: bool,
}

struct EntityInfo {
    desc: String,
    aliases: Vec<String>,
    count: usize,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let num_lines = lines(&args.file)?.count();
    let mut lines = lines(&args.file)?;

    let header = lines.next().expect("file should have at least 1 line")?;
    assert_eq!(header.split_terminator('\t').collect::<Vec<_>>().len(), 5);

    let ent_pattern = Regex::new(r"http://www.wikidata.org/entity/(Q\d+)")?;
    let text_pattern = Regex::new("^\"(.*)\"@en$")?;

    let mut ent_infos = HashMap::new();
    let mut label_to_ents = HashMap::new();
    let mut aliases_to_ents = HashMap::new();

    let pbar = progress_bar(
        "processing wikidata entities",
        num_lines as u64,
        !args.progress,
    );
    for line in lines {
        pbar.inc(1);
        let line = line?;
        let splits: Vec<_> = line.split_terminator('\t').collect();
        assert!(splits.len() <= 5);

        let ent = if let Some(ent) = ent_pattern.captures(splits[0].trim()) {
            ent.get(1).unwrap().as_str().to_string()
        } else {
            continue;
        };

        let label = if let Some(ent_label) = text_pattern.captures(splits[1]) {
            ent_label.get(1).unwrap().as_str().trim().to_string()
        } else {
            continue;
        };
        if label.is_empty() {
            continue;
        }

        let desc = if let Some(ent_desc) = text_pattern.captures(splits[2]) {
            ent_desc.get(1).unwrap().as_str().trim().to_string()
        } else {
            "".to_string()
        };
        let count = splits[3].parse::<usize>()?;

        let aliases: Vec<_> = if splits.len() > 4 {
            splits[4]
                .split_terminator(';')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .sorted()
                .collect()
        } else {
            vec![]
        };

        label_to_ents
            .entry(label.clone())
            .or_insert_with(Vec::new)
            .push(ent.clone());

        if args.check_for_popular_aliases {
            for alias in &aliases {
                aliases_to_ents
                    .entry(alias.clone())
                    .or_insert_with(Vec::new)
                    .push(ent.clone());
            }
        }

        let info = EntityInfo {
            desc,
            aliases,
            count,
        };
        let existing = ent_infos.insert(ent, info);
        assert!(existing.is_none(), "entities should be unique");
    }
    pbar.finish_and_clear();

    let num_ents = ent_infos.len();

    let check_for_more_popular_alias = |label: &str, ent: &str| {
        let Some(alias_ents) = aliases_to_ents.get(label) else {
            return None;
        };
        let info = ent_infos.get(ent).unwrap();
        let Some((alias_ent, alias_count)) = alias_ents
            .iter()
            .filter_map(|alias_ent| {
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
    let mut label_to_ent = HashMap::new();
    assert!(label_to_ents.values().map(|ents| ents.len()).sum::<usize>() == num_ents);
    let mut label_desc_to_ents = HashMap::new();
    for (label, mut entities) in label_to_ents {
        assert!(entities.len() > 0);
        if entities.len() <= 1 {
            let alias_ent = check_for_more_popular_alias(&label, &entities[0]);
            if !args.check_for_popular_aliases || alias_ent.is_none() {
                label_to_ent.insert(label, entities.into_iter().next().unwrap());
                continue;
            }
        } else if args.keep_most_common_non_unique {
            // if we have multiple entities with the same label, we keep the most common one
            // as the one being identified by just the label
            entities.sort_by_key(|ent| ent_infos.get(ent).unwrap().count);
            // keep the most popular one only if its label is not an alias
            // of a more popular entity
            let alias_ent = check_for_more_popular_alias(&label, entities.last().unwrap());
            if !args.check_for_popular_aliases || alias_ent.is_none() {
                label_to_ent.insert(label.clone(), entities.pop().unwrap());
            }
        }
        // if the label alone is not unique, we add the description to it and try again
        for ent in entities {
            let desc = &ent_infos.get(&ent).unwrap().desc;
            if desc.is_empty() {
                continue;
            }
            let label_desc = format!("{label} ({desc})");
            if label_to_ent.contains_key(&label_desc) {
                continue;
            }
            label_desc_to_ents
                .entry(label_desc)
                .or_insert_with(Vec::new)
                .push(ent);
        }
    }
    let num_label_unique = label_to_ent.len();
    assert!(label_to_ent.iter().unique_by(|&(_, ent)| ent).count() == label_to_ent.len());

    let mut ents_left = HashSet::new();
    for (label, mut entities) in label_desc_to_ents {
        if entities.len() <= 1 {
            label_to_ent.insert(label, entities.into_iter().next().unwrap());
            continue;
        } else if args.keep_most_common_non_unique {
            // same as above
            entities.sort_by_key(|ent| ent_infos.get(ent).unwrap().count);
            label_to_ent.insert(label, entities.pop().unwrap());
        }
        // if the label and description are not unique
        // record the entities with entry yet to be preferred when adding aliases
        ents_left.extend(entities);
    }
    let num_label_desc_unique = label_to_ent.len();
    assert!(label_to_ent.iter().unique_by(|&(_, ent)| ent).count() == label_to_ent.len());

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
    println!("entities left before:     {}", ents_left.len(),);

    // now we have all unique entities
    // go over aliases to make sure one entitiy can be found by multiple names
    let mut total_aliases = 0;
    ent_infos
        .into_iter()
        .sorted_by_key(|(ent, info)| (ents_left.contains(ent), info.count))
        .rev()
        .for_each(|(ent, info)| {
            total_aliases += info.aliases.len();
            for alias in info.aliases {
                if let Entry::Vacant(entry) = label_to_ent.entry(alias.clone()) {
                    entry.insert(ent.clone());
                    ents_left.remove(&ent);
                    continue;
                } else if info.desc.is_empty() {
                    continue;
                }
                let alias_desc = format!("{} ({})", alias, info.desc);
                if let Entry::Vacant(entry) = label_to_ent.entry(alias_desc) {
                    entry.insert(ent.clone());
                    ents_left.remove(&ent);
                }
            }
        });

    println!(
        "added unique aliases:     {} ({:.2}% of all aliases)",
        label_to_ent.len() - num_label_desc_unique,
        100.0 * (label_to_ent.len() - num_label_desc_unique) as f32 / total_aliases as f32
    );
    println!("entities left after:      {}", ents_left.len(),);
    println!("final index size:         {}", label_to_ent.len());
    println!(
        "final index coverage:     {:.2}%",
        100.0 * label_to_ent.iter().unique_by(|&(_, ent)| ent).count() as f32 / num_ents as f32
    );

    let mut output = BufWriter::new(fs::File::create(args.output)?);
    for (label, ent) in label_to_ent {
        writeln!(output, "{}\t{}", label, ent)?;
    }

    Ok(())
}
