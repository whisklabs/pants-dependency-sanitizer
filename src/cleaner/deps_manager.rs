//! This module provides functionality for read from and write to Pants BUILD file.

use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;
use std::fmt::{Debug, Formatter};
use std::fs;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::PathBuf;
use std::string::ToString;

/// Representation fof Pants address.
#[derive(Clone, PartialOrd, Ord, PartialEq, Eq)]
pub struct Address {
    pub folder: String,
    pub module_name: String,
}

impl Address {
    pub fn from_str(str: &str) -> Self {
        let split = str.split(':').collect::<Vec<_>>();
        let folder = split[0].to_string();
        let module_name = split[1].to_string();
        Address {
            folder,
            module_name,
        }
    }
    /// In the case when 1 folder == 1 module return true.
    pub fn is_simple(&self) -> bool {
        self.folder.ends_with(&self.module_name)
    }

    /// Line corresponds to this address.
    pub fn match_line(&self, line: &str) -> bool {
        (self.is_simple() && line.contains(&self.folder))
            || line.contains(&format!("\":{}\"", self.module_name))
    }

    pub fn as_str(&self) -> String {
        if self.is_simple() {
            format!("{}", self.folder)
        } else {
            format!("{}:{}", self.folder, self.module_name)
        }
    }
}

impl Debug for Address {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}:{:?}", self.folder, self.module_name)
    }
}

/// Finds BUILD file and removes lines with unused dependencies, returns number of removed lines.
pub fn remove_deps(module: &Address, deps: Vec<Address>) -> Result<usize, Box<dyn Error>> {
    let mut counter = 0;

    for entry in fs::read_dir(&module.folder)? {
        let entry = entry?;
        if entry.file_name() == "BUILD" {
            // read and filter unused dependencies
            let cleaned = {
                let file = BufReader::new(File::open(entry.path())?);

                let mut inside_module_section = module.is_simple();
                let mut inside_module_dep_section = false;

                file.lines()
                    .filter_map(|line| {
                        let line = line.unwrap_or_else(|_| {
                            panic!("Couldn't read line from {}/BUILD ", module.folder)
                        });

                        if line.contains("name=") && line.contains(&module.module_name) {
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

/// Finds a BUILD file and inserts lines with undeclared dependencies, returns number of inserted lines.
pub fn add_deps(module: &Address, deps: Vec<Address>) -> Result<usize, Box<dyn Error>> {
    let mut counter = 0;
    for entry in fs::read_dir(&module.folder)? {
        let entry = entry?;
        if entry.file_name() == "BUILD" {
            // read existed, add undeclared and sort

            let updated_deps = add_deps_to_file(entry.path(), &module, deps, &mut counter)?;

            // write filtered dependencies back in BUILD file
            let mut file = BufWriter::new(File::create(entry.path())?);
            for line in updated_deps {
                writeln!(file, "{}", line)?;
            }
            file.flush()?;
            break;
        } else {
        }
    }
    Ok(counter as usize)
}

/// Adds new deps to dependency block of the BUILD file.
fn add_deps_to_file(
    file: PathBuf,
    module: &Address,
    deps: Vec<Address>,
    counter: &mut isize,
) -> Result<Vec<String>, Box<dyn Error>> {
    let file = BufReader::new(File::open(file)?);

    let deps_iter = deps
        .into_iter()
        .map(|dep| format!("        \"{}\",", dep.as_str()));

    let mut result: Vec<String> = Vec::new();
    // we use BTreeSet because deps should be sorted and unique
    let mut updated_deps = BTreeSet::new();
    let mut inside_module_section = module.is_simple();
    let mut inside_module_dep_section = false;

    for line in file.lines() {
        let line = line?;

        if line.contains("name=") && line.contains(&module.module_name) {
            inside_module_section = true;
        }

        if line.contains(']') && inside_module_dep_section {
            // add undeclared to deps
            let before = updated_deps.len() as isize;
            updated_deps.extend(deps_iter.clone());
            *counter += updated_deps.len() as isize - before;
            // add deps to file
            result.extend(updated_deps.clone());
            result.push(line);
            inside_module_dep_section = false;
            inside_module_section = false; // actually no, but it's ok so simplifying
            continue;
        }

        if inside_module_dep_section {
            // we are into dep block just add new line into deps set
            if line.ends_with(',') {
                updated_deps.insert(line);
            } else if !line.is_empty() {
                updated_deps.insert(line.to_owned() + ",");
            }
            continue;
        }

        if inside_module_section && line.contains("dependencies=[") {
            inside_module_dep_section = true;
            result.push(line);
            continue;
        }

        result.push(line);
    }

    Ok(result)
}
