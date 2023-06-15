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
    inverse_output: Option<PathBuf>,

    #[clap(short, long)]
    no_aliases: bool,

    #[clap(short, long)]
    progress: bool,

    #[clap(short, long)]
    keep_most_common_non_unique: bool,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let num_lines = lines(&args.file)?.count();
    let mut lines = lines(&args.file)?;

    let header = lines.next().expect("file should have at least 1 line")?;
    assert_eq!(header.split_terminator('\t').collect::<Vec<_>>().len(), 4);

    let prop_pattern = Regex::new(r"http://www.wikidata.org/prop/direct(-normalized)?/(P\d+)")?;
    let inv_prop_pattern = Regex::new(r"http://www.wikidata.org/entity/(P\d+)")?;
    let label_pattern = Regex::new("^\"(.*)\"@en$")?;

    let mut label_to_prop = HashMap::new();
    let mut alias_to_prop = HashMap::new();
    let mut inverse_props = HashMap::new();

    let pbar = progress_bar(
        "processing wikidata properties",
        num_lines as u64,
        !args.progress,
    );
    for line in lines {
        pbar.inc(1);
        let line = line?;
        let splits: Vec<_> = line.split_terminator('\t').collect();
        assert!(splits.len() >= 2 && splits.len() <= 4);

        let Some(prop) = prop_pattern.captures(splits[0]) else {
            continue;
        };

        let Some(label) = label_pattern.captures(splits[1]) else {
            continue;
        };

        let prop = prop.get(2).unwrap().as_str().to_string();
        let prop_num = prop.chars().skip(1).collect::<String>().parse::<usize>()?;
        let label = label.get(1).unwrap().as_str().to_string();
        let aliases = splits[2]
            .split_terminator(';')
            .map(|s| s.trim().to_string())
            .collect::<Vec<_>>();
        if args.inverse_output.is_some() && splits.len() == 4 {
            splits[3]
                .split_terminator(';')
                .map(|s| {
                    inv_prop_pattern
                        .captures(s.trim())
                        .unwrap()
                        .get(1)
                        .unwrap()
                        .as_str()
                        .to_string()
                })
                .for_each(|inv| {
                    let inv_num = inv
                        .chars()
                        .skip(1)
                        .collect::<String>()
                        .parse::<usize>()
                        .unwrap();
                    let _ = inverse_props
                        .entry(prop.clone())
                        .or_insert_with(|| (prop_num, vec![]))
                        .1
                        .push((inv, inv_num));
                });
        }
        let existing = label_to_prop.insert(label.clone(), prop.clone());
        assert!(existing.is_none(), "labels for properties should be unique");
        if !args.no_aliases {
            for alias in aliases {
                alias_to_prop
                    .entry(alias.clone())
                    .or_insert_with(HashSet::new)
                    .insert((prop.clone(), prop_num));
            }
        }
    }
    pbar.finish_and_clear();

    let num_label_unique = label_to_prop.len();

    for (alias, props) in alias_to_prop {
        if let Entry::Vacant(entry) = label_to_prop.entry(alias) {
            if props.len() <= 1 {
                entry.insert(props.into_iter().next().unwrap().0);
            } else {
                entry.insert(
                    props
                        .into_iter()
                        .sorted_by_key(|(_, prop_num)| *prop_num)
                        .next()
                        .unwrap()
                        .0,
                );
            }
        }
    }

    println!("Wikidata properties");
    println!("###################");
    println!("lines:           {}", num_lines.saturating_sub(1));
    println!("unique by label: {num_label_unique}");
    println!(
        "unique aliases:  {}",
        label_to_prop.len().saturating_sub(num_label_unique)
    );
    println!("total unique:    {}", label_to_prop.len());

    let mut output = BufWriter::new(fs::File::create(args.output)?);
    for (label, prop) in label_to_prop {
        if label.is_empty() || prop.is_empty() {
            continue;
        }
        writeln!(output, "{}\t{}", label, prop)?;
    }

    if args.inverse_output.is_some() {
        let inverse_props: Vec<_> = inverse_props
            .into_iter()
            .flat_map(|(prop, (prop_num, invs))| {
                invs.into_iter()
                    .map(move |(inv, inv_num)| (prop.clone(), prop_num, inv, inv_num))
            })
            .sorted_by_key(|&(_, prop_num, _, inv_num)| (prop_num, inv_num))
            .collect();
        let num_inverse = inverse_props.len();
        let mut inverse_output = BufWriter::new(fs::File::create(args.inverse_output.unwrap())?);
        for (prop, _, inv, _) in inverse_props.into_iter() {
            if prop.is_empty() || inv.is_empty() {
                continue;
            }
            writeln!(inverse_output, "{}\t{}", prop, inv)?;
        }
        println!();
        println!("Wikidata inverse properties");
        println!("###########################");
        println!("inverse: {num_inverse}");
    }

    Ok(())
}
