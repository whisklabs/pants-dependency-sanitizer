//! This module provides functionality for read from and write to Pants BUILD file.

use std::error::Error;
use std::fmt;
use std::fmt::{Debug, Formatter};
use std::fs;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};

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
        line.contains(&self.folder) || line.contains(&format!("\":{}\"", self.module_name))
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
