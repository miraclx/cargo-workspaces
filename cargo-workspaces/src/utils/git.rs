use crate::utils::{debug, info, validate_value_containing_name, Error, Pkg, WorkspaceConfig};

use camino::Utf8PathBuf;
use clap::Parser;
use globset::Glob;
use oclif::term::ERR_YELLOW;
use semver::Version;

use std::{
    collections::BTreeMap as Map,
    process::{Command, ExitStatus},
};

pub fn git<'a>(
    root: &Utf8PathBuf,
    args: &[&'a str],
) -> Result<(ExitStatus, String, String), Error> {
    debug!("git", args.to_vec().join(" "));

    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .map_err(|err| Error::Git {
            err,
            args: args.iter().map(|x| x.to_string()).collect(),
        })?;

    Ok((
        output.status,
        String::from_utf8(output.stdout)?.trim().to_owned(),
        String::from_utf8(output.stderr)?.trim().to_owned(),
    ))
}

#[derive(Debug, Parser)]
#[clap(next_help_heading = "GIT OPTIONS")]
pub struct GitOpt {
    /// Do not commit version changes, omitting this will tag the current commit
    #[clap(long, conflicts_with_all = &["amend", "message", "allow-branch"])]
    pub no_git_commit: bool,

    /// Specify which branches to allow from [default: master]
    #[clap(long, value_name = "pattern", forbid_empty_values(true))]
    pub allow_branch: Option<String>,

    /// Amend the existing commit, instead of generating a new one
    #[clap(long)]
    pub amend: bool,

    /// Use a custom commit message when creating the version commit [default: Release %v]
    #[clap(
        short,
        long,
        conflicts_with_all = &["amend"],
        forbid_empty_values(true)
    )]
    pub message: Option<String>,

    /// Do not tag generated commit (implies --no-individual-tags and --no-global-tag)
    #[clap(long, conflicts_with_all = &["tag-msg", "tag-prefix", "tag-private", "individual-tag-prefix", "individual-tag-msg", "no-individual-tags", "no-global-tag"])]
    pub no_git_tag: bool,

    /// Do not tag individual versions for crates
    #[clap(long, conflicts_with_all = &["individual-tag-prefix"])]
    pub no_individual_tags: bool,

    /// Do not create a global tag for a workspace
    #[clap(long)]
    pub no_global_tag: bool,

    /// Also tag individual versions of private packages
    #[clap(long)]
    pub tag_private: bool,

    /// Customize tag prefix for global tags (can be empty)
    #[clap(long, default_value = "v", value_name = "prefix")]
    pub tag_prefix: String,

    /// Customize prefix for individual tags (should contain `%n`)
    #[clap(
        long,
        default_value = "%n@",
        value_name = "prefix",
        validator = validate_value_containing_name,
        forbid_empty_values(true)
    )]
    pub individual_tag_prefix: String,

    /// Customize tag msg, defaults to tag name (can contain `%v`)
    #[clap(long, value_name = "msg", multiple_occurrences = true)]
    pub tag_msg: Vec<String>,

    /// Customize tag msg for individual tags, defaults to individual tag name (can contain `%n` and `%v`)
    #[clap(long, value_name = "msg")]
    pub individual_tag_msg: Option<String>,

    /// Do not push generated commit and tags to git remote
    #[clap(long, conflicts_with_all = &["git-remote"])]
    pub no_git_push: bool,

    /// Push git changes to the specified remote
    #[clap(
        long,
        default_value = "origin",
        value_name = "remote",
        forbid_empty_values(true)
    )]
    pub git_remote: String,

    /// Do not perform any git operations (implies --no-git-commit and --no-git-tag)
    #[clap(long, conflicts_with_all = &[
        "no-git-commit", "allow-branch", "amend", "message",
        "no-git-tag", "no-individual-tags", "no-global-tag",
        "tag-private", "tag-prefix", "individual-tag-prefix",
        "tag-msg", "individual-tag-msg", "no-git-push", "git-remote"
    ])]
    pub no_git: bool,
}

