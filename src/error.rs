use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("config error: {0}")]
    Config(String),

    #[error("template error: {0}")]
    Template(String),

    #[error("frontmatter parse error in {file}: {message}")]
    Frontmatter { file: String, message: String },

    #[error("github api error: {0}")]
    GitHub(String),

    #[error("render error for template '{template}': {message}")]
    Render { template: String, message: String },

    #[error("yaml validation error: {0}")]
    YamlValidation(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Tera(#[from] tera::Error),

    #[error(transparent)]
    SerdeYaml(#[from] serde_norway::Error),

    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),

    #[error(transparent)]
    TomlDeser(#[from] toml::de::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
