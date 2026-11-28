use crate::document_path;
use materialcraft_core::Package;
use std::{collections::BTreeSet, sync::Arc};

pub(crate) fn normalize_references(package: &mut Package, path: &str) {
    for resource in &mut package.resources {
        if !resource.source.is_empty() {
            resource.source = document_path::resolve(path, &resource.source);
        }
    }

    for instance in package.graphs.iter_mut().flat_map(|graph| graph.instances_mut()) {
        if !instance.package.is_empty() {
            instance.package = document_path::resolve(path, &instance.package);
        }
    }
}

impl crate::Session {
    pub(crate) fn load_file_links(&mut self, package: &mut Package) -> Vec<String> {
        for (path, source) in &self.pkg.dependencies {
            package.dependencies.entry(path.clone()).or_insert_with(|| source.clone());
        }
        let pending = references(package);
        self.load_sources(package, pending)
    }

    fn load_sources(&mut self, package: &mut Package, pending: Vec<String>) -> Vec<String> {
        read_references(package, pending, |uri, source| {
            // Only aliases actually read from the catalog are owned by it. A caller can supply its own snapshot.
            self.library_sources.insert(uri.to_owned(), Arc::clone(source));
        })
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn refresh_saved_links(&mut self, path: &str) {
        use materialcraft_formats::sbs::library;
        use std::path::Path;

        let file = Path::new(path);
        let mut changed = BTreeSet::new();
        if let Ok(root) = library::root().canonicalize()
            && file.starts_with(&root)
        {
            // The saved path is canonical. The same file may also be reached through a library symlink.
            let aliases: Vec<_> = self
                .library_sources
                .iter()
                .filter_map(|(uri, owned)| {
                    let relative = library::relative_path(uri).ok()?;
                    (self.linked_sources.get(uri).is_some_and(|source| Arc::ptr_eq(source, owned))
                        && root.join(relative).canonicalize().ok()?.as_path() == file)
                        .then(|| uri.clone())
                })
                .collect();
            if !aliases.is_empty() {
                let mut registry = Package { dependencies: self.linked_sources.clone(), ..Default::default() };
                for uri in &aliases {
                    registry.dependencies.remove(uri);
                }
                // Read the saved bytes in each alias's own directory so nested links and resources keep their base.
                for warning in self.load_sources(&mut registry, aliases.clone()) {
                    self.warn(warning);
                }
                changed.extend(
                    aliases.into_iter().filter(|uri| registry.dependencies.get(uri).is_some_and(|source| self.linked_sources.get(uri) != Some(source))),
                );
                self.linked_sources.extend(registry.dependencies);
            }
        }
        self.refresh_links_from(changed);
    }

    pub(crate) fn discard_source_changes(&mut self, path: &str) {
        self.linked_sources.remove(path);
        self.pkg.dependencies.remove(path);
        for document in self.documents.values_mut() {
            document.pkg.dependencies.remove(path);
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut package = self.pkg.clone();

            // Include inactive dependents so closing a source also refreshes their references
            package.graphs.extend(self.documents.values().flat_map(|document| document.pkg.graphs.iter().cloned()));
            package.dependencies = self.linked_sources.clone();
            let pending = references(&package);
            for warning in self.load_sources(&mut package, pending) {
                self.warn(warning);
            }

            self.linked_sources.extend(package.dependencies);
        }

        self.refresh_links_from(BTreeSet::from([path.to_string()]));
    }
}

fn references(package: &Package) -> Vec<String> {
    package.graphs.iter().flat_map(|graph| graph.instances()).filter(|instance| !instance.package.is_empty()).map(|instance| instance.package.clone()).collect()
}

/// Seed the registry with already parsed library packages before traversing dependencies. The
/// registry stays flat so shared dependencies are parsed once and cannot form Arc ownership cycles.
pub(crate) fn load_references(package: &mut Package, pending: Vec<String>) -> Vec<String> {
    resolve(package, pending, &load_file)
}

pub(crate) fn load_scoped(package: &mut Package, read: &dyn Fn(&str) -> Option<Vec<u8>>) -> Vec<String> {
    resolve(package, references(package), &|path| {
        if path.starts_with("sbs://") {
            return load_file(path);
        }

        let bytes = read(path).ok_or_else(|| format!("missing or inaccessible dependency `{path}`"))?;
        let assets = |reference: &str| read(&document_path::resolve(path, reference));
        crate::file_cmds::read_package_bytes(&bytes, path, &assets).map_err(|error| error.to_string())
    })
}

fn load_file(path: &str) -> Result<materialcraft_formats::sbs::SbsRead, String> {
    let bytes = if path.starts_with("sbs://") {
        materialcraft_formats::sbs::library::read(path)?
    } else {
        #[cfg(target_arch = "wasm32")]
        return Err("external file is unavailable in the browser".into());
        #[cfg(not(target_arch = "wasm32"))]
        crate::linked_resource::read_regular(std::path::Path::new(path)).map_err(|error| error.to_string())?
    };
    crate::file_cmds::read_package_bytes(&bytes, path, &crate::file_cmds::linked_files(path)).map_err(|error| error.to_string())
}

const MAX_DEPENDENCY_READS: usize = 4096;

fn resolve(package: &mut Package, pending: Vec<String>, read: &dyn Fn(&str) -> Result<materialcraft_formats::sbs::SbsRead, String>) -> Vec<String> {
    resolve_with_catalog(package, pending, read, |_, _| {})
}

fn read_references(package: &mut Package, pending: Vec<String>, catalog_read: impl FnMut(&str, &Arc<Package>)) -> Vec<String> {
    resolve_with_catalog(package, pending, &load_file, catalog_read)
}

fn resolve_with_catalog(
    package: &mut Package,
    mut pending: Vec<String>,
    read: &dyn Fn(&str) -> Result<materialcraft_formats::sbs::SbsRead, String>,
    mut catalog_read: impl FnMut(&str, &Arc<Package>),
) -> Vec<String> {
    let mut visited = BTreeSet::new();
    let mut warnings = Vec::new();
    let mut read_attempts = 0;
    while let Some(path) = pending.pop() {
        if !visited.insert(path.clone()) {
            continue;
        }

        if let Some(source) = package.dependencies.get(&path) {
            pending.extend(references(source));
            continue;
        }

        // Cached snapshots only need traversal, while failed reads still spend the allowance
        if read_attempts == MAX_DEPENDENCY_READS {
            warnings.push("linked material exceeds the package dependency read limit".into());
            break;
        }

        read_attempts += 1;
        match read(&path) {
            Ok(read) => {
                warnings.extend(read.warnings);
                let source = read.pkg;
                for (name, dependency) in &source.dependencies {
                    package.dependencies.entry(name.clone()).or_insert_with(|| dependency.clone());
                }
                pending.extend(references(&source));
                let source = Arc::new(source);
                if path.starts_with("sbs://") {
                    catalog_read(&path, &source);
                }
                package.dependencies.insert(path, source);
            }
            Err(error) => warnings.push(format!("Could not resolve {path}: {error}")),
        }
    }

    warnings
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, sync::Arc};

