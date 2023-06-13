use derive_more::Deref;
use rustc_hash::FxHashSet;
use shellexpand::tilde;
use std::{
    ffi::OsStr,
    fmt::{self, Display, Formatter},
    fs,
    hash::Hash,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Deref, Eq, Hash, PartialEq)]
pub struct PackagesPath(PathBuf);

impl PackagesPath {
    pub fn find_surrounding_package(&self, path: &Path) -> Option<Package> {
        let mut candidate = dunce::canonicalize(path).unwrap_or_else(|error| {
            panic!(
                "Couldn't `find_surrounding_package(\"{}\")`: `{error}`",
                path.to_string_lossy(),
            )
        });
        if !candidate.is_dir() {
            candidate = candidate.parent().unwrap().to_path_buf();
        }

        loop {
            let children = fs::read_dir(&candidate)
                .unwrap()
                .map(|child| child.unwrap().file_name())
                .collect::<FxHashSet<_>>();

            if !children.contains(OsStr::new("_.candy")) {
                return None;
            }

            if children.contains(OsStr::new("_package.candy")) {
                break;
            } else if let Some(parent) = candidate.parent() {
                candidate = parent.to_path_buf();
            } else {
                return None;
            }
        }

        // The `candidate` folder contains the `_package.candy` file.
        Some(
            if let Ok(path_relative_to_packages) = candidate.strip_prefix(&**self) {
                Package::Managed(path_relative_to_packages.to_path_buf())
            } else {
                Package::User(candidate)
            },
        )
    }
}

impl Display for PackagesPath {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}", self.to_string_lossy())
    }
}

impl TryFrom<&str> for PackagesPath {
    type Error = String;

    fn try_from(path: &str) -> Result<Self, Self::Error> {
        Path::new(tilde(path).as_ref()).try_into()
    }
}
impl TryFrom<&Path> for PackagesPath {
    type Error = String;

    fn try_from(path: &Path) -> Result<Self, Self::Error> {
        let path = dunce::canonicalize(path).map_err(|err| {
            format!(
                "The packages path `{}` does not exist or its path is invalid: {err}.",
                path.to_string_lossy(),
            )
        })?;

        if !path.is_dir() {
            return Err(format!(
                "The packages path `{}` is not a directory.",
                path.to_string_lossy(),
            ));
        }

        Ok(PackagesPath(path))
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Package {
    /// A package written by the user.
    User(PathBuf),

    /// A package managed by the Candy tooling. This is in some special cache
    /// directory where `use`d packages are downloaded to.
    Managed(PathBuf),

    /// An anonymous package. This is created for single untitled files that are
    /// not yet persisted to disk (such as when opening a new VSCode tab and
    /// typing some code).
    Anonymous { url: String },

    /// This package can make the tooling responsible for calls. For example,
    /// the fuzzer and constant evaluator use this.
    Tooling(String),
}

impl Package {
    pub fn builtins() -> Package {
        Package::Managed(PathBuf::from("Builtins"))
    }
    pub fn core() -> Package {
        Package::Managed(PathBuf::from("Core"))
    }

    pub fn to_path(&self, packages_path: &PackagesPath) -> Option<PathBuf> {
        match self {
            Package::User(path) => Some(path.clone()),
            Package::Managed(path) => Some(packages_path.join(path)),
            Package::Anonymous { .. } => None,
            Package::Tooling(_) => None,
        }
    }
}
impl Display for Package {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Package::User(path) => write!(f, "{path:?}"),
            Package::Managed(path) => match path.as_os_str().to_str() {
                Some(string) => write!(f, "{string}"),
                None => write!(f, "{path:?}"),
            },
            Package::Anonymous { url } => write!(f, "anonymous:{url}"),
            Package::Tooling(tooling) => write!(f, "tooling:{tooling}"),
        }
    }
}
