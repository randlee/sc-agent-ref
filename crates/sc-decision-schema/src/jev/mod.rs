//! TypeSafe Jev API types, one versioned module per API shape (ADR-SAG-0030).
//!
//! These are data: serde request and response structs for `POST
//! /v1/systemone`, as documented by TypeSafe. `sc-jev` owns every operation
//! (endpoints, building and sending the body, mapping replies;
//! ADR-SAG-0012). [`v1`] is the only version.

pub mod v1;
