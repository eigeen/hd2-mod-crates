//! Async byte-source abstraction for archive data.
//!
//! Used by Mode A in two driver flavors:
//! - CLI: [`NativeDataSource`] over `std::fs`, returns immediate-ready futures.
//! - Browser: a JS-callback-backed `DataSource` defined in `hd2-migrator-wasm`,
//!   driven by `wasm-bindgen-futures`.
//!
//! Future objects are boxed and not `Send` to keep the wasm path flexible
//! (`js_sys::Function` is `!Send`); the native driver does not share these
//! futures across threads.

pub mod bundle_async;
pub mod native;

pub use bundle_async::BundleSlicer;
pub use native::NativeDataSource;

use std::future::Future;
use std::pin::Pin;

pub type IoFuture<'a, T> = Pin<Box<dyn Future<Output = crate::Result<T>> + 'a>>;

pub trait DataSource {
    fn read_full<'a>(&'a self, path: &'a str) -> IoFuture<'a, Vec<u8>>;
    fn read_range<'a>(&'a self, path: &'a str, offset: u64, len: u64) -> IoFuture<'a, Vec<u8>>;
    fn exists<'a>(&'a self, path: &'a str) -> IoFuture<'a, bool>;
    fn list_bundle_chunks<'a>(&'a self) -> IoFuture<'a, Vec<String>>;
}
