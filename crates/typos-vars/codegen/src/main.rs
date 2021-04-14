use std::collections::BTreeMap;
use std::collections::HashSet;
use std::io::Write;

use structopt::StructOpt;

static CATEGORIES: [varcon::Category; 4] = [
    varcon::Category::American,
    varcon::Category::BritishIse,
    // For now, only want to support one form of British, so going with -ise as it seems more
    // popular.
    varcon::Category::Canadian,
    varcon::Category::Australian,
    // Other basically means all
];

fn generate_variations<W: std::io::Write>(file: &mut W) {
    let entries = entries();

    writeln!(
        file,
        "// This file is code-genned by {}",
        env!("CARGO_PKG_NAME")
    )
    .unwrap();
    writeln!(file, "#![allow(clippy::unreadable_literal)]",).unwrap();
    writeln!(file).unwrap();

    writeln!(file, "use unicase::UniCase;").unwrap();
    writeln!(file).unwrap();

    writeln!(file, "pub type Variants = &'static [&'static str];",).unwrap();
    writeln!(
        file,
        "pub type VariantsMap = [Variants; {}];",
        CATEGORIES.len()
    )
    .unwrap();
    writeln!(file).unwrap();

    writeln!(file, "pub fn all_categories() -> crate::CategorySet {{",).unwrap();
    writeln!(
        file,
        "    {}",
        itertools::join(
            CATEGORIES
                .iter()
                .map(|c| format!("crate::Category::{:?}", c)),
            " | "
        )
    )
    .unwrap();
    writeln!(file, "}}",).unwrap();
    writeln!(file).unwrap();

    writeln!(
        file,
        "pub fn corrections(category: crate::Category, options: VariantsMap) -> &'static [&'static str] {{",
    )
    .unwrap();
    writeln!(file, "  match category {{").unwrap();
    for (index, category) in CATEGORIES.iter().enumerate() {
        writeln!(
            file,
            "    crate::Category::{:?} => options[{}],",
            category, index
        )
        .unwrap();
    }
    writeln!(
        file,
        "    crate::Category::BritishIze | crate::Category::Other => unreachable!(\"{{:?}} is unused\", category),",
    )
    .unwrap();
    writeln!(file, "  }}").unwrap();
    writeln!(file, "}}").unwrap();
    writeln!(file).unwrap();

    let mut smallest = usize::MAX;
    let mut largest = usize::MIN;

    writeln!(
        file,
        "pub static VARS_DICTIONARY: phf::Map<unicase::UniCase<&'static str>, &'static [(u8, &VariantsMap)]> = "
    )
    .unwrap();
    let entry_sets = entry_sets(entries.iter());
    let mut referenced_symbols: HashSet<&str> = HashSet::new();
    let mut builder = phf_codegen::Map::new();
    for (word, data) in entry_sets.iter() {
        if is_always_valid(data) {
            // No need to convert from current form to target form
            continue;
        }
        referenced_symbols.extend(data.iter().map(|(s, _)| s));
        let value = generate_link(&data);
        builder.entry(unicase::UniCase::new(word), &value);
        smallest = std::cmp::min(smallest, word.len());
        largest = std::cmp::max(largest, word.len());
    }
    let codegenned = builder.build();
    writeln!(file, "{}", codegenned).unwrap();
    writeln!(file, ";").unwrap();

    writeln!(file).unwrap();
    writeln!(file, "pub const WORD_MIN: usize = {};", smallest).unwrap();
    writeln!(file, "pub const WORD_MAX: usize = {};", largest).unwrap();

    for (symbol, entry) in entries.iter() {
        if !referenced_symbols.contains(symbol.as_str()) {
            continue;
        }
        generate_entry(file, symbol, entry);
    }
}

fn generate_entry(file: &mut impl std::io::Write, symbol: &str, entry: &varcon_core::Entry) {
    writeln!(file, "pub(crate) static {}: VariantsMap = [", symbol).unwrap();
    for category in &CATEGORIES {
        let corrections = collect_correct(entry, *category);
        let mut corrections: Vec<_> = corrections.iter().collect();
        corrections.sort_unstable();
        writeln!(file, "  &[").unwrap();
        for correction in &corrections {
            writeln!(file, "    {:?},", correction).unwrap();
        }
        writeln!(file, "  ],").unwrap();
    }
    writeln!(file, "];").unwrap();
    writeln!(file).unwrap();
}

fn generate_link(data: &[(&str, varcon::CategorySet)]) -> String {
    let mut output = Vec::new();

    write!(output, "&[").unwrap();
    for (symbol, set) in data.iter() {
        write!(output, "(0b{:05b}, &{}), ", set.bits(), symbol).unwrap();
    }
    write!(output, "]").unwrap();

    String::from_utf8(output).unwrap()
}

