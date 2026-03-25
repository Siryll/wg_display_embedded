//! Common data models used by both frontend and embedded_app.
//!
//! This crate is `no_std` by default. Enable the `yew` feature to also yew related parts required by the frontend.
#![cfg_attr(not(feature = "yew"), no_std)]

extern crate alloc;

pub mod models;
pub mod widget_store_item;
