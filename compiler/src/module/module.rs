use itertools::Itertools;
use lsp_types::Url;
use std::{
    fmt::{self, Display, Formatter},
    fs,
    hash::Hash,
    path::PathBuf,
};
use tracing::{error, warn};

use super::package::Package;

#[derive(Debug, PartialEq, Eq, Clone, Hash, PartialOrd, Ord)]
pub struct Module {
    pub package: Package,
    pub path: Vec<String>,
    pub kind: ModuleKind,
}
#[derive(Debug, PartialEq, Eq, Clone, Hash, PartialOrd, Ord)]
pub enum ModuleKind {
    Code,
    Asset,
}

impl Module {
    pub fn from_package_root_and_url(package_root: PathBuf, url: Url, kind: ModuleKind) -> Self {
        match url.scheme() {
            "file" => {
                Module::from_package_root_and_file(package_root, url.to_file_path().unwrap(), kind)
            }
            "untitled" => Module {
                package: Package::Anonymous {
                    url: url
                        .to_string()
                        .strip_prefix("untitled:")
                        .unwrap()
                        .to_string(),
                },
                path: vec![],
                kind,
            },
            _ => panic!("Unsupported URI scheme: {}", url.scheme()),
        }
    }
    pub fn from_package_root_and_file(
        package_root: PathBuf,
        file: PathBuf,
        kind: ModuleKind,
    ) -> Self {
        let relative_path = fs::canonicalize(&file).unwrap_or_else(|err| {
            panic!(
                "File `{}` does not exist or its path is invalid: {err}.",
                file.to_string_lossy(),
            )
        });
        let relative_path =
            match relative_path.strip_prefix(fs::canonicalize(&package_root).unwrap()) {
                Ok(path) => path,
                Err(_) => {
                    return Module {
                        package: Package::External(file),
                        path: vec![],
                        kind,
                    }
                }
            };

        let mut path = relative_path
            .components()
            .map(|component| match component {
                std::path::Component::Prefix(_) => unreachable!(),
                std::path::Component::RootDir => unreachable!(),
                std::path::Component::CurDir => panic!("`.` is not allowed in a module path."),
                std::path::Component::ParentDir => {
                    panic!("`..` is not allowed in a module path.")
                }
                std::path::Component::Normal(it) => {
                    it.to_str().expect("Invalid UTF-8 in path.").to_owned()
                }
            })
            .collect_vec();

        if kind == ModuleKind::Code {
            let last = path.pop().unwrap();
            let last = last
                .strip_suffix(".candy")
                .expect("Code module doesn't end with `.candy`?");
            if last != "_" {
                path.push(last.to_string());
            }
        }

        Module {
            package: Package::User(package_root),
            path,
            kind,
        }
    }

    pub fn to_possible_paths(&self) -> Option<Vec<PathBuf>> {
        let mut path = self.package.to_path()?;
        for component in self.path.clone() {
            path.push(component);
        }
        Some(match self.kind {
            ModuleKind::Asset => vec![path],
            ModuleKind::Code => vec![
                {
                    let mut path = path.clone();
                    path.push("_.candy");
                    path
                },
                {
                    let mut path = path.clone();
                    path.set_extension("candy");
                    path
                },
            ],
        })
    }
    fn try_to_path(&self) -> Option<PathBuf> {
        let paths = self.to_possible_paths().unwrap_or_else(|| {
            panic!(
                "Tried to get content of anonymous module {self} that is not cached by the language server."
            )
        });
        for path in paths {
            match path.try_exists() {
                Ok(true) => return Some(path),
                Ok(false) => {}
                Err(error) if matches!(error.kind(), std::io::ErrorKind::NotFound) => {}
                Err(_) => error!("Unexpected error when reading file {path:?}."),
            }
        }
        None
    }

    pub fn dump_associated_debug_file(&self, debug_type: &str, content: &str) {
        let Some(mut path) = self.try_to_path() else { return; };
        path.set_extension(format!("candy.{}", debug_type));
        fs::write(path.clone(), content).unwrap_or_else(|error| {
            warn!(
                "Couldn't write to associated debug file {}: {error}.",
                path.to_string_lossy(),
            )
        });
    }
}

impl Display for Module {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(
            f,
            "{}:{}",
            self.package,
            self.path
                .iter()
                .map(|component| component.to_string())
                .join("/")
        )
    }
}
