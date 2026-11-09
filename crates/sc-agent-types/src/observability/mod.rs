//! Session and general log ports (feature `obs`, ADR-SAG-0016 r-1 amendment;
//! general log: ADR-SAG-0013 amendment 2026-11-09).
//!
//! This crate links no `sc-observability*` crate. Composition roots adapt
//! their loggers to [`v2::SessionLog`] and [`v2::GeneralLog`].

pub mod v2;
