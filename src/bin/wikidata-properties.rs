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
}

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let args = Args::parse();

    let num_lines = lines(&args.file)?.count();
    let mut lines = lines(&args.file)?;

    let header = lines.next().expect("file should have at least 1 line")?;
    assert_eq!(header.split_terminator('\t').collect::<Vec<_>>().len(), 4);

    let prop_pattern = Regex::new(r"http://www.wikidata.org/prop/direct(-normalized)?/(P\d+)")?;
    let label_pattern = Regex::new("^\"(.*)\"@en$")?;

    let mut props = HashMap::new();

    let pbar = progress_bar("processing wikidata properties", num_lines as u64, false);
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
        let val = props.insert(label, (prop_num, prop, aliases));
        assert!(val.is_none(), "labels must be unique");
        // let inverse_props = splits[2].parse::<usize>()?;
    }
    pbar.finish_and_clear();
    log::info!("found {} properties", props.len());

    // get also labels as set
    let labels: HashSet<_> = props.iter().map(|(label, _)| label.clone()).collect();
    // get for each alias how often it occurs
    let alias_counts: HashMap<_, _> = props
        .iter()
        .flat_map(|(_, (_, _, aliases))| aliases.clone())
        .fold(HashMap::new(), |mut map, alias| {
            *map.entry(alias).or_insert(0) += 1;
            map
        });
    // filter out aliases that are also labels and aliases that occurr more than once
    props.iter_mut().for_each(|(_, (_, _, aliases))| {
        aliases.retain(|alias| {
            !labels.contains(alias) && alias_counts.get(alias).copied().unwrap_or(0) == 1
        });
    });

    // sort and flatten
    let props: Vec<_> = props
        .into_iter()
        .sorted_by(|(_, a), (_, b)| a.0.cmp(&b.0))
        .flat_map(|(label, (_, prop, aliases))| {
            let mut props = vec![(label, prop.clone())];
            for alias in aliases {
                props.push((alias, prop.clone()));
            }
            props.into_iter().sorted_by(|(a, _), (b, _)| a.cmp(&b))
        })
        .collect();

    // assert uniqueness of labels
    assert!(props.iter().map(|(label, _)| label).unique().count() == props.len());

    let mut output = BufWriter::new(fs::File::create(args.output)?);
    for (label, prop) in props.into_iter() {
        writeln!(output, "{}\t{}", label, prop)?;
    }

    Ok(())
}