    #[test]
    fn cached_paths_do_not_spend_the_new_dependency_read_budget() {
        for cached_first in [true, false] {
            let mut package = Package::new("root");
            let mut pending = vec!["new".to_string()];
            for i in 0..MAX_DEPENDENCY_READS {
                let name = format!("cached{i}");
                package.dependencies.insert(name.clone(), Arc::new(Package::new(&name)));
                pending.push(name);
            }
            if !cached_first {
                pending.reverse();
            }
            let calls = Cell::new(0);
            let warnings = resolve(&mut package, pending, &|path| {
                calls.set(calls.get() + 1);
                assert_eq!(path, "new");
                Ok(materialcraft_formats::sbs::SbsRead { pkg: Package::new(path), warnings: vec![] })
            });
            assert!(warnings.is_empty(), "{warnings:?}");
            assert_eq!(calls.get(), 1);
            assert!(package.dependencies.contains_key("new"));
        }
    }

    #[test]
    fn failed_new_reads_still_spend_the_budget_and_duplicates_do_not() {
        let calls = Cell::new(0);
        let pending = (0..=MAX_DEPENDENCY_READS).flat_map(|i| [format!("missing{i}"), format!("missing{i}")]).collect();
        let warnings = resolve(&mut Package::new("root"), pending, &|_| {
            calls.set(calls.get() + 1);
            Err("missing".into())
        });
        assert_eq!(calls.get(), MAX_DEPENDENCY_READS);
        assert!(warnings.last().unwrap().contains("dependency read limit"));
    }
}
