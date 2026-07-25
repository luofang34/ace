pub(crate) mod aerodynamics;
pub(crate) mod capabilities;
pub(crate) mod content_identity;
pub(crate) mod diagnostic;
#[allow(
    dead_code,
    reason = "evidence schemas are an adapter-independent persistence boundary"
)]
pub(crate) mod evidence;
pub(crate) mod presentation;
pub(crate) mod propulsion;
pub(crate) mod quantity;
pub(crate) mod result;
pub(crate) mod schema;
#[allow(
    dead_code,
    reason = "study identity is part of the portable schema and archive contract"
)]
pub(crate) mod study;
pub(crate) mod topology;
pub(crate) mod validity;
pub(crate) mod warning;
