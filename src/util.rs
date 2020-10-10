use anyhow::{bail, Result};
use log::{error, trace};
use std::fs::File;
use std::io::prelude::*;
use std::io::BufReader;
use std::path::Path;
use thiserror::Error;
use yaml_rust::{yaml, Yaml};

#[derive(Debug, Error)]
#[error("Invalid Yaml type found for key: {key}")]
pub struct InvalidYamlTypeError {
    key: String,
}

/// Helper function static yaml strings.
pub fn yaml_string(s: &str) -> Yaml {
    Yaml::String(String::from(s))
}

/// Get hash digest of given file with chosen length
pub fn get_hash(path: &Path, length: u8) -> Result<String> {
    let hash = if length == 0 {
        bail!("Length cannot be zero!");
    } else if length <= 32 {
        get_explicit_hash::<sha2::Sha256>(path)?
    } else if length <= 64 {
        get_explicit_hash::<sha2::Sha512>(path)?
    } else {
        bail!("Length should be equal to or smaller than 64.");
    };
    Ok(hash[..length as usize].to_string())
}

fn get_explicit_hash<Hasher: sha2::Digest>(path: &Path) -> Result<String> {
    let mut hash = Hasher::new();
    let mut reader = BufReader::new(File::open(path)?);
    loop {
        let buf = &reader.fill_buf()?;
        let to_write = buf.len();
        if to_write > 0 {
            hash.update(buf);
            &reader.consume(to_write);
        } else {
            break;
        }
    }
    Ok(base64::encode_config(hash.finalize(), base64::URL_SAFE))
}

macro_rules! make_yaml_getter {
    ($function_name:ident, $variant:ident, $return_type:ty) => {
        #[allow(dead_code)]
        pub fn $function_name<'a>(
            dict: &'a yaml::Hash,
            name: &str,
        ) -> Result<Option<&'a $return_type>, InvalidYamlTypeError> {
            match dict.get(&yaml_string(name)) {
                Some(Yaml::$variant(v)) => Ok(Some(&v)),
                Some(_) => Err(InvalidYamlTypeError {
                    key: name.to_string(),
                }),
                None => Ok(None),
            }
        }
    };
}

make_yaml_getter! {get_string_from, String, String}
make_yaml_getter! {get_bool_from, Boolean, bool}
make_yaml_getter! {get_dict_from, Hash, yaml::Hash}
make_yaml_getter! {get_array_from, Array, yaml::Array}
make_yaml_getter! {get_int_from, Integer, i64}
make_yaml_getter! {get_real_from, Real, String}

pub fn get_required<
    'a,
    T,
    F: Fn(&'a yaml::Hash, &str) -> Result<Option<&'a T>, InvalidYamlTypeError>,
>(
    dict: &'a yaml::Hash,
    name: &str,
    getter: F,
) -> Result<&'a T> {
    match getter(dict, name)? {
        Some(v) => Ok(v),
        None => bail!("Required key '{}' not defined!", name),
    }
}

/// Optional value: Outter Option is if there was an error, inner Option is the actual type that
/// can be None.
pub fn get_optional<
    'a,
    T,
    F: Fn(&'a yaml::Hash, &str) -> Result<Option<&'a T>, InvalidYamlTypeError>,
>(
    dict: &'a yaml::Hash,
    name: &str,
    getter: F,
) -> Result<Option<&'a T>> {
    match getter(dict, name)? {
        Some(v) => Ok(Some(v)),
        None => {
            trace!("Optional key '{}' not defined.", name);
            Ok(None)
        }
    }
}

#[cfg(test)]
pub mod test {
    use anyhow::{Context, Result};
    /// Perform test initialization that needs to be called prior to any test.
    pub fn init() -> Result<()> {
        simple_logger::SimpleLogger::new()
            .with_level(log::LevelFilter::Trace)
            .init()
            .context("Failed to set up logger for tests.")
    }
}
