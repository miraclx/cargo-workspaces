use crate::utils::GroupName;

use lazy_static::lazy_static;
use oclif::{term::ERR_YELLOW, CliError};
use thiserror::Error;

use std::{
    collections::{HashMap, HashSet},
    io,
    sync::atomic::{AtomicBool, Ordering},
};

lazy_static! {
    static ref DEBUG: AtomicBool = AtomicBool::new(false);
}

pub fn get_debug() -> bool {
    DEBUG.load(Ordering::Relaxed)
}

pub fn set_debug() {
    DEBUG.store(true, Ordering::Relaxed);
}

macro_rules! _info {
    ($desc:expr, $val:expr) => {{
        oclif::term::TERM_ERR.write_line(&format!(
            "{} {} {}",
            oclif::term::ERR_GREEN.apply_to("info"),
            oclif::term::ERR_MAGENTA.apply_to($desc),
            $val
        ))?;
        oclif::term::TERM_ERR.flush()?;
    }};
}

macro_rules! _debug {
    ($desc:expr, $val:expr) => {{
        if $crate::utils::get_debug() {
            oclif::term::TERM_ERR.write_line(&format!(
                "{} {} {}",
                oclif::term::ERR_GREEN.apply_to("debug"),
                oclif::term::ERR_MAGENTA.apply_to($desc),
                $val
            ))?;
            oclif::term::TERM_ERR.flush()?;
        }
    }};
}

pub(crate) use _debug as debug;
pub(crate) use _info as info;