fn is_always_valid(data: &[(&str, varcon::CategorySet)]) -> bool {
    let valid_categories = valid_categories();
    for (_symbol, set) in data.iter() {
        if *set == valid_categories {
            return true;
        }
    }
    false
}

fn entries() -> BTreeMap<String, varcon_core::Entry> {
    varcon::VARCON
        .iter()
        .flat_map(|c| c.entries.iter())
        .filter(|e| {
            e.variants
                .iter()
                .all(|v| typos::tokens::Word::new(&v.word, 0).is_ok())
        })
        .map(|e| {
            let mut e = e.into_owned();
            for variant in e.variants.iter_mut() {
                variant.word.make_ascii_lowercase();
            }
            (entry_symbol(&e), e)
        })
        .collect()
}

fn entry_symbol(entry: &varcon_core::Entry) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::hash::Hash::hash(entry, &mut hasher);
    let hash = std::hash::Hasher::finish(&hasher);
    format!(
        "ENTRY_{}_{}",
        entry.variants[0].word.to_ascii_uppercase(),
        hash
    )
}

fn entry_sets<'e>(
    entries: impl Iterator<Item = (&'e String, &'e varcon_core::Entry)>,
) -> BTreeMap<&'e str, Vec<(&'e str, varcon::CategorySet)>> {
    let mut sets = BTreeMap::new();
    for (symbol, entry) in entries {
        for (word, set) in entry_set(entry).iter() {
            let v = sets.entry(*word).or_insert_with(Vec::new);
            v.push((symbol.as_str(), *set));
        }
    }
    sets
}

fn entry_set(entry: &varcon_core::Entry) -> BTreeMap<&str, varcon::CategorySet> {
    let mut sets = BTreeMap::new();
    let valid_categories = valid_categories();
    for variant in entry.variants.iter() {
        let set = sets
            .entry(variant.word.as_str())
            .or_insert_with(varcon::CategorySet::empty);
        for t in variant.types.iter() {
            match t.category {
                varcon::Category::Other => *set |= valid_categories,
                varcon::Category::BritishIze => (),
                _ => set.insert(t.category),
            }
        }
    }
    sets
}

fn valid_categories() -> varcon::CategorySet {
    let mut c = varcon::CategorySet::empty();
    for cat in CATEGORIES.iter() {
        c.insert(*cat);
    }
    c
}

fn collect_correct(entry: &varcon_core::Entry, category: varcon::Category) -> HashSet<&str> {
    // If there is ambiguity, collect all potential options.
    let mut primary = HashSet::new();
    let mut backup = HashSet::new();
    for variant in entry.variants.iter().filter(|v| !ignore_variant(v)) {
        for t in variant
            .types
            .iter()
            .filter(|t| t.category == category || t.category == varcon::Category::Other)
        {
            let tag = t.tag.unwrap_or(varcon::Tag::Eq);
            if tag == varcon::Tag::Eq {
                primary.insert(variant.word.as_str());
            }
            if tag != varcon::Tag::Improper {
                backup.insert(variant.word.as_str());
            }
        }
    }

    if primary.len() == 1 {
        primary
    } else {
        backup
    }
}

fn ignore_variant(variant: &varcon_core::Variant) -> bool {
    if variant.word == "anesthetisation"
        && variant.types.len() == 1
        && variant.types[0].category == varcon::Category::Australian
        && (variant.types[0].tag == Some(varcon::Tag::Variant)
            || variant.types[0].tag == Some(varcon::Tag::Seldom))
    {
        return true;
    }

    false
}

// dict needs
// all words, with bitfags, pointing to list of entry names
//
// varcon needs
// all entries by name

#[derive(Debug, StructOpt)]
#[structopt(rename_all = "kebab-case")]
struct Options {
    #[structopt(flatten)]
    codegen: codegenrs::CodeGenArgs,
    #[structopt(flatten)]
    rustmft: codegenrs::RustfmtArgs,

    #[structopt(flatten)]
    pub(crate) verbose: clap_verbosity_flag::Verbosity,
}

fn init_logging(level: Option<log::Level>) {
    if let Some(level) = level {
        let mut builder = env_logger::Builder::new();

        builder.filter(None, level.to_level_filter());

        if level == log::LevelFilter::Trace {
            builder.format_timestamp_secs();
        } else {
            builder.format(|f, record| {
                writeln!(
                    f,
                    "[{}] {}",
                    record.level().to_string().to_lowercase(),
                    record.args()
                )
            });
        }

        builder.init();
    }
}

fn run() -> Result<i32, Box<dyn std::error::Error>> {
    let mut options = Options::from_args();
    options.verbose.set_default(Some(log::Level::Info));
    init_logging(options.verbose.log_level());

    let mut content = vec![];
    generate_variations(&mut content);

    let content = String::from_utf8(content)?;
    let content = options.rustmft.reformat(&content)?;
    options.codegen.write_str(&content)?;

    Ok(0)
}

fn main() {
    let code = run().unwrap();
    std::process::exit(code);
}