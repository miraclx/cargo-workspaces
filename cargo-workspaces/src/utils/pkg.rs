use crate::utils::{
    read_config, Error, ListOpt, Listable, PackageConfig, Result, WorkspaceConfig, INTERNAL_ERR,
};

use camino::Utf8PathBuf;
use cargo_metadata::{Metadata, PackageId};
use oclif::{console::style, term::TERM_OUT, CliError};
use semver::Version;
use serde::{Deserialize, Serialize};

use std::{
    cmp::max,
    collections::{HashMap, HashSet},
    fmt,
    iter::repeat,
    path::{Path, PathBuf},
    str::FromStr,
};

#[derive(Serialize, Debug, Clone, Ord, Eq, PartialOrd, PartialEq)]
pub struct Pkg {
    #[serde(skip)]
    pub id: PackageId,
    pub name: String,
    pub version: Version,
    pub location: PathBuf,
    #[serde(skip)]
    pub path: PathBuf,
    pub private: bool,
    #[serde(skip)]
    pub config: PackageConfig,
    #[serde(skip)]
    pub manifest_path: Utf8PathBuf,
}

impl Listable for Vec<(GroupName, Pkg)> {
    fn list(&self, list: ListOpt) -> Result {
        if list.json {
            return self.json();
        }

        if self.is_empty() {
            return Ok(());
        }

        let (first, second, third) =
            self.iter()
                .fold((0, 0, 0), |(first, second, third), (_, x)| {
                    (
                        max(first, x.name.len()),
                        max(second, x.version.to_string().len() + 1),
                        max(third, max(1, x.path.as_os_str().len())),
                    )
                });

        let mut last_group_name = None;
        for (group_name, pkg) in self {
            match last_group_name.replace(group_name) {
                Some(prev_name) if group_name == prev_name => {}
                _ => {
                    if let Some(group_name) = group_name.pretty_fmt() {
                        TERM_OUT.write_line(&group_name.to_string())?;
                    }
                }
            }
            TERM_OUT.write_str(&pkg.name)?;
            let mut width = first - pkg.name.len();

            if list.long {
                TERM_OUT.write_str(&format!(
                    "{:f$} {}{:s$} {}",
                    "",
                    style(format!("v{}", pkg.version)).green(),
                    "",
                    style(pkg.path.display()).black().bright(),
                    f = width,
                    s = second - pkg.version.to_string().len() - 1,
                ))?;

                width = third - pkg.path.as_os_str().len();
            }

            if list.all && pkg.private {
                TERM_OUT.write_str(&format!(
                    "{:w$} ({})",
                    "",
                    style("PRIVATE").red(),
                    w = width
                ))?;
            }

            TERM_OUT.write_line("")?;
        }

        Ok(())
    }
}

macro_rules! ser_unit_variant {
    ($variant:ident) => {
        pub mod $variant {
            pub fn ser<S>(s: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                s.serialize_str(stringify!($variant))
            }
        }
    };
}

mod ser_grp {
    ser_unit_variant!(default);
    ser_unit_variant!(excluded);
}

#[derive(Eq, Hash, Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum GroupName {
    #[serde(serialize_with = "ser_grp::default::ser")]
    Default,
    #[serde(serialize_with = "ser_grp::excluded::ser")]
    Excluded,
    Custom(String),
}

impl GroupName {
    pub fn new(name: &str) -> Result<Self> {
        if let "default" | "excluded" = name {
            return Err(Error::ReservedGroupName { name: name.into() });
        }
        if let Err(msg) = Self::validate(name) {
            return Err(Error::InvalidGroupName { msg });
        }
        Ok(Self::Custom(name.to_owned()))
    }

    pub fn pretty_fmt(&self) -> Option<String> {
        match self {
            GroupName::Default => None,
            GroupName::Excluded => Some(style(format!("[excluded]")).bold().yellow().to_string()),
            GroupName::Custom(group_name) => Some(
                style(format!("[{}]", group_name))
                    .bold()
                    .color256(37)
                    .to_string(),
            ),
        }
    }

    pub fn validate(s: &str) -> std::result::Result<(), String> {
        for c in s.bytes() {
            match c {
                b':' => return Err(format!("invalid character `:` in group name: `{}`", s)),
                b' ' => return Err(format!("unexpected space in group name: `{}`", s)),
                _ => (),
            }
        }
        Ok(())
    }
}

