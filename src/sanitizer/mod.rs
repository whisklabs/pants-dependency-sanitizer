//! Provides functionality to optimizing Pants dependencies.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

use serde::de::DeserializeOwned;
use serde_json;
use walkdir::{DirEntry, WalkDir};

use crate::sanitizer::deps_manager::Address;
use crate::Command::{Sort, Undeclared, Unused};
use crate::{Config, UndeclaredSubCommand, UnusedSubCommand};
use std::env;
use std::error::Error;

mod deps_manager;

/// Perform Action corresponded to the Config.
pub fn perform(config: Config) {
    match config.cmd {
        Unused { cmd } => match cmd {
            UnusedSubCommand::Show => show_unused(config.report_file, config.prefix),
            UnusedSubCommand::Fix => {
                fix_unused(config.report_file, config.prefix, config.skip_marker)
            }
        },
        Undeclared { cmd } => match cmd {
            UndeclaredSubCommand::Show => show_undeclared(config.report_file, config.prefix),
            UndeclaredSubCommand::Fix => {
                fix_undeclared(config.report_file, config.prefix, &config.skip_marker)
            }
        },
        Sort {} => {
            sort_recursively(config.prefix, &config.skip_marker).expect("Cant sort dependencies")
        }
    }
}

/// Print report about all unused dependencies.
fn show_unused(report: PathBuf, prefix: String) {
    let unused = select(report, "unused", prefix);
    let modules = unused.len();
    let unused_amount: usize = unused.values().map(Vec::len).sum();
    println!(
        "{:#?}\n modules affected: {}, total dependencies unused: {}",
        &unused, modules, unused_amount
    );
}

/// Removes all unused dependencies from all corresponded BUILD files.
fn fix_unused(report: PathBuf, prefix: String, skip_marker: String) {
    let unused = select(report, "unused", prefix);
    for (module, deps) in unused {
        let removed = deps_manager::remove_deps(&module, &deps, &skip_marker)
            .unwrap_or_else(|_| panic!("Couldn't remove unused for module: {:?}", module));
        println!("{:?} removed: {}", module, removed)
    }
}

/// Print report about all undeclared dependencies.
fn show_undeclared(report: PathBuf, prefix: String) {
    let undeclared = select(report, "undeclared", prefix);
    let modules = undeclared.len();
    let undeclared_amount: usize = undeclared.values().map(Vec::len).sum();
    println!(
        "{:#?}\n modules affected: {}, total dependencies undeclared: {}",
        &undeclared, modules, undeclared_amount
    );
}

/// Add to corresponded BUILD files all undeclared but used transitively dependencies
fn fix_undeclared(report: PathBuf, prefix: String, skip_marker: &str) {
    let undeclared = select(report, "undeclared", prefix);
    for (module, deps) in undeclared {
        let added = deps_manager::add_deps(&module, deps, skip_marker)
            .unwrap_or_else(|_| panic!("Couldn't add undeclared deps to the module: {:?}", module));
        println!("{:?} added: {}", module, added)
    }
}

/// Aggregates modules and their dependencies with specified type.
fn select(
    report: PathBuf,
    dependency_type: &str,
    prefix: String,
) -> BTreeMap<Address, Vec<Address>> {
    let json = read_report::<HashMap<String, Info>>(report).expect("Couldn't read as json");
    json.into_iter()
        .filter_map(|(module, info)| {
            if module.contains("3rdparty") || !module.starts_with(&prefix) {
                // don't care about unused deps in 3rdparty and  modules that aren't matched a prefix
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

/** Finds all BUILD files recursively and sort dependencies. */
fn sort_recursively(prefix: String, skip_marker: &str) -> Result<(), Box<dyn Error>> {
    let mut current_dir = env::current_dir()?;
    current_dir.push(prefix);

    WalkDir::new(current_dir).into_iter().for_each(|result| {
        match result {
            Ok(entry) if is_build_file(&entry) => {
                println!("sorted {}", entry.path().display());
                deps_manager::run_for_block(
                    entry.clone().into_path(),
                    |line| {
                        deps_manager::deps_block_start(line)
                            || deps_manager::exports_block_start(line)
                    },
                    deps_manager::block_ends,
                    |set: BTreeSet<String>| set,
                    skip_marker,
                )
                .expect(&format!("Cant sort {:?}", entry));
            }
            _ => {
                // skip any error
            }
        }
    });

    Ok(())
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

#[inline]
fn is_build_file(entry: &DirEntry) -> bool {
    entry.file_type().is_file() && entry.file_name() == "BUILD"
}
