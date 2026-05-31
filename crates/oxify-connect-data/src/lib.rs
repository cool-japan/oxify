//! SaaS data connectors for OxiFY.
//!
//! Provides trait abstractions and provider implementations for structured
//! data services:
//!
//! * **Google Sheets** (feature `google-sheets`) — spreadsheet read/write via
//!   the Google Sheets API v4.
//! * **Airtable** (feature `airtable`) — table record CRUD via the Airtable
//!   REST API.
//! * **Notion** (feature `notion`) — page/database read/write via the Notion
//!   API v1.

pub mod error;

#[cfg(feature = "notion")]
pub mod knowledge;
#[cfg(feature = "google-sheets")]
pub mod spreadsheet;
#[cfg(feature = "airtable")]
pub mod table;

pub use error::{DataError, Result};

#[cfg(feature = "notion")]
pub use knowledge::notion::{NotionConfig, NotionProvider};
#[cfg(feature = "google-sheets")]
pub use spreadsheet::google_sheets::{GoogleSheetsConfig, GoogleSheetsProvider};
#[cfg(feature = "airtable")]
pub use table::airtable::{AirtableConfig, AirtableProvider};