impl GitOpt {
    pub fn validate(
        &self,
        root: &Utf8PathBuf,
        config: &WorkspaceConfig,
    ) -> Result<Option<String>, Error> {
        if self.no_git {
            return Ok(None);
        }

        let (_, out, err) = git(root, &["rev-list", "--count", "--all", "--max-count=1"])?;

        if err.contains("not a git repository") {
            return Err(Error::NotGit);
        }

        if out == "0" {
            return Err(Error::NoCommits);
        }

        if self.no_git_push
            || (self.no_git_commit
                && (self.no_git_tag || (self.no_global_tag && self.no_individual_tags)))
        {
            return Ok(None);
        }

        let (_, out, _) = git(
            root,
            &[
                "for-each-ref",
                "--format='%(refname)'",
                &format!("refs/remotes/{}", self.git_remote),
            ],
        )?;

        if out.is_empty() {
            return Err(Error::NoRemote {
                remote: self.git_remote.clone(),
            });
        }

        let (_, branch, _) = git(root, &["rev-parse", "--abbrev-ref", "HEAD"])?;

        if branch == "HEAD" {
            if self.no_git_commit {
                return Ok(None);
            }
            return Err(Error::NotBranch);
        }

        // Get the final `allow_branch` value
        let allow_branch_default_value = String::from("master");
        let allow_branch = self.allow_branch.as_ref().unwrap_or_else(|| {
            config
                .allow_branch
                .as_ref()
                .unwrap_or(&allow_branch_default_value)
        });

        // Treat `main` as `master`
        let test_branch = if branch == "main" && allow_branch.as_str() == "master" {
            "master".into()
        } else {
            branch.clone()
        };

        let pattern = Glob::new(&allow_branch)?;

        if !pattern.compile_matcher().is_match(&test_branch) {
            return Err(Error::BranchNotAllowed {
                branch,
                pattern: pattern.glob().to_string(),
            });
        }

        git(root, &["remote", "update", &self.git_remote])?;

        let remote_branch = format!("{}/{}", self.git_remote, branch);

        let (_, out, _) = git(
            root,
            &[
                "rev-list",
                "--left-only",
                "--count",
                &format!("{}...{}", remote_branch, branch),
            ],
        )?;

        if out != "0" {
            return Err(Error::BehindRemote {
                branch,
                upstream: remote_branch,
            });
        }

        return Ok(Some(branch));
    }

    pub fn commit(
        &self,
        root: &Utf8PathBuf,
        new_version: &Option<Version>,
        new_versions: &Map<String, (Pkg, Version)>,
    ) -> Result<(), Error> {
        if self.no_git || self.no_git_commit {
            return Ok(());
        }

        info!("git", "committing changes");

        let added = git(root, &["add", "-u"])?;

        if !added.0.success() {
            return Err(Error::NotAdded(added.1, added.2));
        }

        let mut args = vec!["commit".to_string()];

        if self.amend {
            args.push("--amend".to_string());
            args.push("--no-edit".to_string());
        } else {
            args.push("-m".to_string());

            let mut msg = "Release %v";

            if let Some(supplied) = &self.message {
                msg = supplied;
            }

            let mut msg = self.commit_msg(msg, new_versions);

            msg = msg.replace(
                "%v",
                &new_version
                    .as_ref()
                    .map_or("independent packages".to_string(), |x| format!("{}", x)),
            );

            args.push(msg);
        }

        let committed = git(root, &args.iter().map(|x| x.as_str()).collect::<Vec<_>>())?;

        if !committed.0.success() {
            return Err(Error::NotCommitted(committed.1, committed.2));
        }

        Ok(())
    }