impl FromStr for GroupName {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Self::validate(s).map(|_| match s {
            "default" => GroupName::Default,
            "excluded" => GroupName::Excluded,
            custom => GroupName::new(custom).expect(INTERNAL_ERR),
        })
    }
}

impl fmt::Display for GroupName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(match self {
            GroupName::Default => "default",
            GroupName::Excluded => "excluded",
            GroupName::Custom(custom) => custom.as_str(),
        })
    }
}

#[derive(Eq, Clone, Debug, PartialEq, Serialize)]
pub struct WorkspaceGroups {
    #[serde(flatten)]
    pub named_groups: HashMap<GroupName, (Option<Version>, Vec<Pkg>)>,
}

impl WorkspaceGroups {
    pub fn into_iter(mut self) -> impl Iterator<Item = ((GroupName, Option<Version>), Pkg)> {
        let default = self
            .named_groups
            .remove_entry(&GroupName::Default)
            .into_iter();
        let excluded = self
            .named_groups
            .remove_entry(&GroupName::Excluded)
            .into_iter();

        let rest = self.named_groups.into_iter();

        default
            .chain(rest)
            .chain(excluded)
            .map(|(group, (ver, pkgs))| repeat((group, ver)).zip(pkgs.into_iter()))
            .flatten()
    }
}

