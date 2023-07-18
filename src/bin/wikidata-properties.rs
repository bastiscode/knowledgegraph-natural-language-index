use std::{
    collections::{hash_map::Entry, HashMap, HashSet},
    fs,
    io::{BufWriter, Write},
    path::PathBuf,
};

use clap::Parser;
use itertools::Itertools;
use regex::Regex;
use sparql_data_preparation::{line_iter, progress_bar};
use text_correction_utils::edit::distance;

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
    full_ids: bool,

    #[clap(short, long)]
    include_qualifiers: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Prop {
    Label(String),
    Alias(String),
}

impl Prop {
    fn as_str(&self) -> &str {
        match self {
            Prop::Label(s) => s,
            Prop::Alias(s) => s,
        }
    }
}

fn qualifiers(label: &str) -> Vec<(String, String)> {
    vec![
        (format!("{label} (statement)"), "p".to_string()),
        (format!("{label} (qualifier)"), "pq".to_string()),
        (format!("{label} (value)"), "ps".to_string()),
    ]
}

fn main() -> anyhow::Result<()> {
    let mut args = Args::parse();
    if args.include_qualifiers {
        args.full_ids = true;
    }

    let num_lines = line_iter(&args.file)?.count();
    let mut lines = line_iter(&args.file)?;

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
                    inverse_props
                        .entry(prop.clone())
                        .or_insert_with(|| (prop_num, vec![]))
                        .1
                        .push((inv, inv_num));
                });
        }
        let existing = label_to_prop.insert(label.clone(), Prop::Label(prop.clone()));
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
        if props.len() != 1 {
            continue;
        }
        if let Entry::Vacant(entry) = label_to_prop.entry(alias) {
            entry.insert(Prop::Alias(props.into_iter().next().unwrap().0));
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

    let format_prop = |prop: &str, pfx: &str| {
        if args.full_ids {
            format!("{pfx}:{prop}")
        } else {
            prop.chars().skip(1).collect::<String>()
        }
    };

    let mut output = BufWriter::new(fs::File::create(args.output)?);
    let mut output_dict = HashMap::new();
    for (label, prop) in label_to_prop {
        output_dict
            .entry(prop.as_str().to_string())
            .or_insert_with(Vec::new)
            .push(match prop {
                Prop::Label(_) => Prop::Label(label),
                Prop::Alias(_) => Prop::Alias(label),
            });
    }
    for (prop, mut labels) in output_dict
        .into_iter()
        .sorted_by_key(|(prop, _)| prop.clone())
    {
        labels.sort_by_key(|label| matches!(label, Prop::Label(_)));
        let Some(label) = labels.pop() else {
            unreachable!();
        };
        assert!(matches!(label, Prop::Label(_)));
        labels.sort_by_key(|alias| {
            distance(label.as_str(), alias.as_str(), true, false, false, false) as usize
        });

        writeln!(
            output,
            "{}\t\t{}",
            format_prop(prop.as_str(), "wdt"),
            vec![&label]
                .into_iter()
                .chain(&labels)
                .map(|p| p.as_str())
                .join("\t")
        )?;
        if !args.include_qualifiers {
            continue;
        }
        vec![&label]
            .into_iter()
            .chain(&labels)
            .flat_map(|l| qualifiers(l.as_str()))
            .fold(HashMap::new(), |mut map, (lbl, pfx)| {
                map.entry(pfx).or_insert_with(Vec::new).push(lbl);
                map
            })
            .into_iter()
            .try_for_each(|(pfx, lbls)| {
                writeln!(
                    output,
                    "{}\t\t{}",
                    format_prop(prop.as_str(), &pfx),
                    lbls.join("\t")
                )
            })?;
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
            writeln!(
                inverse_output,
                "{}\t{}",
                format_prop(&prop, "wdt"),
                format_prop(&inv, "wdt")
            )?;
        }
        println!();
        println!("Wikidata inverse properties");
        println!("###########################");
        println!("inverse: {num_inverse}");
    }

    Ok(())
}