#[derive(Error, Debug)]
pub enum Error {
    #[error("package {id} is not inside workspace {ws}")]
    PackageNotInWorkspace { id: String, ws: String },
    #[error("unable to find package {id}")]
    PackageNotFound { id: String },
    #[error(
        "the package `{name}` ({rel_path}) {note}: {}",
        .groups.iter().map(|group| format!("`{}`", group)).collect::<Vec<_>>().join(", "),
        note = if *.inherits {
            "which inherits the workspace version is also included in these groups"
        } else {
            "was matched in multiple groups"
        }
    )]
    PackageExistsInMultipleGroups {
        name: String,
        rel_path: String,
        inherits: bool,
        groups: Vec<GroupName>,
    },
    #[error("did not find any package")]
    EmptyWorkspace,
    #[error("the group `{name}` has no members")]
    EmptyGroup { name: String },
    #[error("the group `{name}` is defined multiple times")]
    DuplicateGroupName { name: String },
    #[error("the group `{name}` is a reserved group name")]
    ReservedGroupName { name: String },
    #[error("{msg}")]
    InvalidGroupName { msg: String },
    #[error(
        "these group member patterns matched no packages:\n{}",
        .0.iter().map(|(group_name, pkgs)| format!(
            "{:8} - `{}` : {}",
            "",
            group_name,
            pkgs.iter().map(|group| format!("`{}`", group)).collect::<Vec<_>>().join(", ")
        )).collect::<Vec<_>>().join("\n")
    )]
    UnmatchedCustomGroupPattern(HashMap<String, HashSet<String>>),
    #[error(
        "these excluded member patterns matched no packages: {}",
        .0.iter().map(|group| format!("`{}`", group)).collect::<Vec<_>>().join(", ")
    )]
    UnmatchedExcludeGroupPattern(HashSet<String>),
    #[error("package {0}'s manifest has no parent directory")]
    ManifestHasNoParent(String),
    #[error("unable to read metadata specified in Cargo.toml: {0}")]
    BadMetadata(serde_json::Error),

    #[error("unable to verify package {0}")]
    Verify(String),
    #[error("unable to publish package {0}")]
    Publish(String),
    #[error("publishing has timed out")]
    PublishTimeout,
    #[error("unable to update Cargo.lock")]
    Update,

    #[error("{0} value must contain '%n'")]
    MustContainPercentN(String),

    #[error("unable to create crate")]
    Create,

    #[error("given path {0} is not a folder")]
    WorkspaceRootNotDir(String),
    #[error("unable to initialize workspace: {0}")]
    Init(String),

    #[error("unable to run cargo command with args {args:?}, got {err}")]
    Cargo { err: io::Error, args: Vec<String> },
    #[error("unable to run git command with args {args:?}, got {err}")]
    Git { err: io::Error, args: Vec<String> },

    #[error("child command failed to exit successfully")]
    Bail,

    #[error("not a git repository")]
    NotGit,
    #[error("no commits in this repository")]
    NoCommits,
    #[error("not on a git branch")]
    NotBranch,
    #[error("remote {remote} not found or branch {branch} not in {remote}")]
    NoRemote { remote: String, branch: String },
    #[error("local branch {branch} is behind upstream {upstream}")]
    BehindRemote { upstream: String, branch: String },
    #[error("not allowed to run on branch {branch} because it doesn't match pattern {pattern}")]
    BranchNotAllowed { branch: String, pattern: String },
    #[error("unable to add files to git index, out = {0}, err = {1}")]
    NotAdded(String, String),
    #[error("unable to commit to git, out = {0}, err = {1}")]
    NotCommitted(String, String),
    #[error("unable to tag {0}, out = {1}, err = {2}")]
    NotTagged(String, String, String),
    #[error("unterminated tag message scope")]
    UnterminatedTagMsgScope(String),
    #[error("unable to push to remote, out = {0}, err = {1}")]
    NotPushed(String, String),

    #[error("could not understand 'cargo config get' output: {0}")]
    BadConfigGetOutput(String),
    #[error("crates index error: {0}")]
    CratesRegistry(#[from] crates_index::Error),

    #[error("{0}")]
    Semver(#[from] semver::ReqParseError),
    #[error("{0}")]
    Glob(#[from] glob::PatternError),
    #[error("{0}")]
    Globset(#[from] globset::Error),
    #[error("{0}")]
    Serde(#[from] serde_json::Error),
    #[error("{0}")]
    Io(#[from] io::Error),
    #[error("cannot convert command output to string, {0}")]
    FromUtf8(#[from] std::string::FromUtf8Error),
}

impl CliError for Error {
    fn color(self) -> Self {
        match self {
            Self::PackageNotInWorkspace { id, ws } => Self::PackageNotInWorkspace {
                id: format!("{}", ERR_YELLOW.apply_to(id)),
                ws: format!("{}", ERR_YELLOW.apply_to(ws)),
            },
            Self::PackageNotFound { id } => Self::PackageNotFound {
                id: format!("{}", ERR_YELLOW.apply_to(id)),
            },
            Self::Verify(pkg) => Self::Verify(format!("{}", ERR_YELLOW.apply_to(pkg))),
            Self::Publish(pkg) => Self::Publish(format!("{}", ERR_YELLOW.apply_to(pkg))),
            Self::MustContainPercentN(val) => {
                Self::MustContainPercentN(format!("{}", ERR_YELLOW.apply_to(val)))
            }
            Self::WorkspaceRootNotDir(path) => {
                Self::WorkspaceRootNotDir(format!("{}", ERR_YELLOW.apply_to(path)))
            }
            Self::NoRemote { remote, branch } => Self::NoRemote {
                remote: format!("{}", ERR_YELLOW.apply_to(remote)),
                branch: format!("{}", ERR_YELLOW.apply_to(branch)),
            },
            Self::BehindRemote { upstream, branch } => Self::BehindRemote {
                upstream: format!("{}", ERR_YELLOW.apply_to(upstream)),
                branch: format!("{}", ERR_YELLOW.apply_to(branch)),
            },
            Self::BranchNotAllowed { branch, pattern } => Self::BranchNotAllowed {
                branch: format!("{}", ERR_YELLOW.apply_to(branch)),
                pattern: format!("{}", ERR_YELLOW.apply_to(pattern)),
            },
            Self::NotTagged(tag, out, err) => {
                Self::NotTagged(format!("{}", ERR_YELLOW.apply_to(tag)), out, err)
            }
            _ => self,
        }
    }
}
