use std::{
    collections::{hash_map::Entry, HashMap},
    fs::{self, create_dir_all, File},
    io::{BufWriter, Write},
    path::PathBuf,
};

use clap::Parser;
use itertools::Itertools;
use sparql_data_preparation::{
    line_iter, progress_bar, wikidata_qualifiers, KnowledgeGraph, KnowledgeGraphProcessor, Prop,
    PropInfo,
};

#[derive(Parser, Debug)]
struct Args {
    #[clap(short, long)]
    file: PathBuf,

    #[clap(short, long)]
    output: PathBuf,

    #[clap(short, long)]
    knowledge_base: String,

    #[clap(short, long)]
    inverse_output: Option<PathBuf>,

    #[clap(short, long)]
    no_aliases: bool,

    #[clap(short, long)]
    progress: bool,

    #[clap(short, long)]
    short_properties: bool,

    #[clap(short, long)]
    include_wikidata_qualifiers: bool,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let kg = KnowledgeGraph::try_from(args.knowledge_base.as_str())?;
    let kg = KnowledgeGraphProcessor::new(kg)?;

    let num_lines = line_iter(&args.file)?.count();
    let mut lines = line_iter(&args.file)?;

    let header = lines.next().expect("file should have at least 1 line")?;
    assert_eq!(header.split_terminator('\t').collect::<Vec<_>>().len(), 5);

    let mut label_to_prop = HashMap::new();
    let mut prop_infos = HashMap::new();

    let pbar = progress_bar(
        "processing wikidata properties",
        num_lines as u64,
        !args.progress,
    );
    let lines: Vec<_> = lines.collect::<anyhow::Result<_>>()?;
    for line in &lines {
        pbar.inc(1);
        let (prop, info) = kg.parse_property(line)?;

        match label_to_prop.entry(info.label.clone()) {
            Entry::Occupied(mut e) => {
                let existing_prop: &Prop = e.get();
                let existing_info: &PropInfo = prop_infos.get(existing_prop.as_str()).unwrap();
                if info.count > existing_info.count {
                    e.insert(prop.clone());
                }
            }
            Entry::Vacant(e) => {
                e.insert(prop.clone());
            }
        };
        prop_infos.insert(prop.as_str(), info);
    }
    pbar.finish_and_clear();

    let num_label_unique = label_to_prop.len();

    if !args.no_aliases {
        let alias_counts = prop_infos.values().flat_map(|info| &info.aliases).fold(
            HashMap::new(),
            |mut map, &alias| {
                *map.entry(alias).or_insert(0) += 1;
                map
            },
        );
        for (prop, info) in &prop_infos {
            for alias in &info.aliases {
                if alias_counts[alias] != 1 {
                    continue;
                }
                if let Entry::Vacant(entry) = label_to_prop.entry(alias.to_string()) {
                    entry.insert(Prop::Alias(prop));
                }
            }
        }
    }

    println!("{} properties", args.knowledge_base);
    println!("###################");
    println!("lines:           {}", num_lines.saturating_sub(1));
    println!("unique by label: {num_label_unique}");
    println!(
        "unique aliases:  {}",
        label_to_prop.len().saturating_sub(num_label_unique)
    );
    println!("total unique:    {}", label_to_prop.len());

    create_dir_all(&args.output)?;

    let mut output = BufWriter::new(File::create(args.output.join("index.tsv"))?);
    let mut output_dict = HashMap::new();
    for (label, prop) in &label_to_prop {
        output_dict
            .entry(prop.as_str())
            .or_insert_with(Vec::new)
            .push(match prop {
                Prop::Label(_) => Prop::Label(label),
                Prop::Alias(_) => Prop::Alias(label),
            });
    }
    for (prop, labels) in output_dict.iter_mut() {
        labels.sort();

        writeln!(
            output,
            "{}\t{}",
            kg.format_property(prop, args.short_properties, None)?,
            labels.iter().map(|p| p.as_str()).join("\t")
        )?;
        if !args.include_wikidata_qualifiers {
            continue;
        }
        labels
            .iter()
            .flat_map(|l| wikidata_qualifiers(l.as_str()))
            .fold(HashMap::<_, Vec<_>>::new(), |mut map, (lbl, pfx)| {
                map.entry(pfx).or_default().push(lbl);
                map
            })
            .into_iter()
            .try_for_each(|(pfx, lbls)| -> anyhow::Result<()> {
                Ok(writeln!(
                    output,
                    "{}\t{}",
                    kg.format_property(prop, args.short_properties, Some(&pfx))?,
                    lbls.join("\t")
                )?)
            })?;
    }

    let mut prefix_output_file = BufWriter::new(File::create(args.output.join("prefixes.tsv"))?);
    for (short, long) in kg.property_prefixes() {
        writeln!(prefix_output_file, "{short}\t{long}")?;
    }

    if args.inverse_output.is_some() {
        let mut inverse_output = BufWriter::new(fs::File::create(args.inverse_output.unwrap())?);
        let mut num_inverse = 0;
        for prop in output_dict.keys() {
            let info = prop_infos.get(prop).unwrap();
            for inv in &info.inverses {
                writeln!(
                    inverse_output,
                    "{}\t{}",
                    kg.format_property(prop, args.short_properties, None)?,
                    kg.format_property(inv, args.short_properties, None)?,
                )?;
            }
            num_inverse += info.inverses.len();
        }
        println!();
        println!("Wikidata inverse properties");
        println!("###########################");
        println!("inverse: {num_inverse}");
    }

    Ok(())
}
