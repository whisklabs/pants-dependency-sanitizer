#[macro_use]
extern crate serde_derive;

use std::path::PathBuf;
use structopt::StructOpt;

mod sanitizer;

#[derive(StructOpt, Debug)]
#[structopt(
    name = "pants-cleaner",
    about = "A tool for optimize pants jvm dependencies"
)]
pub struct Config {
    /// Full path to Pants 'dep-usage.jvm' report file in Json format.
    /// You should create it before using this tool like this
    /// `./pants -q dep-usage.jvm --no-summary src/:: > deps.json`
    /// and provide full path to this file.
    #[structopt(short, long, parse(from_os_str), default_value = "deps.json")]
    report_file: PathBuf,

    /// Applies any action only for modules that start with this include_prefix.
    #[structopt(short, long, default_value = "src/scala/")]
    prefix: String,

    /// If dependency was  annotated with this marker tath it will be skipped to sanitize(removing)
    #[structopt(short, long, default_value = "#skip-sanitize")]
    skip_marker: String,

    #[structopt(subcommand)]
    cmd: Command,
}

#[derive(StructOpt, Debug)]
pub enum Command {
    /// Manage unused but declared modules dependencies
    #[structopt(name = "unused")]
    Unused {
        #[structopt(subcommand)]
        cmd: UnusedSubCommand,
    },
    /// Manage undeclared but used transitively modules dependencies
    #[structopt(name = "undeclared")]
    Undeclared {
        #[structopt(subcommand)]
        cmd: UndeclaredSubCommand,
    },
    /// Finds all BUILD files downstream to the current folder (with --prefix) and sorts dependencies
    /// in 'dependencies' and 'exports' blocks
    #[structopt(name = "sort")]
    Sort {},
}

#[derive(StructOpt, Debug)]
pub enum UnusedSubCommand {
    /// Shows all unused dependencies
    #[structopt(name = "show")]
    Show,
    /// Removes all unused dependencies
    #[structopt(name = "fix")]
    Fix,
}

#[derive(StructOpt, Debug)]
pub enum UndeclaredSubCommand {
    /// Shows all undeclared dependencies
    #[structopt(name = "show")]
    Show,
    /// Add all undeclared dependencies to corresponded BUILD files
    #[structopt(name = "fix")]
    Fix,
}

fn main() {
    let config: Config = dbg!(Config::from_args());
    sanitizer::perform(config);
}
