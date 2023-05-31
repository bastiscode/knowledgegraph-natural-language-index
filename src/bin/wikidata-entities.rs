use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{BufWriter, Write},
    path::PathBuf,
};

use clap::Parser;
use regex::Regex;
use sparql_data_preparation::{lines, progress_bar};

#[derive(Parser, Debug)]
struct Args {
    #[clap(short, long)]
    file: PathBuf,

    #[clap(short, long)]
    output: PathBuf,

    #[clap(short, long)]
    no_desc: bool,

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
    assert_eq!(header.split_terminator('\t').collect::<Vec<_>>().len(), 5);

    let ent_pattern = Regex::new(r"http://www.wikidata.org/entity/(Q\d+)")?;
    let text_pattern = Regex::new("^\"(.*)\"@en$")?;

    let mut ents = HashMap::with_capacity(8 * num_lines);

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

        let ent_label = if let Some(ent_label) = text_pattern.captures(splits[1].trim()) {
            ent_label.get(1).unwrap().as_str().to_string()
        } else {
            continue;
        };

        let ent_desc = if let Some(ent_desc) = text_pattern.captures(splits[2].trim()) {
            ent_desc.get(1).unwrap().as_str().to_string()
        } else {
            "".to_string()
        };
        let count = splits[3].parse::<usize>()?;

        let aliases: Vec<_> = if splits.len() > 4 {
            splits[4]
                .split_terminator(";")
                .map(|s| s.trim().to_string())
                .collect()
        } else {
            vec![]
        };

        log::info!(
            "entity: {}, label: {}, desc: {}, count: {}, aliases: {:?}",
            ent,
            ent_label,
            ent_desc,
            count,
            aliases,
        );
        ents.entry((ent_label.clone(), String::new()))
            .or_insert_with(HashSet::new)
            .insert((ent.clone(), count));
        if !args.no_desc {
            ents.entry((ent_label.clone(), ent_desc.clone()))
                .or_insert_with(HashSet::new)
                .insert((ent.clone(), count));
        }
        if !args.no_aliases {
            for alias in aliases {
                ents.entry((alias.clone(), String::new()))
                    .or_insert_with(HashSet::new)
                    .insert((ent.clone(), count));
                if !args.no_desc {
                    ents.entry((alias.clone(), ent_desc.clone()))
                        .or_insert_with(HashSet::new)
                        .insert((ent.clone(), count));
                }
            }
        }
    }
    pbar.finish_and_clear();
    let total_ents = ents.iter().map(|(_, ents)| ents.len()).sum::<usize>();
    let unique_ents = ents.len();

    println!("Wikidata entities");
    println!("#################");
    println!("lines:     {}", num_lines.saturating_sub(1));
    println!("total:     {total_ents}");
    println!("unique:    {unique_ents}");
    println!(
        "duplicate: {} ({:.2}%)",
        total_ents.saturating_sub(unique_ents),
        (total_ents.saturating_sub(unique_ents) as f64 / total_ents as f64) * 100.0
    );

    let ents: HashMap<_, _> = if args.keep_most_common_non_unique {
        ents.into_iter()
            .map(|(label, ents)| {
                if ents.len() > 1 {
                    let top = ents.into_iter().max_by_key(|(_, count)| *count).unwrap();
                    (label, top.0)
                } else {
                    (label, ents.into_iter().next().unwrap().0)
                }
            })
            .collect()
    } else {
        ents.into_iter()
            .filter_map(|(label, ents)| {
                if ents.len() == 1 {
                    Some((label, ents.into_iter().next().unwrap().0))
                } else {
                    None
                }
            })
            .collect()
    };

    // now we have unique entities
    // filter out all description based entities if they
    // are already unique without description
    let unique_labels: HashSet<_> = ents.iter().map(|((label, _), _)| label).cloned().collect();
    let ents: HashMap<_, _> = ents
        .into_iter()
        .filter_map(|((label, desc), ent)| {
            if desc.is_empty() || unique_labels.contains(&label) {
                Some((label, ent))
            } else {
                Some((format!("{label} ({desc})"), ent))
            }
        })
        .collect();

    let mut output = BufWriter::new(fs::File::create(args.output)?);
    for (label, ent) in ents {
        if label.is_empty() || ent.is_empty() {
            continue;
        }
        writeln!(output, "{}\t{}", label, ent)?;
    }

    Ok(())
}
