use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// CLI configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Default output format (json or yaml)
    #[serde(default = "default_format")]
    pub default_format: String,

    /// Default LLM provider
    #[serde(default)]
    pub default_llm_provider: Option<String>,

    /// Default LLM model
    #[serde(default)]
    pub default_llm_model: Option<String>,

    /// Default vector database
    #[serde(default)]
    pub default_vector_db: Option<String>,

    /// API server URL (for remote execution)
    #[serde(default)]
    pub api_url: Option<String>,

    /// API authentication token
    #[serde(default)]
    pub api_token: Option<String>,

    /// Verbose output
    #[serde(default)]
    pub verbose: bool,

    /// Color output
    #[serde(default = "default_color")]
    pub color: bool,
}

fn default_format() -> String {
    "json".to_string()
}

fn default_color() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Self {
            default_format: default_format(),
            default_llm_provider: None,
            default_llm_model: None,
            default_vector_db: None,
            api_url: None,
            api_token: None,
            verbose: false,
            color: default_color(),
        }
    }
}

impl Config {
    /// Load configuration from file
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path()?;

        if !config_path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(&config_path)
            .with_context(|| format!("Failed to read config file: {:?}", config_path))?;

        let config: Config =
            toml::from_str(&content).with_context(|| "Failed to parse config file")?;

        Ok(config)
    }

    /// Save configuration to file
    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_path()?;

        // Create config directory if it doesn't exist
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create config directory: {:?}", parent))?;
        }

        let content = toml::to_string_pretty(self).with_context(|| "Failed to serialize config")?;

        fs::write(&config_path, content)
            .with_context(|| format!("Failed to write config file: {:?}", config_path))?;

        Ok(())
    }

    /// Get the path to the config file
    pub fn config_path() -> Result<PathBuf> {
        let home = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?;

        Ok(home.join(".oxify").join("config.toml"))
    }

    /// Initialize a new config file with defaults
    pub fn init() -> Result<()> {
        let config = Self::default();
        config.save()?;

        let config_path = Self::config_path()?;
        println!("✓ Created config file at: {}", config_path.display());
        println!("\nDefault configuration:");
        println!("  default_format: {}", config.default_format);
        println!("  color: {}", config.color);
        println!("\nEdit the config file to customize your settings.");

        Ok(())
    }

    /// Show current configuration
    pub fn show() -> Result<()> {
        let config = Self::load()?;
        let config_path = Self::config_path()?;

        println!("Configuration file: {}", config_path.display());
        println!("\nCurrent settings:");
        println!("  default_format: {}", config.default_format);
        println!(
            "  default_llm_provider: {}",
            config.default_llm_provider.as_deref().unwrap_or("(none)")
        );
        println!(
            "  default_llm_model: {}",
            config.default_llm_model.as_deref().unwrap_or("(none)")
        );
        println!(
            "  default_vector_db: {}",
            config.default_vector_db.as_deref().unwrap_or("(none)")
        );
        println!(
            "  api_url: {}",
            config.api_url.as_deref().unwrap_or("(none)")
        );
        println!("  verbose: {}", config.verbose);
        println!("  color: {}", config.color);

        Ok(())
    }

    /// Set a configuration value
    pub fn set(key: &str, value: &str) -> Result<()> {
        let mut config = Self::load()?;

        match key {
            "default_format" => {
                if value != "json" && value != "yaml" {
                    anyhow::bail!("Invalid format. Use 'json' or 'yaml'.");
                }
                config.default_format = value.to_string();
            }
            "default_llm_provider" => config.default_llm_provider = Some(value.to_string()),
            "default_llm_model" => config.default_llm_model = Some(value.to_string()),
            "default_vector_db" => config.default_vector_db = Some(value.to_string()),
            "api_url" => config.api_url = Some(value.to_string()),
            "api_token" => config.api_token = Some(value.to_string()),
            "verbose" => config.verbose = value.parse()
                .with_context(|| "Invalid boolean value. Use 'true' or 'false'.")?,
            "color" => config.color = value.parse()
                .with_context(|| "Invalid boolean value. Use 'true' or 'false'.")?,
            _ => anyhow::bail!("Unknown config key: {}. Valid keys: default_format, default_llm_provider, default_llm_model, default_vector_db, api_url, api_token, verbose, color", key),
        }

        config.save()?;
        println!("✓ Set {} = {}", key, value);

        Ok(())
    }

    /// Unset a configuration value (reset to default)
    pub fn unset(key: &str) -> Result<()> {
        let mut config = Self::load()?;

        match key {
            "default_format" => config.default_format = default_format(),
            "default_llm_provider" => config.default_llm_provider = None,
            "default_llm_model" => config.default_llm_model = None,
            "default_vector_db" => config.default_vector_db = None,
            "api_url" => config.api_url = None,
            "api_token" => config.api_token = None,
            "verbose" => config.verbose = false,
            "color" => config.color = default_color(),
            _ => anyhow::bail!("Unknown config key: {}", key),
        }

        config.save()?;
        println!("✓ Unset {}", key);

        Ok(())
    }
}
