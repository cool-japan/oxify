//! Error types for the UI layer

use askama::Template;
use axum::{
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};
use thiserror::Error;

/// UI-specific errors
#[derive(Error, Debug)]
pub enum UiError {
    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Unauthorized")]
    Unauthorized,

    #[error("Forbidden")]
    Forbidden,

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Template error: {0}")]
    Template(String),

    #[error("API error: {0}")]
    Api(String),
}

/// Error page template
#[derive(Template)]
#[template(
    source = r#"<!DOCTYPE html>
<html lang="en" class="h-full">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>{{ title }} - OxiFY</title>
  <script src="https://cdn.tailwindcss.com"></script>
</head>
<body class="h-full bg-gray-50 dark:bg-gray-900">
  <div class="min-h-full flex flex-col justify-center py-12 sm:px-6 lg:px-8">
    <div class="sm:mx-auto sm:w-full sm:max-w-md">
      <div class="text-center">
        <h1 class="text-9xl font-bold text-gray-200 dark:text-gray-700">{{ status_code }}</h1>
        <h2 class="mt-4 text-3xl font-bold text-gray-900 dark:text-white">{{ title }}</h2>
        <p class="mt-2 text-base text-gray-500 dark:text-gray-400">{{ message }}</p>
        <div class="mt-6">
          <a href="/" class="inline-flex items-center px-4 py-2 border border-transparent text-sm font-medium rounded-md text-white bg-primary-600 hover:bg-primary-700 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-primary-500">
            <svg class="-ml-1 mr-2 h-5 w-5" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor">
              <path stroke-linecap="round" stroke-linejoin="round" d="M2.25 12l8.954-8.955c.44-.439 1.152-.439 1.591 0L21.75 12M4.5 9.75v10.125c0 .621.504 1.125 1.125 1.125H9.75v-4.875c0-.621.504-1.125 1.125-1.125h2.25c.621 0 1.125.504 1.125 1.125V21h4.125c.621 0 1.125-.504 1.125-1.125V9.75M8.25 21h8.25" />
            </svg>
            Go to Dashboard
          </a>
          <button onclick="history.back()" class="ml-3 inline-flex items-center px-4 py-2 border border-gray-300 dark:border-gray-600 text-sm font-medium rounded-md text-gray-700 dark:text-gray-200 bg-white dark:bg-gray-800 hover:bg-gray-50 dark:hover:bg-gray-700 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-primary-500">
            <svg class="-ml-1 mr-2 h-5 w-5" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor">
              <path stroke-linecap="round" stroke-linejoin="round" d="M9 15L3 9m0 0l6-6M3 9h12a6 6 0 010 12h-3" />
            </svg>
            Go Back
          </button>
        </div>
      </div>
    </div>
  </div>
</body>
</html>"#,
    ext = "html"
)]
struct ErrorPageTemplate {
    title: String,
    message: String,
    status_code: u16,
}

impl IntoResponse for UiError {
    fn into_response(self) -> Response {
        let (status, title, message) = match &self {
            UiError::NotFound(msg) => (StatusCode::NOT_FOUND, "Not Found", msg.clone()),
            UiError::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "Unauthorized",
                "You need to sign in to access this resource.".to_string(),
            ),
            UiError::Forbidden => (
                StatusCode::FORBIDDEN,
                "Forbidden",
                "You don't have permission to access this resource.".to_string(),
            ),
            UiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, "Bad Request", msg.clone()),
            UiError::Internal(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal Server Error",
                msg.clone(),
            ),
            UiError::Template(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Template Error",
                msg.clone(),
            ),
            UiError::Api(msg) => (StatusCode::BAD_GATEWAY, "API Error", msg.clone()),
        };

        let template = ErrorPageTemplate {
            title: title.to_string(),
            message,
            status_code: status.as_u16(),
        };

        match template.render() {
            Ok(html) => (status, Html(html)).into_response(),
            Err(_) => {
                // Fallback if template rendering fails
                (status, format!("{}: {}", title, self)).into_response()
            }
        }
    }
}

