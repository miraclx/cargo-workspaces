<!-- omit in TOC -->
# cargo-workspaces

Inspired by [Lerna](https://lerna.js.org/)

A tool that optimizes the workflow around cargo workspaces with `git` and `cargo` by providing utilities to
version, publish, execute commands and more.

I made this to work on [clap](https://github.com/clap-rs/clap) and other projects that rely on workspaces.
But this will also work on single crates because by default every individual crate is a workspace.

1. [Installation](#installation)
2. [Usage](#usage)
   1. [Init](#init)
   2. [Create](#create)
   3. [List](#list)
   4. [Changed](#changed)
   5. [Exec](#exec)
   6. [Version](#version)
      1. [Fixed or Independent](#fixed-or-independent)
   7. [Publish](#publish)
3. [Config](#config)
4. [Changelog](#changelog)

## Installation

```
cargo install cargo-workspaces
```

## Usage

The installed tool can be called by `cargo workspaces` or `cargo ws`. Both of them point to the same.

You can use `cargo ws help` or `cargo ws help <subcmd>` anytime to understand allowed options.

The basic commands available for this tool are given below. Assuming you run them inside a cargo workspace.

### Init

Initializes a new cargo workspace in the given directory. Creates `Cargo.toml` if it does not exist and
fills the `members` with the all the crates that can be found in that directory.

```
USAGE:
    cargo workspaces init [path]

ARGS:
    <path>    Path to the workspace root [default: .]

FLAGS:
    -h, --help    Prints help information
```

### Create

Interactively creates a new crate in the workspace. *We recommend using this instead of `cargo new`*. All
the crates start with `0.0.0` version because the [version](#version) is responsible for determining the
version.

```
USAGE:
    cargo workspaces create <path>

ARGS:
    <path>    Path for the crate relative to the workspace manifest

FLAGS:
    -h, --help    Prints help information
```

### List

Lists crates in the workspace.

```
USAGE:
    cargo workspaces list [FLAGS]

FLAGS:
    -a, --all     Show private crates that are normally hidden
    -h, --help    Prints help information
        --json    Show information as a JSON array
    -l, --long    Show extended information
```

Several aliases are available.

* `cargo ws ls` implies `cargo ws list`
* `cargo ws ll` implies `cargo ws list --long`
* `cargo ws la` implies `cargo ws list --all`

### Changed

List crates that have changed since the last git tag. This is useful to see the list of crates that
would be the subjects of the next [version](#version) or [publish](#publish) command.

```
USAGE:
    cargo workspaces changed [FLAGS] [OPTIONS]

FLAGS:
    -a, --all                    Show private crates that are normally hidden
    -h, --help                   Prints help information
        --include-merged-tags    Include tags from merged branches
        --json                   Show information as a JSON array
    -l, --long                   Show extended information

OPTIONS:
        --force <pattern>             Always include targeted crates matched by glob even when there are no changes
        --ignore-changes <pattern>    Ignore changes in files matched by glob
        --since <since>               Use this git reference instead of the last tag
```

### Exec

Executes an arbitrary command in each crate of the workspace.

```
USAGE:
    cargo workspaces exec [FLAGS] <args>...

ARGS:
    <args>...

FLAGS:
    -h, --help       Prints help information
        --no-bail    Continue executing command despite non-zero exit in a given crate
```

For example, if you want to run `ls -l` in each crate, you can simply do `cargo ws exec ls -l`.

### Version

Bump versions of the crates in the workspace. This command does the following:

1. Identifies crates that have been updated since the previous tagged release
2. Prompts for a new version according to the crate
3. Modifies crate manifest to reflect new release
4. Update intra-workspace dependency version constraints if needed
5. Commits those changes
6. Tags the commit
7. Pushes to the git remote

You can influence the above steps with the flags and options for this command.

```
USAGE:
    cargo workspaces version [FLAGS] [OPTIONS] [ARGS]

ARGS:
    <bump>      Increment all versions by the given explicit semver keyword while skipping the prompts for them
                [possible values: major, minor, patch, premajor, preminor, prepatch, prerelease, custom]
    <custom>    Specify custom version value when 'bump' is set to 'custom'

FLAGS:
    -a, --all                    Also do versioning for private crates (will not be published)
        --amend                  Amend the existing commit, instead of generating a new one
        --exact                  Specify inter dependency version numbers exactly with `=`
    -h, --help                   Prints help information
        --include-merged-tags    Include tags from merged branches
        --no-git-commit          Do not commit version changes
        --no-git-push            Do not push generated commit and tags to git remote
        --no-git-tag             Do not tag generated commit
        --no-global-tag          Do not create a global tag for a workspace
        --no-individual-tags     Do not tag individual versions for crates
    -y, --yes                    Skip confirmation prompt

OPTIONS:
        --allow-branch <pattern>            Specify which branches to allow from [default: master]
        --force <pattern>                   Always include targeted crates matched by glob even when there are no changes
        --git-remote <remote>               Push git changes to the specified remote [default: origin]
        --ignore-changes <pattern>          Ignore changes in files matched by glob
        --individual-tag-prefix <prefix>    Customize prefix for individual tags (should contain `%n`) [default: %n@]
    -m, --message <message>                 Use a custom commit message when creating the version commit
        --pre-id <identifier>               Specify prerelease identifier
        --tag-prefix <prefix>               Customize tag prefix (can be empty) [default: v]
```

#### Fixed or Independent

By default, all the crates in the workspace will share a single version. But if you want the crate to have
it's version be independent of the other crates, you can add the following to that crate:

```toml
[package.metadata.workspaces]
independent = true
```

If you want groups of crates to share a single version, independent of the rest of the workspace, see [Groups and Grouping](#groups-and-grouping).

For more details, check [Config](#config) section below.

#### Exclusion

To have crates opt-out from being versioned, you can add the following to the workspace:

```toml
[workspace.metadata.workspaces]
exclude = [
    "./crates/*",
    "path/to/some/specific/crate",
]
```

For more details, check [Config](#config) section below.

#### Groups and Grouping

Use this to group certain crates together and have them share a single version that is independent from the rest of workspace.

This also lets you choose to version and publish only a specific subset of crates in the workspace<sup>[1](#groups-and-grouping-1)</sup> instead of all at once.
As well as visualizing [changes](#changed) by group or [listing](#list) crates on a per-group basis.

<sup id="groups-and-grouping-1"><sup>1</sup> If a crate from outside a group depends on crates within the group that is versioned, their versions are bumped too.</sup>

To create groups, add the following to the workspace:

```toml
[[workspace.metadata.workspaces.group]]
name = "foobar"
members = [
    "./foo",
    "./bar/*",
]

[[workspace.metadata.workspaces.group]]
name = "another-group"
members = [ "crates/*" ]
```

> Note that group membership is exclusive, a crate isn't allowed to be a part of multiple groups.
> Also, the `default` group name is reserved for crates that don't belong to any group.
> And, the `excluded` group name is reserved for crates that are marked to be [excluded](#exclusion) from being versioned

For more details, check [Config](#config) section below.

### Publish

Publish all the crates from the workspace in the correct order according to the dependencies. By default,
this command runs [version](#version) first. If you do not want that to happen, you can supply the
`--from-git` option.

> Note: dev-dependencies are not taken into account when building the dependency
> graph used to determine the proper publishing order. This is because
> dev-dependencies are ignored by `cargo publish` - as such, a dev-dependency on a
> local crate (with a `path` attribute), should _not_ have a `version` field.

```
USAGE:
    cargo workspaces publish [FLAGS] [OPTIONS] [ARGS]

ARGS:
    <bump>      Increment all versions by the given explicit semver keyword while skipping the prompts for them
                [possible values: major, minor, patch, premajor, preminor, prepatch, prerelease, custom]
    <custom>    Specify custom version value when 'bump' is set to 'custom'

FLAGS:
    -a, --all                    Also do versioning for private crates (will not be published)
        --amend                  Amend the existing commit, instead of generating a new one
        --exact                  Specify inter dependency version numbers exactly with `=`
        --from-git               Publish crates from the current commit without versioning
    -h, --help                   Prints help information
        --include-merged-tags    Include tags from merged branches
        --no-git-commit          Do not commit version changes
        --no-git-push            Do not push generated commit and tags to git remote
        --no-git-tag             Do not tag generated commit
        --no-global-tag          Do not create a global tag for a workspace
        --no-individual-tags     Do not tag individual versions for crates
        --no-verify              Skip crate verification (not recommended)
    -y, --yes                    Skip confirmation prompt

OPTIONS:
        --allow-branch <pattern>            Specify which branches to allow from [default: master]
        --force <pattern>                   Always include targeted crates matched by glob even when there are no changes
        --git-remote <remote>               Push git changes to the specified remote [default: origin]
        --ignore-changes <pattern>          Ignore changes in files matched by glob
        --individual-tag-prefix <prefix>    Customize prefix for individual tags (should contain `%n`) [default: %n@]
    -m, --message <message>                 Use a custom commit message when creating the version commit
        --pre-id <identifier>               Specify prerelease identifier
        --tag-prefix <prefix>               Customize tag prefix (can be empty) [default: v]
        --token <token>                     The token to use for publishing
```

## Config

There are two kinds of configuration options.

* **Workspace**: Options that are specified in the workspace with `[workspace.metadata.workspaces]`
* **Package**: Options that are specified in the package with `[package.metadata.workspaces]`

### Package Configuration

```toml
[package.metadata.workspaces]
independent = false  # This package should be versioned independently from the rest
```

### Workspace Configuration

```toml
[workspace.metadata.workspaces]
allow_branch = "master"                 # Specify which branches to allow from [default: master]
no_individual_tags = false              # Do not tag individual versions for crates
exclude = [ "./foo", "./bar/*" ]        # List of crates to exclude from actions

[[workspace.metadata.workspaces.group]]
name = "utils"                          # Name for this group
members = [ "./utils/a", "./utils/b" ]  # Member crates belonging to this group
```

<!-- omit in TOC -->
## Contributors
Here is a list of [Contributors](http://github.com/pksunkara/cargo-workspaces/contributors)

<!-- omit in TOC -->
### TODO

## Changelog
Please see [CHANGELOG.md](CHANGELOG.md).

<!-- omit in TOC -->
## License
MIT/X11

<!-- omit in TOC -->
## Bug Reports
Report [here](http://github.com/pksunkara/cargo-workspaces/issues).

<!-- omit in TOC -->
## Creator
Pavan Kumar Sunkara (pavan.sss1991@gmail.com)

Follow me on [github](https://github.com/users/follow?target=pksunkara), [twitter](http://twitter.com/pksunkara)
