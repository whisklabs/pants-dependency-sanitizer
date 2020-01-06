//! This module provides functionality for read from and write to Pants BUILD file.

use crate::sanitizer::deps_manager;
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

/// Representation for Pants address.
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
pub fn remove_deps(
    module: &Address,
    deps: &Vec<Address>,
    skip_marker: &str,
) -> Result<i32, Box<dyn Error>> {
    let mut counter = 0;

    for entry in fs::read_dir(&module.folder)? {
        let entry = entry?;
        if entry.file_name() == "BUILD" {
            let mut inside_module_section = module.is_simple();
            counter += run_for_block(
                entry.path(),
                |line| {
                    if line.contains("name=") && line.contains(&module.module_name) {
                        inside_module_section = true;
                    }
                    inside_module_section && deps_manager::deps_block_start(line)
                },
                deps_manager::block_ends,
                |lines| {
                    lines
                        .into_iter()
                        .filter(|line| {
                            line.contains(skip_marker)
                                || !deps.iter().any(|target| target.match_line(&line))
                        })
                        .collect()
                },
                skip_marker,
            )
            .unwrap();
        }
    }
    Ok(counter.abs())
}

/// Finds a BUILD file and inserts lines with undeclared dependencies, returns number of inserted lines.
pub fn add_deps(
    module: &Address,
    deps: Vec<Address>,
    skip_marker: &str,
) -> Result<i32, Box<dyn Error>> {
    let mut counter = 0;

    for entry in fs::read_dir(&module.folder)? {
        let entry = entry?;

        if entry.file_name() == "BUILD" {
            // add undeclared and sort

            let mut inside_module_section = module.is_simple();

            counter += run_for_block(
                entry.path(),
                |line: &str| {
                    if line.contains("name=") && line.contains(&module.module_name) {
                        inside_module_section = true;
                    }

                    inside_module_section && deps_manager::deps_block_start(line)
                },
                deps_manager::block_ends,
                |mut file_deps: BTreeSet<String>| {
                    // add undeclared deps to deps from file
                    let deps_iter = deps
                        .clone()
                        .into_iter()
                        .map(|dep| format!("        '{}',", dep.as_str()));

                    file_deps.extend(deps_iter);
                    file_deps
                },
                skip_marker,
            )
            .unwrap();
        }
    }
    Ok(counter)
}

/// Finds block in the specified BUILD file and executes `block_fn` for each founded blocks.
///
/// # Arguments
///
/// * `build_file` - path to BUILD file
/// * `block_start_fn` - marks that some block is started
/// * `block_end_fn` - marks that some block is ended
/// * `block_fn` - some action that will be executed when block is ended for each lines of this block
/// * `skip_marker` - marker that prevent removing dependencies
///
pub fn run_for_block<F1: FnMut(&str) -> bool, F2: FnMut(BTreeSet<String>) -> BTreeSet<String>>(
    build_file: PathBuf,
    mut block_start_fn: F1,
    block_end_fn: fn(&str) -> bool,
    mut block_fn: F2,
    skip_marker: &str,
) -> Result<i32, Box<dyn Error>> {
    let file = BufReader::new(File::open(&build_file)?);

    let mut line_buffer: BTreeSet<String> = BTreeSet::new();
    let mut inside_block = false;
    let mut line_edited: i32 = 0;

    let lines: Vec<String> = file
        .lines()
        .filter_map(|line| {
            match line {
                Ok(line) if !inside_block && block_start_fn(&line) => {
                    assert!(
                        line_buffer.is_empty(),
                        "Buffer should be empty, inner blocks isn't supported"
                    );
                    inside_block = true;
                    Some(vec![line])
                }
                Ok(line) if inside_block && block_end_fn(&line) => {
                    // reached block end, runs `block_fn`
                    let mut result = block_fn(line_buffer.clone());
                    line_edited = result.len() as i32 - line_buffer.len() as i32;
                    line_buffer.clear();
                    result.insert(line);
                    inside_block = false;
                    Some(result.into_iter().collect())
                }
                Ok(line) if inside_block => {
                    // inside a block, accumulate lines into buffer
                    if line.ends_with(',') || line.contains(skip_marker) {
                        line_buffer.insert(line.replace('"', "'"));
                    } else if !line.is_empty() {
                        line_buffer.insert(line.replace('"', "'") + ",");
                    };
                    None
                }
                Ok(line) => {
                    // other lines just ignore
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

    Ok(line_edited)
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
}
