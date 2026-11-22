use crate::{Error, Result};
use std::path::{Path, PathBuf};
use tempfile::{Builder, NamedTempFile};

pub(crate) struct Output {
    temp: NamedTempFile,
    destination: PathBuf,
    overwrite: bool,
}

impl Output {
    pub fn new(input: &Path, destination: &Path, overwrite: bool) -> Result<Self> {
        if !input.is_file() {
            return Err(Error::Invalid(format!(
                "Input is not a file: {}",
                input.display()
            )));
        }
        if destination.exists() {
            if input.canonicalize()? == destination.canonicalize()? {
                return Err(Error::Invalid(
                    "Input and output must be different files".into(),
                ));
            }
            if !overwrite {
                return Err(Error::Invalid(format!(
                    "Output exists (use --overwrite): {}",
                    destination.display()
                )));
            }
            if !destination.is_file() {
                return Err(Error::Invalid("Output is not a regular file".into()));
            }
        }
        let parent = destination
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        std::fs::create_dir_all(parent)?;
        let suffix = format!(".{}", crate::extension(destination)?);
        let temp = Builder::new()
            .prefix(".mediaforge-")
            .suffix(&suffix)
            .tempfile_in(parent)?;
        Ok(Self {
            temp,
            destination: destination.into(),
            overwrite,
        })
    }
    pub fn path(&self) -> &Path {
        self.temp.path()
    }
    pub fn commit(self) -> Result<()> {
        self.temp.as_file().sync_all()?;
        let result = if self.overwrite {
            self.temp.persist(&self.destination)
        } else {
            self.temp.persist_noclobber(&self.destination)
        };
        result.map_err(|e| Error::Io(e.error))?;
        Ok(())
    }
}
