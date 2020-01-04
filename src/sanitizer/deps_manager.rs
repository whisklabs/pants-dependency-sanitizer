//! This module provides functionality for read from and write to Pants BUILD file.

use regex::Regex;
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
        line.contains(&format!("'{}:{}'", self.folder, self.module_name))       // full address
            || (self.is_simple() && line.contains(&format!("'{}'", &self.folder)))  // only folder
            || line.contains(&format!("':{}'", self.module_name)) // only module name
    }

    pub fn as_str(&self) -> String {
        if self.is_simple() {
            self.folder.to_string()
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
// todo refactor, use 'run_for_block'
pub fn remove_deps(
    module: &Address,
    deps: Vec<Address>,
    skip_marker: &str,
) -> Result<usize, Box<dyn Error>> {
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

                        if inside_module_section && line.contains("dependencies") {
                            inside_module_dep_section = true;
                        }

                        if inside_module_dep_section && line.contains(']') {
                            inside_module_dep_section = false;
                            inside_module_section = false; // actually no, but it's ok so simplifying
                        }

                        if inside_module_dep_section
                            && !line.contains(skip_marker)
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
pub fn add_deps(
    module: &Address,
    deps: Vec<Address>,
    skip_marker: &str,
) -> Result<usize, Box<dyn Error>> {
    let mut counter = 0;
    for entry in fs::read_dir(&module.folder)? {
        let entry = entry?;
        if entry.file_name() == "BUILD" {
            // read existed, add undeclared and sort

            let updated_deps =
                add_deps_to_file(entry.path(), &module, deps, &mut counter, skip_marker)?;

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
// todo refactor, use 'run_for_block'
fn add_deps_to_file(
    file: PathBuf,
    module: &Address,
    deps: Vec<Address>,
    counter: &mut isize,
    skip_marker: &str,
) -> Result<Vec<String>, Box<dyn Error>> {
    let file = BufReader::new(File::open(file)?);

    let deps_iter = deps
        .into_iter()
        .map(|dep| format!("        '{}',", dep.as_str()));

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
            if line.ends_with(',') || line.contains(skip_marker) {
                updated_deps.insert(line.replace('"', "'"));
            } else if !line.is_empty() {
                updated_deps.insert(line.replace('"', "'") + ",");
            };
            continue;
        }

        if inside_module_section && line.contains("dependencies") {
            inside_module_dep_section = true;
            result.push(line);
            continue;
        }

        result.push(line);
    }

    Ok(result)
}

/// Finds block in the specified BUILD file and executes `block_fn` for each founded blocks.
///
/// # Arguments
///
/// * `build_file` - path to BUILD file
/// * `block_start_fn` - marks that some block is started
/// * `block_end_fn` - marks that some block is ended
/// * `block_fn` - some action that will be executed when block is ended for each lines of this block
///
pub fn run_for_block(
    build_file: PathBuf,
    block_start_fn: fn(&str) -> bool,
    block_end_fn: fn(&str) -> bool,
    block_fn: fn(Vec<String>) -> Vec<String>,
) -> Result<(), Box<dyn Error>> {
    let file = BufReader::new(File::open(&build_file)?);

    let mut line_buffer: BTreeSet<String> = BTreeSet::new();
    let mut inside_block = false;

    let lines: Vec<String> = file.lines().filter_map( |line| {
        match line {
            Ok(line) if block_start_fn(&line) => {
                assert!(
                    line_buffer.is_empty(),
                    "Buffer should be empty, inner blocks isn't supported"
                );
                inside_block = true;
                Some(vec![line])
            }
            Ok(line) if inside_block && block_end_fn(&line) => {
                // reached block end, runs `block_fn`
                let mut result = block_fn(line_buffer.iter().cloned().collect());
                line_buffer.clear();
                result.push(line);
                inside_block = false;
                Some(result)
            }
            Ok(line) if inside_block => {
                // inside a block, accumulate lines into buffer
                line_buffer.insert(line);
                None
            }
            Ok(line) => {
                // other lines just ignored
                Some(vec![line])
            }
            Err(err) => {
                println!("Error reading BUILD file {:?}: {:?}", build_file, err);
                None
            }
        }
    })
        .flatten()
        .collect();

    // write filtered dependencies back in BUILD file
    let mut file = BufWriter::new(File::create(&build_file)?);
    for line in lines {
        writeln!(file, "{}", line)?;
    }
    file.flush()?;

    Ok(())
}

const DEPS_START: &str = r"dependencies[\s]*=[\s]*\[";

const EXPORTS_START: &str = r"exports[\s]*=[\s]*\[";

#[inline]
pub fn deps_block_start(line: &str) -> bool {
    Regex::new(DEPS_START).unwrap().is_match(line)
}

#[inline]
pub fn exports_block_start(line: &str) -> bool {
    Regex::new(EXPORTS_START).unwrap().is_match(line)
}

#[inline]
pub fn block_ends(line: &str) -> bool {
    line.contains("]")
}

#[cfg(test)]
mod tests {
    use crate::sanitizer::deps_manager::{block_ends, deps_block_start, exports_block_start};

    #[test]
    fn deps_block_start_test() {
        assert!(!deps_block_start(""));
        assert!(!deps_block_start("dependencies"));
        assert!(!deps_block_start("deps"));

        assert!(!deps_block_start("dependencies="));
        assert!(!deps_block_start("dependencies["));

        assert!(deps_block_start("dependencies=["));
        assert!(deps_block_start("dependencies= ["));
        assert!(deps_block_start("  dependencies = ["));
    }

    #[test]
    fn exports_block_start_test() {
        assert!(!exports_block_start(""));
        assert!(!exports_block_start("exports"));

        assert!(!exports_block_start("exports="));
        assert!(!exports_block_start("exports["));

        assert!(exports_block_start("exports=["));
        assert!(exports_block_start("exports= ["));
        assert!(exports_block_start("  exports = ["));
    }

    #[test]
    fn block_ends_test() {
        assert!(!block_ends(""));
        assert!(!block_ends("deps"));

        assert!(block_ends("]"));
        assert!(block_ends(" ]"));
        assert!(block_ends("] "));
        assert!(block_ends(" ] "));
    }

    // todo test run_for_block
}