pub fn get_group_packages(
    metadata: &Metadata,
    workspace_config: &WorkspaceConfig,
    all: bool,
) -> Result<WorkspaceGroups> {
    let mut non_empty = false;

    let mut named_groups = workspace_config.groups.iter().fold(
        Ok(HashMap::from([
            (
                GroupName::Default,
                (workspace_config.version.clone(), vec![]),
            ),
            (GroupName::Excluded, (None, vec![])),
        ])),
        |mut acc, group| {
            if let Ok(acc) = &mut acc {
                let group_name = GroupName::new(&group.name)?;
                if acc.contains_key(&group_name) {
                    return Err(Error::DuplicateGroupName {
                        name: group.name.clone(),
                    });
                }
                if group.members.is_empty() {
                    return Err(Error::EmptyGroup {
                        name: group.name.clone(),
                    });
                }
                acc.insert(group_name, (group.version.clone(), vec![]));
            }
            acc
        },
    )?;

    for id in &metadata.workspace_members {
        if let Some(pkg) = metadata.packages.iter().find(|x| x.id == *id) {
            let private = pkg.publish.as_ref().map_or(false, Vec::is_empty);

            let loc = match pkg.manifest_path.strip_prefix(&metadata.workspace_root) {
                Ok(loc) => loc,
                Err(_) => {
                    return Err(Error::PackageNotInWorkspace {
                        id: pkg.id.repr.clone(),
                        ws: metadata.workspace_root.to_string(),
                    })
                }
            };

            let loc = if loc.is_file() {
                loc.parent().expect(INTERNAL_ERR)
            } else {
                loc
            };

            let pkg = Pkg {
                id: pkg.id.clone(),
                name: pkg.name.clone(),
                version: pkg.version.clone(),
                location: metadata.workspace_root.join(loc).into(),
                path: if loc.as_os_str().is_empty() {
                    Path::new(".").to_owned()
                } else {
                    loc.into()
                },
                private,
                config: read_config(&pkg.metadata)?,
                manifest_path: pkg.manifest_path.clone(),
            };

            let (group_name, member_pat) = 'found_group: loop {
                if let Some(ref exclude_spec) = workspace_config.exclude {
                    for member in exclude_spec.members.iter() {
                        if member.matches(&pkg.path) {
                            break 'found_group (
                                GroupName::Excluded,
                                Some(member.pattern.as_str()),
                            );
                        }
                    }
                }

                let mut matched_groups = vec![];

                non_empty |= true;

                for group in &workspace_config.groups {
                    for member in &group.members {
                        if member.matches(&pkg.path) {
                            matched_groups.push((
                                GroupName::new(&group.name).expect(INTERNAL_ERR),
                                Some(member.pattern.as_str()),
                            ));
                            break;
                        }
                    }
                }

                if let Ok(manifest) =
                    toml::from_str::<CrateManifest>(&std::fs::read_to_string(&pkg.manifest_path)?)
                {
                    if let CrateManifestPackageEntryVersion::Table { .. } = manifest.package.version
                    {
                        if !matched_groups.is_empty() {
                            return Err(Error::PackageExistsInMultipleGroups {
                                name: pkg.name,
                                rel_path: pkg.path.display().to_string(),
                                inherits: true,
                                groups: matched_groups
                                    .into_iter()
                                    .map(|(group_name, _)| group_name)
                                    .collect(),
                            });
                        }
                    }
                }

                break 'found_group match matched_groups.len() {
                    0 => (GroupName::Default, None),
                    1 => matched_groups.remove(0),
                    _ => {
                        return Err(Error::PackageExistsInMultipleGroups {
                            name: pkg.name,
                            rel_path: pkg.path.display().to_string(),
                            inherits: false,
                            groups: matched_groups
                                .into_iter()
                                .map(|(group_name, _)| group_name)
                                .collect(),
                        })
                    }
                };
            };

            named_groups
                .get_mut(&group_name)
                .expect(INTERNAL_ERR)
                .1
                .push((pkg, member_pat));
        } else {
            Error::PackageNotFound {
                id: id.repr.clone(),
            }
            .print()?;
        }
    }

    if !non_empty {
        return Err(Error::EmptyWorkspace);
    }

    let mut unmatched_group_patterns = HashMap::new();

    for group in &workspace_config.groups {
        let (_, pkgs) = named_groups
            .get(&GroupName::new(&group.name).expect(INTERNAL_ERR))
            .expect(INTERNAL_ERR);
        'member: for member in &group.members {
            for (_, pat) in pkgs {
                if member.pattern.as_str() == pat.expect(INTERNAL_ERR) {
                    continue 'member;
                }
            }
            unmatched_group_patterns
                .entry(group.name.clone())
                .or_insert_with(HashSet::new)
                .insert(member.pattern.as_str().to_owned());
        }
    }

    if !unmatched_group_patterns.is_empty() {
        return Err(Error::UnmatchedCustomGroupPattern(unmatched_group_patterns));
    }

    if let Some(exclude_spec) = &workspace_config.exclude {
        let mut unmatched_exclude_group_patterns = HashSet::new();
        let (_, pkgs) = named_groups.get(&GroupName::Excluded).expect(INTERNAL_ERR);
        'member: for member in &exclude_spec.members {
            for (_, pat) in pkgs {
                if member.pattern.as_str() == pat.expect(INTERNAL_ERR) {
                    continue 'member;
                }
            }
            unmatched_exclude_group_patterns.insert(member.pattern.as_str().to_owned());
        }
        if !unmatched_exclude_group_patterns.is_empty() {
            return Err(Error::UnmatchedExcludeGroupPattern(
                unmatched_exclude_group_patterns,
            ));
        }
    }

    named_groups
        .values_mut()
        .for_each(|(_, pkgs)| pkgs.sort_by_key(|(pkg, _)| pkg.name.clone()));

    Ok(WorkspaceGroups {
        named_groups: named_groups
            .into_iter()
            .map(|(k, (ver, pkgs))| {
                (
                    k,
                    (
                        ver,
                        pkgs.into_iter()
                            .filter_map(|(pkg, _)| (all || !pkg.private).then_some(pkg))
                            .collect(),
                    ),
                )
            })
            .collect(),
    })
}

#[derive(Deserialize)]
struct CrateManifest {
    package: CrateManifestPackageEntry,
}

#[derive(Deserialize)]
struct CrateManifestPackageEntry {
    version: CrateManifestPackageEntryVersion,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum CrateManifestPackageEntryVersion {
    String(String),
    Table {
        #[serde(
            rename = "workspace",
            deserialize_with = "validate_workspace_version_value"
        )]
        _workspace: (),
    },
}

fn validate_workspace_version_value<'de, D>(d: D) -> std::result::Result<(), D::Error>
where
    D: serde::Deserializer<'de>,
{
    bool::deserialize(d)?
        .then(|| ())
        .ok_or_else(|| serde::de::Error::invalid_value(serde::de::Unexpected::Bool(false), &"true"))
}
