use std::{
    collections::{HashMap, HashSet},
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
    env_logger::init();
    let args = Args::parse();

    let num_lines = lines(&args.file)?.count();
    let mut lines = lines(&args.file)?;

    let header = lines.next().expect("file should have at least 1 line")?;
    assert_eq!(header.split_terminator('\t').collect::<Vec<_>>().len(), 4);

    let prop_pattern = Regex::new(r"http://www.wikidata.org/prop/direct(-normalized)?/(P\d+)")?;
    let inv_prop_pattern = Regex::new(r"http://www.wikidata.org/entity/(P\d+)")?;
    let label_pattern = Regex::new("^\"(.*)\"@en$")?;

    let mut props = HashMap::new();

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
            .split_terminator(";")
            .map(|s| s.trim().to_string())
            .collect::<Vec<_>>();
        if args.inverse_output.is_some() && splits.len() == 4 {
            splits[3]
                .split_terminator(";")
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
                    let _ = inverse_props.insert(prop.clone(), (prop_num, inv));
                });
        }
        props
            .entry(label.clone())
            .or_insert_with(HashSet::new)
            .insert((prop.clone(), prop_num));
        if !args.no_aliases {
            for alias in aliases {
                props
                    .entry(alias.clone())
                    .or_insert_with(HashSet::new)
                    .insert((prop.clone(), prop_num));
            }
        }
    }
    pbar.finish_and_clear();
    let total_props = props.iter().map(|(_, ents)| ents.len()).sum::<usize>();
    let unique_props = props.len();

    println!("Wikidata properties");
    println!("###################");
    println!("lines:     {}", num_lines.saturating_sub(1));
    println!("total:     {total_props}");
    println!("unique:    {unique_props}");
    println!(
        "duplicate: {} ({:.2}%)",
        total_props.saturating_sub(unique_props),
        (total_props.saturating_sub(unique_props) as f64 / total_props as f64) * 100.0
    );

    let props: HashMap<_, _> = if args.keep_most_common_non_unique {
        props
            .into_iter()
            .map(|(label, props)| {
                if props.len() > 1 {
                    let top = props.into_iter().min_by_key(|(_, count)| *count).unwrap();
                    (label, top.0)
                } else {
                    (label, props.into_iter().next().unwrap().0)
                }
            })
            .collect()
    } else {
        props
            .into_iter()
            .filter_map(|(label, props)| {
                if props.len() == 1 {
                    Some((label, props.into_iter().next().unwrap().0))
                } else {
                    None
                }
            })
            .collect()
    };

    let mut output = BufWriter::new(fs::File::create(args.output)?);
    for (label, prop) in props {
        if label.is_empty() || prop.is_empty() {
            continue;
        }
        writeln!(output, "{}\t{}", label, prop)?;
    }

    if args.inverse_output.is_some() {
        let inverse_props: Vec<_> = inverse_props
            .into_iter()
            .sorted_by(|(_, a), (_, b)| a.0.cmp(&b.0))
            .collect();
        let num_inverse = inverse_props.len();
        let mut inverse_output = BufWriter::new(fs::File::create(args.inverse_output.unwrap())?);
        for (prop, (_, inv)) in inverse_props.into_iter() {
            if prop.is_empty() || inv.is_empty() {
                continue;
            }
            writeln!(inverse_output, "{}\t{}", prop, inv)?;
        }
        println!("inverse:   {num_inverse}");
    }

    Ok(())
}
