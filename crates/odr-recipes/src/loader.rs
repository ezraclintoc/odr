//! Loading and validating recipes from a directory tree of YAML files.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::schema::{Flow, Recipe, Step};

/// An error encountered while loading or validating recipes.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("could not read recipe directory {path}: {source}")]
    Walk {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("could not read {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("could not parse {path}: {source}")]
    Parse {
        path: PathBuf,
        source: serde_yaml_ng::Error,
    },

    #[error("recipe {path} is invalid: {reason}")]
    Invalid { path: PathBuf, reason: String },

    #[error("duplicate recipe id `{id}` in {path} (already defined elsewhere)")]
    DuplicateId { id: String, path: PathBuf },
}

/// A recipe together with the file it came from — handy for error messages and
/// for editors reporting which file to fix.
#[derive(Debug, Clone)]
pub struct LoadedRecipe {
    pub path: PathBuf,
    pub recipe: Recipe,
}

/// Load every `.yaml`/`.yml` recipe under `dir`, parse it, validate it, and
/// enforce that ids are globally unique.
pub fn load_dir(dir: impl AsRef<Path>) -> Result<Vec<LoadedRecipe>, LoadError> {
    let dir = dir.as_ref();
    let mut loaded = Vec::new();
    let mut seen_ids: HashSet<String> = HashSet::new();

    for entry in walkdir::WalkDir::new(dir).sort_by_file_name() {
        let entry = entry.map_err(|e| LoadError::Walk {
            path: dir.to_path_buf(),
            source: e.into(),
        })?;

        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if !matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("yaml") | Some("yml")
        ) {
            continue;
        }

        let recipe = load_file(path)?;

        if !seen_ids.insert(recipe.id.clone()) {
            return Err(LoadError::DuplicateId {
                id: recipe.id.clone(),
                path: path.to_path_buf(),
            });
        }

        loaded.push(LoadedRecipe {
            path: path.to_path_buf(),
            recipe,
        });
    }

    Ok(loaded)
}

/// Load and validate a single recipe file.
pub fn load_file(path: impl AsRef<Path>) -> Result<Recipe, LoadError> {
    let path = path.as_ref();
    let text = std::fs::read_to_string(path).map_err(|source| LoadError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    let recipe: Recipe = serde_yaml_ng::from_str(&text).map_err(|source| LoadError::Parse {
        path: path.to_path_buf(),
        source,
    })?;
    validate(&recipe).map_err(|reason| LoadError::Invalid {
        path: path.to_path_buf(),
        reason,
    })?;
    Ok(recipe)
}

/// Structural checks the type system can't express on its own. Kept here (not
/// in `serde`) so CI can give contributors a clear, human reason to fix.
pub fn validate(recipe: &Recipe) -> Result<(), String> {
    if recipe.id.is_empty() {
        return Err("`id` must not be empty".into());
    }
    if recipe.id != recipe.id.to_lowercase()
        || recipe
            .id
            .chars()
            .any(|c| !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'))
    {
        return Err(format!(
            "`id` must be lowercase-kebab (got `{}`)",
            recipe.id
        ));
    }
    if recipe.name.trim().is_empty() {
        return Err("`name` must not be empty".into());
    }

    match &recipe.flow {
        Flow::WebForm(f) => {
            if f.steps.is_empty() {
                return Err("web_form recipe has no steps".into());
            }
            // A web-form flow that never fills or clicks anything is almost
            // certainly a mistake worth catching before it ships.
            let does_something = f.steps.iter().any(|s| {
                matches!(
                    s,
                    Step::Fill { .. } | Step::Click { .. } | Step::Select { .. }
                )
            });
            if !does_something {
                return Err("web_form recipe never fills, selects, or clicks anything".into());
            }
        }
        Flow::Email(f) => {
            if !f.to.contains('@') {
                return Err(format!("email `to` is not an address: `{}`", f.to));
            }
        }
        Flow::Manual(f) => {
            if f.steps.is_empty() {
                return Err("manual recipe has no steps".into());
            }
        }
    }

    Ok(())
}
