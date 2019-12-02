//! Provides functionality to optimizing Pants dependencies.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::error::Error;
use std::fs;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::PathBuf;

use serde::de::DeserializeOwned;
use serde_json;

use crate::Command::{Undeclared, Unused};
use crate::{Config, UndeclaredSubCommand, UnusedSubCommand};

/// Perform Action corresponded to the Config.
pub fn perform(config: Config) {
    match config.cmd {
        Unused { cmd } => match cmd {
            UnusedSubCommand::Show => show_unused(config.report_file),
            UnusedSubCommand::Fix => fix_unused(config.report_file),
        },
        Undeclared { cmd } => match cmd {
            UndeclaredSubCommand::Show => show_undeclared(config.report_file),
            UndeclaredSubCommand::Fix => fix_undeclared(config.report_file),
        },
    }
}

/// Print report about all unused dependencies.
fn show_unused(report: PathBuf) {
    let unused = select(report, "unused");
    let modules = unused.len();
    let unused_amount: usize = unused.values().map(Vec::len).sum();
    println!(
        "{:#?}\n modules affected: {}, total dependencies unused: {}",
        &unused, modules, unused_amount
    );
}

/// Removes all unused dependencies from all corresponded BUILD files.
fn fix_unused(report: PathBuf) {
    let unused = select(report, "unused");
    for (module, deps) in unused {
        let removed = remove_deps(&module, deps)
            .unwrap_or_else(|_| panic!("Couldn't remove unused for module: {}", module));
        println!("{} removed: {}", module, removed)
    }
}

/// Removes from the Pants Address target name, leaves only directory.
fn address_to_folder(address: &str) -> String {
    if address.contains("3rdparty") {
        address.to_owned()
    } else {
        address.split(':').collect::<Vec<_>>()[0].to_string()
    }
}

/// Finds BUILD file and removes lines with unused dependencies, returns number of removed lines.
fn remove_deps(folder: &str, deps: Vec<String>) -> Result<usize, Box<dyn Error>> {
    let mut counter = 0;
    for entry in fs::read_dir(folder)? {
        let entry = entry?;
        if entry.file_name() == "BUILD" {
            // read and filter unused dependencies
            let cleaned = {
                let file = BufReader::new(File::open(entry.path())?);

                file.lines()
                    .filter_map(|line| {
                        let line = line
                            .unwrap_or_else(|_| panic!("Couldn't read line for {}/BUILD ", folder));

                        if deps.iter().any(|target| line.contains(target)) {
                            // if line contents unused dep remove it from result
                            counter += 1;
                            None
                        } else {
                            Some(line)
                        }
                    })
                    .collect::<Vec<String>>()
            };

            // write filtered dependencies back in BUILD file
            let mut file = BufWriter::new(File::create(entry.path())?);
            for line in cleaned {
                writeln!(file, "{}", line)?;
            }
            file.flush()?;
            break;
        }
    }
    Ok(counter)
}

fn show_undeclared(report: PathBuf) {
    let unused = select(report, "undeclared");
    let modules = unused.len();
    let unused_amount: usize = unused.values().map(Vec::len).sum();
    println!(
        "{:#?}\n modules affected: {}, total dependencies undeclared: {}",
        &unused, modules, unused_amount
    );
}

fn fix_undeclared(_report: PathBuf) {
    unimplemented!("Not implemented will appear in new releases")
}

/// Aggregates modules and their dependencies with specified type.
fn select(report: PathBuf, dependency_type: &str) -> BTreeMap<String, Vec<String>> {
    let json = read_report::<HashMap<String, Info>>(report).expect("Couldn't read as json");
    json.into_iter()
        .filter_map(|(module, info)| {
            if module.contains("3rdparty") {
                None
            } else {
                let unused_deps = info
                    .dependencies
                    .iter()
                    .filter_map(|dep| {
                        if dep.dependency_type == dependency_type {
                            Some(address_to_folder(&dep.target))
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>();

                if unused_deps.is_empty() {
                    None
                } else {
                    Some((address_to_folder(&module), unused_deps))
                }
            }
        })
        .collect()
}

#[derive(Deserialize, Debug)]
pub struct Summary {
    badness: isize,
    max_usage: f32,
    cost_transitive: isize,
    target: String,
}

#[derive(Deserialize, Debug)]
pub struct Report {
    badness: isize,
    max_usage: f32,
    cost_transitive: isize,
    target: String,
}

#[derive(Deserialize, Debug)]
pub struct Dependency {
    aliases: Vec<String>,
    dependency_type: String,
    products_used: usize,
    products_used_ratio: f32,
    target: String,
}

#[derive(Deserialize, Debug)]
pub struct Info {
    cost: usize,
    cost_transitive: usize,
    dependencies: Vec<Dependency>,
    products_total: usize,
}

/// Try to read report json file
pub fn read_report<T: DeserializeOwned>(report: PathBuf) -> Result<T, String> {
    let file = File::open(&report)
        .map_err(|e| format!("Couldn't open the file {:?}. Cause={}", &report, e))?;
    serde_json::from_reader(BufReader::new(file))
        .map_err(|e| format!("Couldn't parse json file {:?}. Cause = {}", &report, e))
}
