pub(crate) const ERROR_QUERY_PARAM: &str = "__error";
pub(crate) const ERROR_HEADER_NAME: &str = "x-api-faker-error";
pub(crate) const FLAGS_QUERY_PARAM: &str = "__flags";
pub(crate) const FLAGS_HEADER_NAME: &str = "x-api-faker-flags";

mod extractors;
mod handlers;
mod openapi;
mod router;
mod state;

pub use router::build_router;
pub use state::AppState;