impl From<askama::Error> for UiError {
    fn from(err: askama::Error) -> Self {
        UiError::Template(err.to_string())
    }
}

impl From<crate::api::ApiError> for UiError {
    fn from(err: crate::api::ApiError) -> Self {
        match err {
            crate::api::ApiError::NotFound(msg) => UiError::NotFound(msg),
            crate::api::ApiError::Unauthorized => UiError::Unauthorized,
            crate::api::ApiError::Forbidden => UiError::Forbidden,
            _ => UiError::Api(err.to_string()),
        }
    }
}

/// Generate an HTMX-compatible error toast HTML
pub fn error_toast_html(message: &str) -> String {
    format!(
        r#"<div class="bg-red-100 dark:bg-red-900 border-l-4 border-red-500 text-red-700 dark:text-red-200 p-4 rounded shadow-lg" role="alert" x-data="{{ show: true }}" x-show="show" x-init="setTimeout(() => show = false, 5000)" x-transition>
  <div class="flex">
    <div class="flex-shrink-0">
      <svg class="h-5 w-5 text-red-500" viewBox="0 0 20 20" fill="currentColor">
        <path fill-rule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zM8.28 7.22a.75.75 0 00-1.06 1.06L8.94 10l-1.72 1.72a.75.75 0 101.06 1.06L10 11.06l1.72 1.72a.75.75 0 101.06-1.06L11.06 10l1.72-1.72a.75.75 0 00-1.06-1.06L10 8.94 8.28 7.22z" clip-rule="evenodd" />
      </svg>
    </div>
    <div class="ml-3">
      <p class="text-sm font-medium">{}</p>
    </div>
    <div class="ml-auto pl-3">
      <button @click="show = false" class="inline-flex text-red-500 hover:text-red-700">
        <svg class="h-5 w-5" viewBox="0 0 20 20" fill="currentColor">
          <path d="M6.28 5.22a.75.75 0 00-1.06 1.06L8.94 10l-3.72 3.72a.75.75 0 101.06 1.06L10 11.06l3.72 3.72a.75.75 0 101.06-1.06L11.06 10l3.72-3.72a.75.75 0 00-1.06-1.06L10 8.94 6.28 5.22z" />
        </svg>
      </button>
    </div>
  </div>
</div>"#,
        html_escape(message)
    )
}

/// Generate an HTMX-compatible success toast HTML
pub fn success_toast_html(message: &str) -> String {
    format!(
        r#"<div class="bg-green-100 dark:bg-green-900 border-l-4 border-green-500 text-green-700 dark:text-green-200 p-4 rounded shadow-lg" role="alert" x-data="{{ show: true }}" x-show="show" x-init="setTimeout(() => show = false, 5000)" x-transition>
  <div class="flex">
    <div class="flex-shrink-0">
      <svg class="h-5 w-5 text-green-500" viewBox="0 0 20 20" fill="currentColor">
        <path fill-rule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zm3.857-9.809a.75.75 0 00-1.214-.882l-3.483 4.79-1.88-1.88a.75.75 0 10-1.06 1.061l2.5 2.5a.75.75 0 001.137-.089l4-5.5z" clip-rule="evenodd" />
      </svg>
    </div>
    <div class="ml-3">
      <p class="text-sm font-medium">{}</p>
    </div>
    <div class="ml-auto pl-3">
      <button @click="show = false" class="inline-flex text-green-500 hover:text-green-700">
        <svg class="h-5 w-5" viewBox="0 0 20 20" fill="currentColor">
          <path d="M6.28 5.22a.75.75 0 00-1.06 1.06L8.94 10l-3.72 3.72a.75.75 0 101.06 1.06L10 11.06l3.72 3.72a.75.75 0 101.06-1.06L11.06 10l3.72-3.72a.75.75 0 00-1.06-1.06L10 8.94 6.28 5.22z" />
        </svg>
      </button>
    </div>
  </div>
</div>"#,
        html_escape(message)
    )
}

/// Simple HTML escaping for messages
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