    pub fn global_tag(
        &self,
        root: &Utf8PathBuf,
        new_version: &Version,
        new_versions: &Map<String, (Pkg, Version)>,
    ) -> Result<Option<String>, Error> {
        if self.no_git || self.no_git_tag || self.no_global_tag {
            return Ok(None);
        }

        let tag = format!("{}{}", &self.tag_prefix, new_version);
        let mut msgs = Vec::with_capacity(self.tag_msg.capacity().max(1));
        for msg in &self.tag_msg {
            let mut s = String::new();
            for (i, scope) in msg.split("%{").enumerate() {
                if i == 0 {
                    s.push_str(scope);
                    continue;
                }
                let (template, rest) = scope
                    .split_once("}")
                    .ok_or_else(|| Error::UnterminatedTagMsgScope(msg.clone()))?;
                for (_, (p, version)) in new_versions.iter() {
                    if !p.private || self.tag_private {
                        s.push_str(
                            &template
                                .replace("%n", &p.name)
                                .replace("%v", &version.to_string()),
                        );
                    }
                }
                s.push_str(rest);
            }
            msgs.push(s.replace("%v", &new_version.to_string()));
        }
        if msgs.is_empty() {
            msgs.push(tag.clone());
        }

        self.tag(root, &tag, &msgs)?;

        Ok(Some(tag))
    }

    pub fn individual_tag(
        &self,
        root: &Utf8PathBuf,
        pkg_name: &str,
        is_private: bool,
        new_version: &str,
        config: &WorkspaceConfig,
    ) -> Result<Option<String>, Error> {
        if self.no_git
            || self.no_git_tag
            || self.no_individual_tags
            || config.no_individual_tags.unwrap_or_default()
            || (is_private && !self.tag_private)
        {
            return Ok(None);
        }

        let tag = format!(
            "{}{}",
            self.individual_tag_prefix.replace("%n", pkg_name),
            new_version
        );
        let msg = self.individual_tag_msg.as_ref().map_or(tag.clone(), |msg| {
            msg.replace("%n", pkg_name).replace("%v", new_version)
        });

        self.tag(root, &tag, &[msg])?;

        Ok(Some(tag))
    }

    pub fn push(
        &self,
        root: &Utf8PathBuf,
        branch: &Option<String>,
        tags: &Vec<String>,
    ) -> Result<(), Error> {
        if self.no_git || self.no_git_push {
            return Ok(());
        }

        let mut rest = vec![];
        if let Some(branch) = &branch {
            rest.push(branch as _);
        }
        if !tags.is_empty() {
            rest.push("tag");
            rest.extend(tags.iter().map(|x| x.as_str()));
        }
        if rest.is_empty() {
            return Ok(());
        }

        info!("git", "pushing");

        let mut args = vec!["push", "--no-follow-tags", &self.git_remote];
        args.extend(rest);

        let pushed = git(root, &args)?;

        if !pushed.0.success() {
            return Err(Error::NotPushed(pushed.1, pushed.2));
        }

        Ok(())
    }

    fn tag(&self, root: &Utf8PathBuf, tag: &str, msgs: &[String]) -> Result<(), Error> {
        let (_, tags, _) = git(root, &["tag"])?;
        if let None = tags.split("\n").find(|existing_tag| &tag == existing_tag) {
            let mut args = vec!["tag", tag, "-a"];
            for msg in msgs {
                args.extend(&["-m", &msg]);
            }
            info!("git", format!("tagging {}", ERR_YELLOW.apply_to(tag)));

            let tagged = git(root, &args)?;

            if !tagged.0.success() {
                return Err(Error::NotTagged(tag.to_string(), tagged.1, tagged.2));
            }
        } else {
            info!(
                "git",
                format!("tag {} already exists", ERR_YELLOW.apply_to(tag))
            );
        }
        Ok(())
    }

    fn commit_msg(&self, msg: &str, new_versions: &Map<String, (Pkg, Version)>) -> String {
        format!(
            "{}\n\n{}\n\nGenerated by cargo-workspaces",
            msg,
            new_versions
                .iter()
                .map(|x| format!("{}@{}", x.0, x.1 .1))
                .collect::<Vec<_>>()
                .join("\n")
        )
    }
}
