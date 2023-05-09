use crate::utils::{
    get_group_packages, read_config, rename_packages, validate_value_containing_name, Error,
    GroupName, WorkspaceConfig,
};
use cargo_metadata::Metadata;
use clap::Parser;
use globset::{Error as GlobsetError, Glob};
use std::{collections::BTreeMap as Map, fs};

/// Rename crates in the project
#[derive(Debug, Parser)]
pub struct Rename {
    /// Rename private crates too
    #[clap(short, long)]
    pub all: bool,

    /// Ignore the crates matched by glob
    #[clap(long, value_name = "pattern")]
    pub ignore: Option<String>,

    /// Rename only a specific crate
    #[clap(short, long, value_name = "crate", conflicts_with_all = &["all", "ignore"])]
    pub from: Option<String>,

    /// The value that should be used as new name (should contain `%n`)
    #[clap(forbid_empty_values(true))]
    pub to: String,

    /// Comma separated list of crate groups to rename
    #[clap(
        long,
        multiple_occurrences = true,
        use_value_delimiter = true,
        number_of_values = 1
    )]
    pub groups: Vec<GroupName>,
}

impl Rename {
    pub fn run(self, metadata: Metadata) -> Result<(), Error> {
        let config: WorkspaceConfig = read_config(&metadata.workspace_metadata)?;

        let workspace_groups =
            get_group_packages(&metadata, &config, self.all || self.from.is_some())?;

        let ignore = self
            .ignore
            .clone()
            .map(|x| Glob::new(&x))
            .map_or::<Result<_, GlobsetError>, _>(Ok(None), |x| Ok(x.ok()))?;

        let mut rename_map = Map::new();

        if let Some(from) = self.from {
            if workspace_groups
                .into_iter()
                .map(|(_, p)| p.name)
                .collect::<Vec<_>>()
                .contains(&&from)
            {
                rename_map.insert(from, self.to.clone());
            } else {
                return Err(Error::PackageNotFound { id: from });
            }
        } else {
            // Validate the `to` value
            validate_value_containing_name(&self.to)
                .map_err(|_| Error::MustContainPercentN("<TO>".into()))?;

            for ((group_name, _), pkg) in workspace_groups.into_iter() {
                if let Some(pattern) = &ignore {
                    if pattern.compile_matcher().is_match(&pkg.name) {
                        continue;
                    }
                }
                if !(self.groups.is_empty() || self.groups.contains(&group_name)) {
                    continue;
                }

                let new_name = self.to.replace("%n", &pkg.name);

                rename_map.insert(pkg.name, new_name);
            }
        }

        for pkg in &metadata.packages {
            if rename_map.contains_key(&pkg.name)
                || pkg
                    .dependencies
                    .iter()
                    .map(|p| &p.name)
                    .any(|p| rename_map.contains_key(p))
            {
                fs::write(
                    &pkg.manifest_path,
                    format!(
                        "{}\n",
                        rename_packages(
                            fs::read_to_string(&pkg.manifest_path)?,
                            &pkg.name,
                            &rename_map,
                        )?
                    ),
                )?;
            }
        }

        Ok(())
    }
}
