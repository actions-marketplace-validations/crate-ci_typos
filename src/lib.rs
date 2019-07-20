#[macro_use]
extern crate serde_derive;

mod dict;
mod dict_codegen;

pub mod report;
pub mod tokens;

pub use crate::dict::*;

use std::fs::File;
use std::io::Read;

use bstr::ByteSlice;

pub fn process_file(
    path: &std::path::Path,
    dictionary: &Dictionary,
    check_filenames: bool,
    check_files: bool,
    ignore_hex: bool,
    binary: bool,
    report: report::Report,
) -> Result<(), failure::Error> {
    if check_filenames {
        for part in path.components().filter_map(|c| c.as_os_str().to_str()) {
            for ident in tokens::Identifier::parse(part) {
                if !ignore_hex && is_hex(ident.token()) {
                    continue;
                }
                if let Some(correction) = dictionary.correct_ident(ident) {
                    let msg = report::FilenameCorrection {
                        path,
                        typo: ident.token(),
                        correction,
                        non_exhaustive: (),
                    };
                    report(msg.into());
                }
                for word in ident.split() {
                    if let Some(correction) = dictionary.correct_word(word) {
                        let msg = report::FilenameCorrection {
                            path,
                            typo: word.token(),
                            correction,
                            non_exhaustive: (),
                        };
                        report(msg.into());
                    }
                }
            }
        }
    }

    if check_files {
        let mut buffer = Vec::new();
        File::open(path)?.read_to_end(&mut buffer)?;
        if !binary && buffer.find_byte(b'\0').is_some() {
            let msg = report::BinaryFile {
                path,
                non_exhaustive: (),
            };
            report(msg.into());
            return Ok(());
        }

        for (line_idx, line) in buffer.lines().enumerate() {
            let line_num = line_idx + 1;
            for ident in tokens::Identifier::parse_bytes(line) {
                if !ignore_hex && is_hex(ident.token()) {
                    continue;
                }
                if let Some(correction) = dictionary.correct_ident(ident) {
                    let col_num = ident.offset();
                    let msg = report::Correction {
                        path,
                        line,
                        line_num,
                        col_num,
                        typo: ident.token(),
                        correction,
                        non_exhaustive: (),
                    };
                    report(msg.into());
                }
                for word in ident.split() {
                    if let Some(correction) = dictionary.correct_word(word) {
                        let col_num = word.offset();
                        let msg = report::Correction {
                            path,
                            line,
                            line_num,
                            col_num,
                            typo: word.token(),
                            correction,
                            non_exhaustive: (),
                        };
                        report(msg.into());
                    }
                }
            }
        }
    }

    Ok(())
}

fn is_hex(ident: &str) -> bool {
    lazy_static::lazy_static! {
        // `_`: number literal separator in Rust and other languages
        // `'`: number literal separator in C++
        static ref HEX: regex::Regex = regex::Regex::new(r#"^0[xX][0-9a-fA-F_']+$"#).unwrap();
    }
    HEX.is_match(ident)
}
