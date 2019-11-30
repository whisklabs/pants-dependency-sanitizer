//! Provides all functionality

use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::BufReader;
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
    let unused = get_unused(report);
    let modules = unused.len();
    let unused_amount: usize = unused.values().map(Vec::len).sum();
    println!(
        "{:#?}\n modules affected: {}, total dependencies unused: {}",
        &unused, modules, unused_amount
    );
}

/// Removes all unused dependencies from all corresponded BUILD files.
fn fix_unused(_report: PathBuf) {
    unimplemented!()
}

/// Aggregates modules and their unused dependencies.
fn get_unused(report: PathBuf) -> BTreeMap<String, Vec<String>> {
    let json = read_report::<HashMap<String, Info>>(report).expect("Couldn't read as json");
    json.into_iter()
        .filter_map(|(module, info)| {
            let unused_deps = info
                .dependencies
                .iter()
                .filter_map(|dep| {
                    if dep.dependency_type == "unused" {
                        Some(dep.target.to_owned())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();

            if unused_deps.is_empty() {
                None
            } else {
                Some((module, unused_deps))
            }
        })
        .collect()
}

fn show_undeclared(_report: PathBuf) {
    unimplemented!()
}

fn fix_undeclared(_report: PathBuf) {
    unimplemented!()
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
