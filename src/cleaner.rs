//! Provides functionality to optimizing Pants dependencies.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::error::Error;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::PathBuf;
use std::{fmt, fs};

use serde::de::DeserializeOwned;
use serde_json;

use crate::Command::{Undeclared, Unused};
use crate::{Config, UndeclaredSubCommand, UnusedSubCommand};
use serde::export::fmt::Debug;
use serde::export::Formatter;

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
            .unwrap_or_else(|_| panic!("Couldn't remove unused for module: {:?}", module));
        println!("{:?} removed: {}", module, removed)
    }
}

/// Finds BUILD file and removes lines with unused dependencies, returns number of removed lines.
fn remove_deps(module: &Address, deps: Vec<Address>) -> Result<usize, Box<dyn Error>> {
    let mut counter = 0;

    for entry in fs::read_dir(&module.folder)? {
        let entry = entry?;
        if entry.file_name() == "BUILD" {
            // read and filter unused dependencies
            let cleaned = {
                let file = BufReader::new(File::open(entry.path())?);
                if module.is_simple() {
                    // simple case when folder name hash BUILD file with one module with the same name
                    file.lines()
                        .filter_map(|line| {
                            let line = line.unwrap_or_else(|_| {
                                panic!("Couldn't read line for {}/BUILD ", module.folder)
                            });

                            if deps.iter().any(|target| target.match_line(&line)) {
                                // if line contents unused dep remove it from result
                                counter += 1;
                                None
                            } else {
                                Some(line)
                            }
                        })
                        .collect::<Vec<String>>()
                } else {
                    let mut inside_module_section = false;
                    let mut inside_module_dep_section = false;
                    file.lines()
                        .filter_map(|line| {
                            let line = line.unwrap_or_else(|_| {
                                panic!("Couldn't read line from {}/BUILD ", module.folder)
                            });

                            if line.contains(&format!("name=\"{}\"", module.module_name)) {
                                inside_module_section = true;
                            }

                            if inside_module_section && line.contains("dependencies=[") {
                                inside_module_dep_section = true;
                            }

                            if inside_module_dep_section && line.contains(']') {
                                inside_module_dep_section = false;
                                inside_module_section = false; // actually no, but it's ok so simplifying
                            }

                            if inside_module_dep_section
                                && deps.iter().any(|target| target.match_line(&line))
                            {
                                // we are in dependency block of required module
                                // if line contents unused dep remove it from result
                                counter += 1;
                                None
                            } else {
                                Some(line)
                            }
                        })
                        .collect::<Vec<String>>()
                }
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

/// Print report about all undeclared dependencies.
fn show_undeclared(report: PathBuf) {
    let undeclared = select(report, "undeclared");
    let modules = undeclared.len();
    let undeclared_amount: usize = undeclared.values().map(Vec::len).sum();
    println!(
        "{:#?}\n modules affected: {}, total dependencies undeclared: {}",
        &undeclared, modules, undeclared_amount
    );
}

/// Add to corresponded BUILD files all undeclared but used transitively dependencies
fn fix_undeclared(report: PathBuf) {
    unimplemented!()
}

/// Aggregates modules and their dependencies with specified type.
fn select(report: PathBuf, dependency_type: &str) -> BTreeMap<Address, Vec<Address>> {
    let json = read_report::<HashMap<String, Info>>(report).expect("Couldn't read as json");
    json.into_iter()
        .filter_map(|(module, info)| {
            if module.contains("3rdparty") {
                // don't care about unused deps in 3rdparty
                None
            } else {
                let unused_deps = info
                    .dependencies
                    .iter()
                    .filter_map(|dep| {
                        if dep.dependency_type == dependency_type {
                            Some(Address::from_str(&dep.target))
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>();

                if unused_deps.is_empty() {
                    None
                } else {
                    Some((Address::from_str(&module), unused_deps))
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

#[derive(Clone, PartialOrd, Ord, PartialEq, Eq)]
struct Address {
    folder: String,
    module_name: String,
}

impl Address {
    fn from_str(str: &str) -> Self {
        let split = str.split(':').collect::<Vec<_>>();
        let folder = split[0].to_string();
        let module_name = split[1].to_string();
        Address {
            folder,
            module_name,
        }
    }
    /// In the case when 1 folder == 1 module return true.
    fn is_simple(&self) -> bool {
        self.folder.ends_with(&self.module_name)
    }

    /// Line corresponds to this address.
    fn match_line(&self, line: &str) -> bool {
        line.contains(&self.folder) || line.contains(&format!("\":{}\"", self.module_name))
    }
}

impl Debug for Address {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}:{:?}", self.folder, self.module_name)
    }
}

/// Try to read report json file
pub fn read_report<T: DeserializeOwned>(report: PathBuf) -> Result<T, String> {
    let file = File::open(&report)
        .map_err(|e| format!("Couldn't open the file {:?}. Cause={}", &report, e))?;
    serde_json::from_reader(BufReader::new(file))
        .map_err(|e| format!("Couldn't parse json file {:?}. Cause = {}", &report, e))
}
